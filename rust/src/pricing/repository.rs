use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
    time::{Duration as StdDuration, Instant},
};

use async_trait::async_trait;
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use super::{
    provider::{ProviderError, ResolutionStatus},
    snapshot::{AwsPriceSnapshot, AzurePriceSnapshot, SnapshotMetadata},
};

const HOT_CACHE_MAX_AGE: Duration = Duration::minutes(15);
const FRESH_MAX_AGE: Duration = Duration::hours(24);
const USABLE_MAX_AGE: Duration = Duration::days(7);
const HOT_ENTRY_MAX_AGE: StdDuration = StdDuration::from_secs(15 * 60);
const MAX_HOT_SNAPSHOTS_PER_PROVIDER: usize = 256;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SnapshotRepositoryError {
    #[error("pricing snapshot repository is unavailable")]
    Unavailable,
    #[error("stored pricing snapshot is invalid")]
    InvalidData,
    #[error("pricing snapshot exceeds the persistence limit")]
    PayloadTooLarge,
}

#[async_trait]
pub trait DurableSnapshotRepository: Send + Sync {
    async fn put_aws(
        &self,
        snapshot: &AwsPriceSnapshot,
    ) -> Result<AwsPriceSnapshot, SnapshotRepositoryError>;

    async fn put_azure(&self, snapshot: &AzurePriceSnapshot)
    -> Result<(), SnapshotRepositoryError>;

    async fn get_aws(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<AwsPriceSnapshot>, SnapshotRepositoryError>;

    async fn get_azure(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<AzurePriceSnapshot>, SnapshotRepositoryError>;

    async fn find_aws(
        &self,
        currency: &str,
        source_region: &str,
    ) -> Result<Option<AwsPriceSnapshot>, SnapshotRepositoryError>;

    async fn find_azure(
        &self,
        currency: &str,
        target_region: &str,
    ) -> Result<Option<AzurePriceSnapshot>, SnapshotRepositoryError>;

    async fn list_latest_aws(&self) -> Result<Vec<AwsPriceSnapshot>, SnapshotRepositoryError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshLeaseDecision {
    Acquired,
    Pending,
    Succeeded(String),
    Failed(ProviderError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshLeaseOutcome {
    Succeeded(String),
    Failed(ProviderError),
}

#[async_trait]
pub trait RefreshLeaseRepository: Send + Sync {
    async fn claim_refresh_lease(
        &self,
        cache_key_sha256: &str,
        owner_token: &str,
        request_started_at: &str,
    ) -> Result<RefreshLeaseDecision, SnapshotRepositoryError>;

    async fn publish_refresh_lease(
        &self,
        cache_key_sha256: &str,
        owner_token: &str,
        outcome: &RefreshLeaseOutcome,
    ) -> Result<(), SnapshotRepositoryError>;
}

#[derive(Clone, Default)]
pub struct InMemorySnapshotRepository {
    aws: Arc<RwLock<HashMap<String, HotSnapshot<AwsPriceSnapshot>>>>,
    azure: Arc<RwLock<HashMap<String, HotSnapshot<AzurePriceSnapshot>>>>,
}

#[derive(Clone)]
struct HotSnapshot<T> {
    snapshot: Arc<T>,
    inserted_at: Instant,
}

impl InMemorySnapshotRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put_aws(&self, snapshot: AwsPriceSnapshot) -> Result<(), SnapshotRepositoryError> {
        let id = snapshot.metadata.snapshot_id.clone();
        let mut snapshots = self
            .aws
            .write()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?;
        insert_hot(&mut snapshots, id, snapshot);
        Ok(())
    }

    pub fn put_azure(&self, snapshot: AzurePriceSnapshot) -> Result<(), SnapshotRepositoryError> {
        let id = snapshot.metadata.snapshot_id.clone();
        let mut snapshots = self
            .azure
            .write()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?;
        insert_hot(&mut snapshots, id, snapshot);
        Ok(())
    }

    pub fn get_aws(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, SnapshotRepositoryError> {
        let snapshot = self
            .aws
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .get(snapshot_id)
            .map(|entry| Arc::clone(&entry.snapshot));
        Ok(snapshot.and_then(|snapshot| classify_aws(snapshot, OffsetDateTime::now_utc())))
    }

    pub fn get_aws_hot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, SnapshotRepositoryError> {
        let snapshot = self
            .aws
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .get(snapshot_id)
            .filter(|entry| entry.inserted_at.elapsed() <= HOT_ENTRY_MAX_AGE)
            .map(|entry| Arc::clone(&entry.snapshot));
        Ok(snapshot.and_then(|snapshot| classify_aws(snapshot, OffsetDateTime::now_utc())))
    }

    pub fn get_azure(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, SnapshotRepositoryError> {
        let snapshot = self
            .azure
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .get(snapshot_id)
            .map(|entry| Arc::clone(&entry.snapshot));
        Ok(snapshot.and_then(|snapshot| classify_azure(snapshot, OffsetDateTime::now_utc())))
    }

    pub fn get_azure_hot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, SnapshotRepositoryError> {
        let snapshot = self
            .azure
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .get(snapshot_id)
            .filter(|entry| entry.inserted_at.elapsed() <= HOT_ENTRY_MAX_AGE)
            .map(|entry| Arc::clone(&entry.snapshot));
        Ok(snapshot.and_then(|snapshot| classify_azure(snapshot, OffsetDateTime::now_utc())))
    }

    pub fn find_aws(
        &self,
        currency: &str,
        source_region: &str,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, SnapshotRepositoryError> {
        let snapshot = self
            .aws
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .values()
            .map(|entry| &entry.snapshot)
            .filter(|snapshot| snapshot.matches_scope(currency, source_region))
            .max_by(|left, right| left.metadata.retrieved_at.cmp(&right.metadata.retrieved_at))
            .map(Arc::clone);
        Ok(snapshot.and_then(|snapshot| classify_aws(snapshot, OffsetDateTime::now_utc())))
    }

    pub fn find_aws_hot(
        &self,
        currency: &str,
        source_region: &str,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, SnapshotRepositoryError> {
        let snapshot = self
            .aws
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .values()
            .filter(|entry| entry.inserted_at.elapsed() <= HOT_ENTRY_MAX_AGE)
            .map(|entry| &entry.snapshot)
            .filter(|snapshot| snapshot.matches_scope(currency, source_region))
            .max_by(|left, right| left.metadata.retrieved_at.cmp(&right.metadata.retrieved_at))
            .map(Arc::clone);
        Ok(snapshot.and_then(|snapshot| classify_aws(snapshot, OffsetDateTime::now_utc())))
    }

    pub fn find_azure(
        &self,
        currency: &str,
        target_region: &str,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, SnapshotRepositoryError> {
        let snapshot = self
            .azure
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .values()
            .map(|entry| &entry.snapshot)
            .filter(|snapshot| snapshot.matches_scope(currency, target_region))
            .max_by(|left, right| left.metadata.retrieved_at.cmp(&right.metadata.retrieved_at))
            .map(Arc::clone);
        Ok(snapshot.and_then(|snapshot| classify_azure(snapshot, OffsetDateTime::now_utc())))
    }

    pub fn find_azure_hot(
        &self,
        currency: &str,
        target_region: &str,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, SnapshotRepositoryError> {
        let snapshot = self
            .azure
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .values()
            .filter(|entry| entry.inserted_at.elapsed() <= HOT_ENTRY_MAX_AGE)
            .map(|entry| &entry.snapshot)
            .filter(|snapshot| snapshot.matches_scope(currency, target_region))
            .max_by(|left, right| left.metadata.retrieved_at.cmp(&right.metadata.retrieved_at))
            .map(Arc::clone);
        Ok(snapshot.and_then(|snapshot| classify_azure(snapshot, OffsetDateTime::now_utc())))
    }

    pub fn list_latest_aws(&self) -> Result<Vec<Arc<AwsPriceSnapshot>>, SnapshotRepositoryError> {
        let snapshots = self
            .aws
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?;
        let mut latest = BTreeMap::<(String, String), Arc<AwsPriceSnapshot>>::new();
        for snapshot in snapshots.values().map(|entry| &entry.snapshot) {
            let key = (
                snapshot.metadata.currency.clone(),
                snapshot.source_region.clone(),
            );
            match latest.get(&key) {
                Some(current)
                    if current.metadata.retrieved_at >= snapshot.metadata.retrieved_at => {}
                _ => {
                    latest.insert(key, Arc::clone(snapshot));
                }
            }
        }
        let now = OffsetDateTime::now_utc();
        Ok(latest
            .into_values()
            .filter_map(|snapshot| classify_aws(snapshot, now))
            .collect())
    }
}

fn insert_hot<T>(snapshots: &mut HashMap<String, HotSnapshot<T>>, id: String, snapshot: T) {
    if !snapshots.contains_key(&id)
        && snapshots.len() >= MAX_HOT_SNAPSHOTS_PER_PROVIDER
        && let Some(oldest_id) = snapshots
            .iter()
            .min_by_key(|(_, entry)| entry.inserted_at)
            .map(|(id, _)| id.clone())
    {
        snapshots.remove(&oldest_id);
    }
    snapshots.insert(
        id,
        HotSnapshot {
            snapshot: Arc::new(snapshot),
            inserted_at: Instant::now(),
        },
    );
}

fn classify_aws(
    snapshot: Arc<AwsPriceSnapshot>,
    now: OffsetDateTime,
) -> Option<Arc<AwsPriceSnapshot>> {
    let status = effective_status(&snapshot.metadata, now)?;
    if snapshot.metadata.status == status {
        return Some(snapshot);
    }
    let mut classified = (*snapshot).clone();
    classified.metadata.status = status;
    Some(Arc::new(classified))
}

fn classify_azure(
    snapshot: Arc<AzurePriceSnapshot>,
    now: OffsetDateTime,
) -> Option<Arc<AzurePriceSnapshot>> {
    let status = effective_status(&snapshot.metadata, now)?;
    if snapshot.metadata.status == status {
        return Some(snapshot);
    }
    let mut classified = (*snapshot).clone();
    classified.metadata.status = status;
    Some(Arc::new(classified))
}

fn effective_status(metadata: &SnapshotMetadata, now: OffsetDateTime) -> Option<ResolutionStatus> {
    let retrieved_at = OffsetDateTime::parse(&metadata.retrieved_at, &Rfc3339).ok()?;
    let age = now - retrieved_at;
    if age.is_negative() || age > USABLE_MAX_AGE {
        None
    } else if age > FRESH_MAX_AGE {
        Some(ResolutionStatus::Stale)
    } else if age > HOT_CACHE_MAX_AGE || metadata.status != ResolutionStatus::Fresh {
        Some(ResolutionStatus::Cached)
    } else {
        Some(ResolutionStatus::Fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::{
        provider::ResolutionStatus,
        snapshot::{AwsPriceSnapshot, SnapshotCreationMetadata},
    };

    #[test]
    fn snapshots_are_loaded_only_by_server_issued_id() {
        let repository = InMemorySnapshotRepository::new();
        let snapshot = AwsPriceSnapshot::create(
            SnapshotCreationMetadata {
                status: ResolutionStatus::Fresh,
                retrieved_at: crate::pricing::snapshot::utc_now_rfc3339()
                    .expect("current retrieval time"),
                source_published_at: None,
                currency: "USD".to_owned(),
                source_urls: vec!["https://example.invalid/synthetic-prices".to_owned()],
                parser_schema_version: "test-v1".to_owned(),
                warnings: Vec::new(),
            },
            "eu-west-1",
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .expect("synthetic snapshot");
        let snapshot_id = snapshot.metadata.snapshot_id.clone();

        repository.put_aws(snapshot).expect("store snapshot");

        assert!(
            repository
                .get_aws(&snapshot_id)
                .expect("load snapshot")
                .is_some()
        );
        assert!(
            repository
                .get_aws("aws-unknown")
                .expect("unknown lookup")
                .is_none()
        );
        assert!(
            repository
                .get_azure(&snapshot_id)
                .expect("provider-scoped lookup")
                .is_none()
        );
    }

    #[test]
    fn snapshot_status_follows_the_freshness_windows() {
        let now = OffsetDateTime::parse("2026-08-10T12:00:00Z", &Rfc3339).expect("test time");
        for (retrieved_at, original_status, expected) in [
            (
                "2026-08-10T11:50:00Z",
                ResolutionStatus::Fresh,
                Some(ResolutionStatus::Fresh),
            ),
            (
                "2026-08-10T11:50:00Z",
                ResolutionStatus::Cached,
                Some(ResolutionStatus::Cached),
            ),
            (
                "2026-08-10T11:00:00Z",
                ResolutionStatus::Fresh,
                Some(ResolutionStatus::Cached),
            ),
            (
                "2026-08-08T12:00:00Z",
                ResolutionStatus::Fresh,
                Some(ResolutionStatus::Stale),
            ),
            (
                "2026-08-03T12:00:00Z",
                ResolutionStatus::Fresh,
                Some(ResolutionStatus::Stale),
            ),
            ("2026-08-03T11:59:59Z", ResolutionStatus::Fresh, None),
            ("2026-08-10T12:01:00Z", ResolutionStatus::Fresh, None),
        ] {
            let snapshot = Arc::new(snapshot(retrieved_at, original_status));
            assert_eq!(
                classify_aws(snapshot, now).map(|snapshot| snapshot.metadata.status),
                expected
            );
        }
    }

    fn snapshot(retrieved_at: &str, status: ResolutionStatus) -> AwsPriceSnapshot {
        AwsPriceSnapshot::create(
            SnapshotCreationMetadata {
                status,
                retrieved_at: retrieved_at.to_owned(),
                source_published_at: None,
                currency: "USD".to_owned(),
                source_urls: vec!["https://example.invalid/synthetic-prices".to_owned()],
                parser_schema_version: "test-v1".to_owned(),
                warnings: Vec::new(),
            },
            "eu-west-1",
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .expect("synthetic snapshot")
    }
}
