use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pricing::{
    provider::Provider,
    snapshot::{
        AwsPriceSnapshot, AzurePriceSnapshot, SnapshotError, validate_stored_aws_snapshot,
        validate_stored_azure_snapshot,
    },
};

pub(crate) const PRICING_CACHE_CONTAINER_ID: &str = "pricing-cache";
pub(crate) const PRICING_CACHE_PARTITION: &str = "pricing";
pub(crate) const AWS_SNAPSHOT_DOCUMENT_TYPE: &str = "aws_price_snapshot";
pub(crate) const AZURE_SNAPSHOT_DOCUMENT_TYPE: &str = "azure_price_snapshot";

#[derive(Debug, Error)]
pub(crate) enum PricingCacheDocumentError {
    #[error("pricing cache document envelope does not match its snapshot")]
    EnvelopeMismatch,
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct AwsSnapshotDocument {
    pub id: String,
    pub document_type: String,
    pub cache_partition: String,
    pub provider: Provider,
    pub currency: String,
    pub source_region: String,
    pub retrieved_at: String,
    pub snapshot: AwsPriceSnapshot,
}

impl AwsSnapshotDocument {
    pub fn new(snapshot: AwsPriceSnapshot) -> Self {
        Self {
            id: snapshot.metadata.snapshot_id.clone(),
            document_type: AWS_SNAPSHOT_DOCUMENT_TYPE.to_owned(),
            cache_partition: PRICING_CACHE_PARTITION.to_owned(),
            provider: Provider::Aws,
            currency: snapshot.metadata.currency.clone(),
            source_region: snapshot.source_region.clone(),
            retrieved_at: snapshot.metadata.retrieved_at.clone(),
            snapshot,
        }
    }

    pub fn into_snapshot(self) -> Result<AwsPriceSnapshot, PricingCacheDocumentError> {
        if self.id != self.snapshot.metadata.snapshot_id
            || self.document_type != AWS_SNAPSHOT_DOCUMENT_TYPE
            || self.cache_partition != PRICING_CACHE_PARTITION
            || self.provider != Provider::Aws
            || self.snapshot.metadata.provider != Provider::Aws
            || self.currency != self.snapshot.metadata.currency
            || self.source_region != self.snapshot.source_region
            || self.retrieved_at != self.snapshot.metadata.retrieved_at
        {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        validate_stored_aws_snapshot(self.snapshot).map_err(Into::into)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct AzureSnapshotDocument {
    pub id: String,
    pub document_type: String,
    pub cache_partition: String,
    pub provider: Provider,
    pub currency: String,
    pub target_region: String,
    pub retrieved_at: String,
    pub snapshot: AzurePriceSnapshot,
}

impl AzureSnapshotDocument {
    pub fn new(snapshot: AzurePriceSnapshot) -> Self {
        Self {
            id: snapshot.metadata.snapshot_id.clone(),
            document_type: AZURE_SNAPSHOT_DOCUMENT_TYPE.to_owned(),
            cache_partition: PRICING_CACHE_PARTITION.to_owned(),
            provider: Provider::Azure,
            currency: snapshot.metadata.currency.clone(),
            target_region: snapshot.target_region.clone(),
            retrieved_at: snapshot.metadata.retrieved_at.clone(),
            snapshot,
        }
    }

    pub fn into_snapshot(self) -> Result<AzurePriceSnapshot, PricingCacheDocumentError> {
        if self.id != self.snapshot.metadata.snapshot_id
            || self.document_type != AZURE_SNAPSHOT_DOCUMENT_TYPE
            || self.cache_partition != PRICING_CACHE_PARTITION
            || self.provider != Provider::Azure
            || self.snapshot.metadata.provider != Provider::Azure
            || self.currency != self.snapshot.metadata.currency
            || self.target_region != self.snapshot.target_region
            || self.retrieved_at != self.snapshot.metadata.retrieved_at
        {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        validate_stored_azure_snapshot(self.snapshot).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::local_fixture;

    #[test]
    fn snapshot_documents_round_trip_only_when_envelopes_and_hashes_match() {
        let (aws, azure) = local_fixture::load().expect("valid local snapshots");
        let expected_aws_id = aws.metadata.snapshot_id.clone();
        let expected_azure_id = azure.metadata.snapshot_id.clone();

        let aws: AwsSnapshotDocument = serde_json::from_slice(
            &serde_json::to_vec(&AwsSnapshotDocument::new(aws)).expect("serialize AWS document"),
        )
        .expect("deserialize AWS document");
        let azure: AzureSnapshotDocument = serde_json::from_slice(
            &serde_json::to_vec(&AzureSnapshotDocument::new(azure))
                .expect("serialize Azure document"),
        )
        .expect("deserialize Azure document");

        assert_eq!(
            aws.into_snapshot()
                .expect("validated AWS snapshot")
                .metadata
                .snapshot_id,
            expected_aws_id
        );
        assert_eq!(
            azure
                .into_snapshot()
                .expect("validated Azure snapshot")
                .metadata
                .snapshot_id,
            expected_azure_id
        );
    }

    #[test]
    fn snapshot_documents_reject_tampered_envelopes_and_content() {
        let (aws, azure) = local_fixture::load().expect("valid local snapshots");
        let mut mismatched_envelope = AwsSnapshotDocument::new(aws);
        mismatched_envelope.source_region = "us-east-1".to_owned();
        assert!(matches!(
            mismatched_envelope.into_snapshot(),
            Err(PricingCacheDocumentError::EnvelopeMismatch)
        ));

        let mut mismatched_content = AzureSnapshotDocument::new(azure);
        mismatched_content.snapshot.metadata.content_sha256 = "0".repeat(64);
        assert!(matches!(
            mismatched_content.into_snapshot(),
            Err(PricingCacheDocumentError::Snapshot(
                SnapshotError::StoredSnapshotMismatch
            ))
        ));
    }
}
