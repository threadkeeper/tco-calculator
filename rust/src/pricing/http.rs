use std::time::{Duration, Instant};

use reqwest::{
    Client, StatusCode, Url,
    header::{ETAG, HeaderMap, LAST_MODIFIED, RETRY_AFTER},
    redirect::Policy,
};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc2822};

use crate::config::APP_VERSION;

use super::provider::ProviderError;

const ALLOWED_HOSTS: [&str; 4] = [
    "calculator.aws",
    "pricing.us-east-1.amazonaws.com",
    "prices.azure.com",
    "azure.microsoft.com",
];
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RESOLUTION_BUDGET: Duration = Duration::from_secs(120);
const MAX_ATTEMPTS: u8 = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpPayload {
    pub source_url: String,
    pub source_version: Option<String>,
    pub effective_at: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Clone)]
pub struct PricingHttpClient {
    client: Client,
    max_response_bytes: usize,
}

enum AttemptFailure {
    Terminal(ProviderError),
    Temporary(Option<Duration>),
}

impl PricingHttpClient {
    pub fn new(max_response_bytes: usize) -> Result<Self, ProviderError> {
        if max_response_bytes == 0 {
            return Err(ProviderError::Unsupported);
        }
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .https_only(true)
            .no_proxy()
            .redirect(Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .user_agent(format!("azure-sql-tco/{APP_VERSION}"))
            .build()
            .map_err(|_| ProviderError::TemporarilyUnavailable)?;
        Ok(Self {
            client,
            max_response_bytes,
        })
    }

    pub async fn get(&self, source_url: &str) -> Result<HttpPayload, ProviderError> {
        let url = validate_url(source_url)?;
        let started_at = Instant::now();
        for attempt in 1..=MAX_ATTEMPTS {
            let remaining = RESOLUTION_BUDGET
                .checked_sub(started_at.elapsed())
                .filter(|remaining| !remaining.is_zero())
                .ok_or(ProviderError::TemporarilyUnavailable)?;
            match self
                .send_once(url.clone(), remaining.min(REQUEST_TIMEOUT))
                .await
            {
                Ok(payload) => return Ok(payload),
                Err(AttemptFailure::Terminal(error)) => return Err(error),
                Err(AttemptFailure::Temporary(retry_after)) if attempt < MAX_ATTEMPTS => {
                    let elapsed = started_at.elapsed();
                    let remaining = RESOLUTION_BUDGET
                        .checked_sub(elapsed)
                        .ok_or(ProviderError::TemporarilyUnavailable)?;
                    let delay = retry_after.unwrap_or_else(|| retry_delay(&url, attempt));
                    if delay >= remaining {
                        return Err(ProviderError::TemporarilyUnavailable);
                    }
                    tokio::time::sleep(delay).await;
                }
                Err(AttemptFailure::Temporary(_)) => {
                    return Err(ProviderError::TemporarilyUnavailable);
                }
            }
        }
        Err(ProviderError::TemporarilyUnavailable)
    }

    async fn send_once(
        &self,
        url: Url,
        request_timeout: Duration,
    ) -> Result<HttpPayload, AttemptFailure> {
        let mut response = self
            .client
            .get(url.clone())
            .timeout(request_timeout)
            .send()
            .await
            .map_err(|_| AttemptFailure::Temporary(None))?;
        let status = response.status();
        if !status.is_success() {
            return Err(classify_status(status, response.headers()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > self.max_response_bytes as u64)
        {
            return Err(AttemptFailure::Terminal(ProviderError::SchemaChanged));
        }

        let source_version = header_value(response.headers(), ETAG.as_str());
        let effective_at = header_value(response.headers(), LAST_MODIFIED.as_str());
        let mut body = Vec::with_capacity(
            response
                .content_length()
                .unwrap_or_default()
                .min(self.max_response_bytes as u64) as usize,
        );
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AttemptFailure::Temporary(None))?
        {
            append_chunk(&mut body, &chunk, self.max_response_bytes)
                .map_err(AttemptFailure::Terminal)?;
        }
        Ok(HttpPayload {
            source_url: url.to_string(),
            source_version,
            effective_at,
            body,
        })
    }
}

fn validate_url(source_url: &str) -> Result<Url, ProviderError> {
    let url = Url::parse(source_url).map_err(|_| ProviderError::Unsupported)?;
    let allowed = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| ALLOWED_HOSTS.contains(&host));
    if allowed {
        Ok(url)
    } else {
        Err(ProviderError::Unsupported)
    }
}

fn classify_status(status: StatusCode, headers: &HeaderMap) -> AttemptFailure {
    if status == StatusCode::NOT_FOUND {
        AttemptFailure::Terminal(ProviderError::NotFound)
    } else if status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
    {
        AttemptFailure::Temporary(parse_retry_after(headers, OffsetDateTime::now_utc()))
    } else {
        AttemptFailure::Terminal(ProviderError::Unsupported)
    }
}

fn parse_retry_after(headers: &HeaderMap, now: OffsetDateTime) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let retry_at = OffsetDateTime::parse(value, &Rfc2822).ok()?;
    Some(Duration::from_secs(
        (retry_at - now).whole_seconds().max(0) as u64
    ))
}

fn retry_delay(url: &Url, attempt: u8) -> Duration {
    let digest = Sha256::digest(format!("{url}|{attempt}").as_bytes());
    let jitter = u16::from_be_bytes([digest[0], digest[1]]) % 251;
    Duration::from_millis(250 * (1_u64 << (attempt - 1)) + u64::from(jitter))
}

fn append_chunk(
    body: &mut Vec<u8>,
    chunk: &[u8],
    max_response_bytes: usize,
) -> Result<(), ProviderError> {
    if chunk.len() > max_response_bytes.saturating_sub(body.len()) {
        return Err(ProviderError::SchemaChanged);
    }
    body.extend_from_slice(chunk);
    Ok(())
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;
    use time::{Date, Month, Time};

    use super::*;

    #[test]
    fn pricing_urls_are_limited_to_disclosed_https_hosts() {
        for url in [
            "https://calculator.aws/pricing/index.json",
            "https://pricing.us-east-1.amazonaws.com/offers/index.json",
            "https://prices.azure.com/api/retail/prices?$filter=test",
            "https://azure.microsoft.com/api/v3/pricing/test",
        ] {
            assert!(validate_url(url).is_ok(), "{url}");
        }
        for url in [
            "http://prices.azure.com/api/retail/prices",
            "https://prices.azure.com.example.invalid/api",
            "https://user@prices.azure.com/api",
            "https://prices.azure.com:444/api",
            "https://example.invalid/api",
        ] {
            assert_eq!(validate_url(url), Err(ProviderError::Unsupported), "{url}");
        }
    }

    #[test]
    fn retry_after_supports_seconds_and_http_dates() {
        let now = OffsetDateTime::new_utc(
            Date::from_calendar_date(2026, Month::August, 10).expect("date"),
            Time::from_hms(12, 0, 0).expect("time"),
        );
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("17"));
        assert_eq!(
            parse_retry_after(&headers, now),
            Some(Duration::from_secs(17))
        );

        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Mon, 10 Aug 2026 12:00:31 GMT"),
        );
        assert_eq!(
            parse_retry_after(&headers, now),
            Some(Duration::from_secs(31))
        );
    }

    #[test]
    fn response_body_limit_is_enforced_incrementally() {
        let mut body = vec![1, 2];
        append_chunk(&mut body, &[3, 4], 4).expect("within limit");
        assert_eq!(body, vec![1, 2, 3, 4]);
        assert_eq!(
            append_chunk(&mut body, &[5], 4),
            Err(ProviderError::SchemaChanged)
        );
    }

    #[test]
    fn only_transient_statuses_are_retryable() {
        let headers = HeaderMap::new();
        assert!(matches!(
            classify_status(StatusCode::TOO_MANY_REQUESTS, &headers),
            AttemptFailure::Temporary(_)
        ));
        assert!(matches!(
            classify_status(StatusCode::BAD_GATEWAY, &headers),
            AttemptFailure::Temporary(_)
        ));
        assert!(matches!(
            classify_status(StatusCode::NOT_FOUND, &headers),
            AttemptFailure::Terminal(ProviderError::NotFound)
        ));
        assert!(matches!(
            classify_status(StatusCode::BAD_REQUEST, &headers),
            AttemptFailure::Terminal(ProviderError::Unsupported)
        ));
    }
}
