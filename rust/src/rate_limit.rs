use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderMap, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

use crate::{
    auth::resolve_principal,
    config::{AppEnvironment, Config},
    problem::Problem,
    state::AppState,
};

const MAX_TRACKED_KEYS: usize = 10_000;

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

impl TokenBucket {
    pub fn new(capacity: u32, period: Duration) -> Self {
        Self {
            capacity: f64::from(capacity),
            refill_per_second: f64::from(capacity) / period.as_secs_f64(),
            stale_after: period.saturating_mul(2),
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn check(&self, key: &str) -> Result<Option<u64>, ()> {
        let digest: [u8; 32] = Sha256::digest(key.as_bytes()).into();
        let now = Instant::now();
        let mut entries = self.entries.lock().map_err(|_| ())?;
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
            Err(()) => return Problem::internal(request.uri().path()).into_response(),
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
    match state.refresh_rate_limit.check(&key) {
        Ok(Some(retry_after)) => {
            Problem::rate_limited(request.uri().path(), retry_after).into_response()
        }
        Err(()) => Problem::internal(request.uri().path()).into_response(),
        Ok(None) => next.run(request).await,
    }
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
    use std::time::Duration;

    use super::TokenBucket;

    #[test]
    fn token_bucket_rejects_after_capacity_is_consumed() {
        let bucket = TokenBucket::new(2, Duration::from_secs(60));

        assert_eq!(bucket.check("client").expect("first request"), None);
        assert_eq!(bucket.check("client").expect("second request"), None);
        assert!(bucket.check("client").expect("limited request").is_some());
        assert_eq!(bucket.check("other client").expect("independent key"), None);
    }
}
