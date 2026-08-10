use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
};

use thiserror::Error;

use super::snapshot::{AwsPriceSnapshot, AzurePriceSnapshot};

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SnapshotRepositoryError {
    #[error("pricing snapshot repository is unavailable")]
    Unavailable,
}

#[derive(Clone, Default)]
pub struct InMemorySnapshotRepository {
    aws: Arc<RwLock<HashMap<String, Arc<AwsPriceSnapshot>>>>,
    azure: Arc<RwLock<HashMap<String, Arc<AzurePriceSnapshot>>>>,
}

impl InMemorySnapshotRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put_aws(&self, snapshot: AwsPriceSnapshot) -> Result<(), SnapshotRepositoryError> {
        let id = snapshot.metadata.snapshot_id.clone();
        self.aws
            .write()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .insert(id, Arc::new(snapshot));
        Ok(())
    }

    pub fn put_azure(&self, snapshot: AzurePriceSnapshot) -> Result<(), SnapshotRepositoryError> {
        let id = snapshot.metadata.snapshot_id.clone();
        self.azure
            .write()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .insert(id, Arc::new(snapshot));
        Ok(())
    }

    pub fn get_aws(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, SnapshotRepositoryError> {
        Ok(self
            .aws
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .get(snapshot_id)
            .cloned())
    }

    pub fn get_azure(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, SnapshotRepositoryError> {
        Ok(self
            .azure
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .get(snapshot_id)
            .cloned())
    }

    pub fn find_aws(
        &self,
        currency: &str,
        source_region: &str,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, SnapshotRepositoryError> {
        Ok(self
            .aws
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .values()
            .filter(|snapshot| snapshot.matches_scope(currency, source_region))
            .max_by(|left, right| left.metadata.retrieved_at.cmp(&right.metadata.retrieved_at))
            .cloned())
    }

    pub fn find_azure(
        &self,
        currency: &str,
        target_region: &str,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, SnapshotRepositoryError> {
        Ok(self
            .azure
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?
            .values()
            .filter(|snapshot| snapshot.matches_scope(currency, target_region))
            .max_by(|left, right| left.metadata.retrieved_at.cmp(&right.metadata.retrieved_at))
            .cloned())
    }

    pub fn list_latest_aws(&self) -> Result<Vec<Arc<AwsPriceSnapshot>>, SnapshotRepositoryError> {
        let snapshots = self
            .aws
            .read()
            .map_err(|_| SnapshotRepositoryError::Unavailable)?;
        let mut latest = BTreeMap::<(String, String), Arc<AwsPriceSnapshot>>::new();
        for snapshot in snapshots.values() {
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
        Ok(latest.into_values().collect())
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
                retrieved_at: "2026-07-31T00:00:00Z".to_owned(),
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
}
