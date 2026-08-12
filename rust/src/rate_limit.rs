use std::{
    collections::HashMap,
    fmt::Write,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderMap, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    auth::resolve_principal,
    config::{AppEnvironment, Config},
    problem::Problem,
    state::AppState,
};

const MAX_TRACKED_KEYS: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshQuotaDecision {
    Allowed,
    Limited { retry_after_seconds: u64 },
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RefreshQuotaError {
    #[error("refresh quota repository is unavailable")]
    Unavailable,
    #[error("stored refresh quota is invalid")]
    InvalidData,
}

#[async_trait]
pub trait RefreshQuotaRepository: Send + Sync {
    async fn consume_refresh_quota(
        &self,
        identity_sha256: &str,
        operation_token: &str,
        limit: u32,
    ) -> Result<RefreshQuotaDecision, RefreshQuotaError>;
}

#[derive(Clone)]
pub struct RefreshQuota {
    limit: u32,
    local: Option<TokenBucket>,
    distributed: Option<Arc<dyn RefreshQuotaRepository>>,
}

impl RefreshQuota {
    pub fn new(limit: u32, distributed: Option<Arc<dyn RefreshQuotaRepository>>) -> Self {
        let local = distributed
            .is_none()
            .then(|| TokenBucket::new(limit, Duration::from_secs(60 * 60)));
        Self {
            limit,
            local,
            distributed,
        }
    }

    async fn check(&self, identity: &str) -> Result<Option<u64>, RefreshQuotaError> {
        if let Some(repository) = &self.distributed {
            let identity_sha256 = digest_hex(identity);
            let operation_token = Uuid::new_v4().to_string();
            return repository
                .consume_refresh_quota(&identity_sha256, &operation_token, self.limit)
                .await
                .map(|decision| match decision {
                    RefreshQuotaDecision::Allowed => None,
                    RefreshQuotaDecision::Limited {
                        retry_after_seconds,
                    } => Some(retry_after_seconds),
                });
        }
        self.local
            .as_ref()
            .ok_or(RefreshQuotaError::Unavailable)?
            .check(identity)
            .map_err(|_| RefreshQuotaError::Unavailable)
    }
}

#[derive(Clone)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_second: f64,
    stale_after: Duration,
    entries: Arc<Mutex<HashMap<[u8; 32], BucketEntry>>>,
}

#[derive(Clone, Copy)]
struct BucketEntry {
    tokens: f64,
    updated_at: Instant,
}

#[derive(Debug)]
pub(crate) struct TokenBucketError;

impl TokenBucket {
    pub fn new(capacity: u32, period: Duration) -> Self {
        Self {
            capacity: f64::from(capacity),
            refill_per_second: f64::from(capacity) / period.as_secs_f64(),
            stale_after: period.saturating_mul(2),
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn check(&self, key: &str) -> Result<Option<u64>, TokenBucketError> {
        let digest: [u8; 32] = Sha256::digest(key.as_bytes()).into();
        let now = Instant::now();
        let mut entries = self.entries.lock().map_err(|_| TokenBucketError)?;
        if !entries.contains_key(&digest) && entries.len() >= MAX_TRACKED_KEYS {
            entries.retain(|_, entry| now.duration_since(entry.updated_at) <= self.stale_after);
            if entries.len() >= MAX_TRACKED_KEYS {
                return Ok(Some(self.stale_after.as_secs().max(1)));
            }
        }

        let entry = entries.entry(digest).or_insert(BucketEntry {
            tokens: self.capacity,
            updated_at: now,
        });
        let elapsed = now.duration_since(entry.updated_at).as_secs_f64();
        entry.tokens = (entry.tokens + elapsed * self.refill_per_second).min(self.capacity);
        entry.updated_at = now;
        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            Ok(None)
        } else {
            let retry_after = ((1.0 - entry.tokens) / self.refill_per_second).ceil() as u64;
            Ok(Some(retry_after.max(1)))
        }
    }
}

pub async fn enforce_guest_quota(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let authenticated = resolve_principal(request.headers(), &state.config)
        .ok()
        .flatten()
        .is_some();
    if !authenticated {
        let key = requester_key(request.headers(), request.extensions(), &state.config);
        match state.guest_rate_limit.check(&key) {
            Ok(Some(retry_after)) => {
                return Problem::rate_limited(request.uri().path(), retry_after).into_response();
            }
            Err(TokenBucketError) => {
                return Problem::internal(request.uri().path()).into_response();
            }
            Ok(None) => {}
        }
    }
    next.run(request).await
}

pub async fn enforce_refresh_quota(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let key = resolve_principal(request.headers(), &state.config)
        .ok()
        .flatten()
        .map(|principal| format!("principal:{}", principal.owner_id()))
        .unwrap_or_else(|| requester_key(request.headers(), request.extensions(), &state.config));
    match state.refresh_rate_limit.check(&key).await {
        Ok(Some(retry_after)) => {
            Problem::rate_limited(request.uri().path(), retry_after).into_response()
        }
        Err(error) => {
            tracing::warn!(error = %error, "refresh quota check failed");
            Problem::internal(request.uri().path()).into_response()
        }
        Ok(None) => next.run(request).await,
    }
}

fn digest_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn requester_key(
    headers: &HeaderMap,
    extensions: &axum::http::Extensions,
    config: &Config,
) -> String {
    let forwarded_ip = (config.environment != AppEnvironment::Local)
        .then(|| headers.get("x-forwarded-for"))
        .flatten()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse::<IpAddr>().ok());
    let peer_ip = extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip());
    let ip = forwarded_ip
        .or(peer_ip)
        .map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    format!("ip:{ip}")
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::{
        RefreshQuota, RefreshQuotaDecision, RefreshQuotaError, RefreshQuotaRepository, TokenBucket,
    };

    struct RecordingQuotaRepository {
        calls: Mutex<Vec<(String, String, u32)>>,
        decision: RefreshQuotaDecision,
    }

    #[async_trait]
    impl RefreshQuotaRepository for RecordingQuotaRepository {
        async fn consume_refresh_quota(
            &self,
            identity_sha256: &str,
            operation_token: &str,
            limit: u32,
        ) -> Result<RefreshQuotaDecision, RefreshQuotaError> {
            self.calls.lock().expect("calls lock").push((
                identity_sha256.to_owned(),
                operation_token.to_owned(),
                limit,
            ));
            Ok(self.decision)
        }
    }

    #[test]
    fn token_bucket_rejects_after_capacity_is_consumed() {
        let bucket = TokenBucket::new(2, Duration::from_secs(60));

        assert_eq!(bucket.check("client").expect("first request"), None);
        assert_eq!(bucket.check("client").expect("second request"), None);
        assert!(bucket.check("client").expect("limited request").is_some());
        assert_eq!(bucket.check("other client").expect("independent key"), None);
    }

    #[tokio::test]
    async fn distributed_quota_sends_only_distinct_identity_digests() {
        let repository = Arc::new(RecordingQuotaRepository {
            calls: Mutex::new(Vec::new()),
            decision: RefreshQuotaDecision::Allowed,
        });
        let quota = RefreshQuota::new(6, Some(repository.clone()));

        assert_eq!(quota.check("principal:tenant:owner").await, Ok(None));
        assert_eq!(quota.check("ip:192.0.2.10").await, Ok(None));

        let calls = repository.calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0.len(), 64);
        assert_eq!(calls[1].0.len(), 64);
        assert_ne!(calls[0].0, calls[1].0);
        assert!(!calls[0].0.contains("tenant"));
        assert!(!calls[1].0.contains("192.0.2.10"));
        assert_eq!(calls[0].2, 6);
        assert_eq!(calls[1].2, 6);
        assert!(calls.iter().all(|call| Uuid::parse_str(&call.1).is_ok()));
    }

    #[tokio::test]
    async fn distributed_quota_preserves_retry_after() {
        let repository = Arc::new(RecordingQuotaRepository {
            calls: Mutex::new(Vec::new()),
            decision: RefreshQuotaDecision::Limited {
                retry_after_seconds: 3_599,
            },
        });
        let quota = RefreshQuota::new(6, Some(repository));

        assert_eq!(quota.check("ip:192.0.2.10").await, Ok(Some(3_599)));
    }
}
