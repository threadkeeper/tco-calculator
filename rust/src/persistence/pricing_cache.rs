use std::{collections::BTreeSet, fmt::Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    config::MAX_PROVIDER_REFRESHES_PER_HOUR,
    pricing::{
        provider::{Provider, ProviderError, ResolutionStatus},
        repository::{RefreshLeaseDecision, RefreshLeaseOutcome},
        snapshot::{
            AwsEbsRateRecord, AwsEc2RateRecord, AwsPriceSnapshot, AwsRdsRateRecord,
            AwsServiceContentHashes, AzurePriceSnapshot, SnapshotCreationMetadata, SnapshotError,
            SnapshotMetadata, aws_ebs_content_hash, aws_ec2_content_hash, aws_rds_content_hash,
            aws_service_content_hashes, validate_stored_azure_snapshot,
        },
    },
};

pub(crate) const PRICING_CACHE_CONTAINER_ID: &str = "pricing-cache";
pub(crate) const PRICING_CACHE_PARTITION: &str = "pricing";
pub(crate) const AWS_STATE_DOCUMENT_TYPE: &str = "aws_price_state";
pub(crate) const AWS_EC2_DOCUMENT_TYPE: &str = "aws_ec2_prices";
pub(crate) const AWS_RDS_DOCUMENT_TYPE: &str = "aws_rds_prices";
pub(crate) const AWS_EBS_DOCUMENT_TYPE: &str = "aws_ebs_prices";
pub(crate) const AZURE_SNAPSHOT_DOCUMENT_TYPE: &str = "azure_price_snapshot";
pub(crate) const REFRESH_LEASE_DOCUMENT_TYPE: &str = "pricing_refresh_lease";
pub(crate) const REFRESH_LEASE_TTL_SECONDS: i32 = 150;
pub(crate) const REFRESH_QUOTA_DOCUMENT_TYPE: &str = "pricing_refresh_quota";
pub(crate) const REFRESH_QUOTA_TTL_SECONDS: i32 = 60 * 60;
const REFRESH_LEASE_ID_PREFIX: &str = "refresh-lease-";
const REFRESH_QUOTA_ID_PREFIX: &str = "refresh-quota-";
const PERSISTENT_DOCUMENT_TTL: i32 = -1;

#[derive(Debug, Error)]
pub(crate) enum PricingCacheDocumentError {
    #[error("pricing cache document envelope is invalid")]
    EnvelopeMismatch,
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct AwsSnapshotStateDocument {
    pub id: String,
    pub document_type: String,
    pub cache_partition: String,
    pub provider: Provider,
    pub currency: String,
    pub source_region: String,
    pub snapshot_id: String,
    pub metadata: SnapshotMetadata,
    pub ec2_content_sha256: String,
    pub rds_content_sha256: String,
    pub ebs_content_sha256: String,
    pub ttl: i32,
}

impl AwsSnapshotStateDocument {
    pub fn new(snapshot: &AwsPriceSnapshot, hashes: &AwsServiceContentHashes) -> Self {
        Self {
            id: aws_state_document_id(&snapshot.metadata.currency, &snapshot.source_region),
            document_type: AWS_STATE_DOCUMENT_TYPE.to_owned(),
            cache_partition: PRICING_CACHE_PARTITION.to_owned(),
            provider: Provider::Aws,
            currency: snapshot.metadata.currency.clone(),
            source_region: snapshot.source_region.clone(),
            snapshot_id: snapshot.metadata.snapshot_id.clone(),
            metadata: snapshot.metadata.clone(),
            ec2_content_sha256: hashes.ec2.clone(),
            rds_content_sha256: hashes.rds.clone(),
            ebs_content_sha256: hashes.ebs.clone(),
            ttl: PERSISTENT_DOCUMENT_TTL,
        }
    }

    pub fn validate(&self) -> Result<(), PricingCacheDocumentError> {
        if self.id != aws_state_document_id(&self.currency, &self.source_region)
            || self.document_type != AWS_STATE_DOCUMENT_TYPE
            || self.cache_partition != PRICING_CACHE_PARTITION
            || self.provider != Provider::Aws
            || self.metadata.provider != Provider::Aws
            || self.currency != self.metadata.currency
            || self.snapshot_id != self.metadata.snapshot_id
            || !valid_snapshot_id(&self.snapshot_id)
            || self.metadata.content_sha256 != self.snapshot_id[4..]
            || self.metadata.status == ResolutionStatus::Unavailable
            || !valid_hash(&self.ec2_content_sha256)
            || !valid_hash(&self.rds_content_sha256)
            || !valid_hash(&self.ebs_content_sha256)
            || self.ttl != PERSISTENT_DOCUMENT_TTL
        {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        parse_timestamp(&self.metadata.retrieved_at)?;
        Ok(())
    }

    pub fn into_snapshot(
        self,
        ec2: AwsEc2PriceDocument,
        rds: AwsRdsPriceDocument,
        ebs: AwsEbsPriceDocument,
    ) -> Result<AwsPriceSnapshot, PricingCacheDocumentError> {
        self.validate()?;
        ec2.validate()?;
        rds.validate()?;
        ebs.validate()?;
        if ec2.content_sha256 != self.ec2_content_sha256
            || rds.content_sha256 != self.rds_content_sha256
            || ebs.content_sha256 != self.ebs_content_sha256
            || !ec2.matches_scope(&self)
            || !rds.matches_scope(&self)
            || !ebs.matches_scope(&self)
        {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        let expected_metadata = self.metadata;
        let snapshot = AwsPriceSnapshot::create(
            creation_metadata(&expected_metadata),
            self.source_region,
            ec2.records,
            rds.records,
            ebs.records,
        )?;
        if serde_json::to_value(&snapshot.metadata).map_err(|_| SnapshotError::Canonicalization)?
            != serde_json::to_value(expected_metadata)
                .map_err(|_| SnapshotError::Canonicalization)?
        {
            return Err(PricingCacheDocumentError::Snapshot(
                SnapshotError::StoredSnapshotMismatch,
            ));
        }
        Ok(snapshot)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct AwsEc2PriceDocument {
    pub id: String,
    pub document_type: String,
    pub cache_partition: String,
    pub provider: Provider,
    pub currency: String,
    pub source_region: String,
    pub parser_schema_version: String,
    pub content_sha256: String,
    pub records_sha256: String,
    pub records: Vec<AwsEc2RateRecord>,
    pub ttl: i32,
}

impl AwsEc2PriceDocument {
    pub fn new(snapshot: &AwsPriceSnapshot, content_sha256: String) -> Result<Self, SnapshotError> {
        Ok(Self {
            id: aws_component_document_id("ec2", &content_sha256),
            document_type: AWS_EC2_DOCUMENT_TYPE.to_owned(),
            cache_partition: PRICING_CACHE_PARTITION.to_owned(),
            provider: Provider::Aws,
            currency: snapshot.metadata.currency.clone(),
            source_region: snapshot.source_region.clone(),
            parser_schema_version: snapshot.metadata.parser_schema_version.clone(),
            content_sha256,
            records_sha256: hash_serialized(&snapshot.ec2_rates)?,
            records: snapshot.ec2_rates.clone(),
            ttl: PERSISTENT_DOCUMENT_TTL,
        })
    }

    pub fn validate(&self) -> Result<(), PricingCacheDocumentError> {
        validate_component_envelope(
            self,
            "ec2",
            AWS_EC2_DOCUMENT_TYPE,
            aws_ec2_content_hash(
                &self.currency,
                &self.source_region,
                &self.parser_schema_version,
                &self.records,
            )?,
            hash_serialized(&self.records)?,
        )?;
        validate_ec2_records(self)
    }

    fn matches_scope(&self, state: &AwsSnapshotStateDocument) -> bool {
        component_matches_scope(self, state)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct AwsRdsPriceDocument {
    pub id: String,
    pub document_type: String,
    pub cache_partition: String,
    pub provider: Provider,
    pub currency: String,
    pub source_region: String,
    pub parser_schema_version: String,
    pub content_sha256: String,
    pub records_sha256: String,
    pub records: Vec<AwsRdsRateRecord>,
    pub ttl: i32,
}

impl AwsRdsPriceDocument {
    pub fn new(snapshot: &AwsPriceSnapshot, content_sha256: String) -> Result<Self, SnapshotError> {
        Ok(Self {
            id: aws_component_document_id("rds", &content_sha256),
            document_type: AWS_RDS_DOCUMENT_TYPE.to_owned(),
            cache_partition: PRICING_CACHE_PARTITION.to_owned(),
            provider: Provider::Aws,
            currency: snapshot.metadata.currency.clone(),
            source_region: snapshot.source_region.clone(),
            parser_schema_version: snapshot.metadata.parser_schema_version.clone(),
            content_sha256,
            records_sha256: hash_serialized(&snapshot.rds_rates)?,
            records: snapshot.rds_rates.clone(),
            ttl: PERSISTENT_DOCUMENT_TTL,
        })
    }

    pub fn validate(&self) -> Result<(), PricingCacheDocumentError> {
        validate_component_envelope(
            self,
            "rds",
            AWS_RDS_DOCUMENT_TYPE,
            aws_rds_content_hash(
                &self.currency,
                &self.source_region,
                &self.parser_schema_version,
                &self.records,
            )?,
            hash_serialized(&self.records)?,
        )?;
        validate_rds_records(self)
    }

    fn matches_scope(&self, state: &AwsSnapshotStateDocument) -> bool {
        component_matches_scope(self, state)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct AwsEbsPriceDocument {
    pub id: String,
    pub document_type: String,
    pub cache_partition: String,
    pub provider: Provider,
    pub currency: String,
    pub source_region: String,
    pub parser_schema_version: String,
    pub content_sha256: String,
    pub records_sha256: String,
    pub records: Vec<AwsEbsRateRecord>,
    pub ttl: i32,
}

impl AwsEbsPriceDocument {
    pub fn new(snapshot: &AwsPriceSnapshot, content_sha256: String) -> Result<Self, SnapshotError> {
        Ok(Self {
            id: aws_component_document_id("ebs", &content_sha256),
            document_type: AWS_EBS_DOCUMENT_TYPE.to_owned(),
            cache_partition: PRICING_CACHE_PARTITION.to_owned(),
            provider: Provider::Aws,
            currency: snapshot.metadata.currency.clone(),
            source_region: snapshot.source_region.clone(),
            parser_schema_version: snapshot.metadata.parser_schema_version.clone(),
            content_sha256,
            records_sha256: hash_serialized(&snapshot.ebs_rates)?,
            records: snapshot.ebs_rates.clone(),
            ttl: PERSISTENT_DOCUMENT_TTL,
        })
    }

    pub fn validate(&self) -> Result<(), PricingCacheDocumentError> {
        validate_component_envelope(
            self,
            "ebs",
            AWS_EBS_DOCUMENT_TYPE,
            aws_ebs_content_hash(
                &self.currency,
                &self.source_region,
                &self.parser_schema_version,
                &self.records,
            )?,
            hash_serialized(&self.records)?,
        )?;
        validate_ebs_records(self)
    }

    fn matches_scope(&self, state: &AwsSnapshotStateDocument) -> bool {
        component_matches_scope(self, state)
    }
}

pub(crate) struct AwsPriceDocuments {
    pub state: AwsSnapshotStateDocument,
    pub ec2: AwsEc2PriceDocument,
    pub rds: AwsRdsPriceDocument,
    pub ebs: AwsEbsPriceDocument,
}

impl AwsPriceDocuments {
    pub fn new(snapshot: &AwsPriceSnapshot) -> Result<Self, PricingCacheDocumentError> {
        let hashes = aws_service_content_hashes(snapshot)?;
        Ok(Self {
            state: AwsSnapshotStateDocument::new(snapshot, &hashes),
            ec2: AwsEc2PriceDocument::new(snapshot, hashes.ec2)?,
            rds: AwsRdsPriceDocument::new(snapshot, hashes.rds)?,
            ebs: AwsEbsPriceDocument::new(snapshot, hashes.ebs)?,
        })
    }

    pub fn state_for_components(
        incoming: &AwsPriceSnapshot,
        ec2: &AwsEc2PriceDocument,
        rds: &AwsRdsPriceDocument,
        ebs: &AwsEbsPriceDocument,
    ) -> Result<(AwsSnapshotStateDocument, AwsPriceSnapshot), PricingCacheDocumentError> {
        ec2.validate()?;
        rds.validate()?;
        ebs.validate()?;
        let same_scope = [
            (
                ec2.currency.as_str(),
                ec2.source_region.as_str(),
                ec2.parser_schema_version.as_str(),
            ),
            (
                rds.currency.as_str(),
                rds.source_region.as_str(),
                rds.parser_schema_version.as_str(),
            ),
            (
                ebs.currency.as_str(),
                ebs.source_region.as_str(),
                ebs.parser_schema_version.as_str(),
            ),
        ]
        .into_iter()
        .all(|(currency, region, parser)| {
            currency == incoming.metadata.currency
                && region == incoming.source_region
                && parser == incoming.metadata.parser_schema_version
        });
        if !same_scope {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        let persisted = AwsPriceSnapshot::create(
            creation_metadata(&incoming.metadata),
            incoming.source_region.clone(),
            ec2.records.clone(),
            rds.records.clone(),
            ebs.records.clone(),
        )?;
        Ok((Self::new(&persisted)?.state, persisted))
    }
}

trait AwsComponentDocument {
    fn id(&self) -> &str;
    fn document_type(&self) -> &str;
    fn cache_partition(&self) -> &str;
    fn provider(&self) -> Provider;
    fn currency(&self) -> &str;
    fn source_region(&self) -> &str;
    fn parser_schema_version(&self) -> &str;
    fn content_sha256(&self) -> &str;
    fn records_sha256(&self) -> &str;
    fn ttl(&self) -> i32;
}

macro_rules! impl_aws_component_document {
    ($type:ty) => {
        impl AwsComponentDocument for $type {
            fn id(&self) -> &str {
                &self.id
            }
            fn document_type(&self) -> &str {
                &self.document_type
            }
            fn cache_partition(&self) -> &str {
                &self.cache_partition
            }
            fn provider(&self) -> Provider {
                self.provider
            }
            fn currency(&self) -> &str {
                &self.currency
            }
            fn source_region(&self) -> &str {
                &self.source_region
            }
            fn parser_schema_version(&self) -> &str {
                &self.parser_schema_version
            }
            fn content_sha256(&self) -> &str {
                &self.content_sha256
            }
            fn records_sha256(&self) -> &str {
                &self.records_sha256
            }
            fn ttl(&self) -> i32 {
                self.ttl
            }
        }
    };
}

impl_aws_component_document!(AwsEc2PriceDocument);
impl_aws_component_document!(AwsRdsPriceDocument);
impl_aws_component_document!(AwsEbsPriceDocument);

fn validate_component_envelope(
    document: &impl AwsComponentDocument,
    service: &str,
    document_type: &str,
    computed_hash: String,
    computed_records_hash: String,
) -> Result<(), PricingCacheDocumentError> {
    if document.id() != aws_component_document_id(service, document.content_sha256())
        || document.document_type() != document_type
        || document.cache_partition() != PRICING_CACHE_PARTITION
        || document.provider() != Provider::Aws
        || document.currency().is_empty()
        || document.source_region().is_empty()
        || document.parser_schema_version().is_empty()
        || document.content_sha256() != computed_hash
        || document.records_sha256() != computed_records_hash
        || document.ttl() != PERSISTENT_DOCUMENT_TTL
    {
        return Err(PricingCacheDocumentError::EnvelopeMismatch);
    }
    Ok(())
}

fn component_matches_scope(
    document: &impl AwsComponentDocument,
    state: &AwsSnapshotStateDocument,
) -> bool {
    document.currency() == state.currency
        && document.source_region() == state.source_region
        && document.parser_schema_version() == state.metadata.parser_schema_version
}

fn validate_ec2_records(document: &AwsEc2PriceDocument) -> Result<(), PricingCacheDocumentError> {
    let original =
        serde_json::to_value(&document.records).map_err(|_| SnapshotError::Canonicalization)?;
    let rebuilt = AwsPriceSnapshot::create(
        component_metadata(document),
        document.source_region.clone(),
        document.records.clone(),
        Vec::new(),
        Vec::new(),
    )?;
    validate_component_round_trip(original, &rebuilt.ec2_rates)
}

fn validate_rds_records(document: &AwsRdsPriceDocument) -> Result<(), PricingCacheDocumentError> {
    let original =
        serde_json::to_value(&document.records).map_err(|_| SnapshotError::Canonicalization)?;
    let rebuilt = AwsPriceSnapshot::create(
        component_metadata(document),
        document.source_region.clone(),
        Vec::new(),
        document.records.clone(),
        Vec::new(),
    )?;
    validate_component_round_trip(original, &rebuilt.rds_rates)
}

fn validate_ebs_records(document: &AwsEbsPriceDocument) -> Result<(), PricingCacheDocumentError> {
    let original =
        serde_json::to_value(&document.records).map_err(|_| SnapshotError::Canonicalization)?;
    let rebuilt = AwsPriceSnapshot::create(
        component_metadata(document),
        document.source_region.clone(),
        Vec::new(),
        Vec::new(),
        document.records.clone(),
    )?;
    validate_component_round_trip(original, &rebuilt.ebs_rates)
}

fn component_metadata(document: &impl AwsComponentDocument) -> SnapshotCreationMetadata {
    SnapshotCreationMetadata {
        status: ResolutionStatus::Fresh,
        retrieved_at: "1970-01-01T00:00:00Z".to_owned(),
        source_published_at: None,
        currency: document.currency().to_owned(),
        source_urls: Vec::new(),
        parser_schema_version: document.parser_schema_version().to_owned(),
        warnings: Vec::new(),
    }
}

fn validate_component_round_trip(
    original: serde_json::Value,
    rebuilt: &impl Serialize,
) -> Result<(), PricingCacheDocumentError> {
    if serde_json::to_value(rebuilt).map_err(|_| SnapshotError::Canonicalization)? != original {
        return Err(PricingCacheDocumentError::Snapshot(
            SnapshotError::StoredSnapshotMismatch,
        ));
    }
    Ok(())
}

fn creation_metadata(metadata: &SnapshotMetadata) -> SnapshotCreationMetadata {
    SnapshotCreationMetadata {
        status: metadata.status,
        retrieved_at: metadata.retrieved_at.clone(),
        source_published_at: metadata.source_published_at.clone(),
        currency: metadata.currency.clone(),
        source_urls: metadata.source_urls.clone(),
        parser_schema_version: metadata.parser_schema_version.clone(),
        warnings: metadata.warnings.clone(),
    }
}

fn hash_serialized(value: &impl Serialize) -> Result<String, SnapshotError> {
    let digest =
        Sha256::digest(serde_json::to_vec(value).map_err(|_| SnapshotError::Canonicalization)?);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").map_err(|_| SnapshotError::Canonicalization)?;
    }
    Ok(encoded)
}

pub(crate) fn aws_state_document_id(currency: &str, source_region: &str) -> String {
    format!(
        "aws-current-{}-{source_region}",
        currency.to_ascii_lowercase()
    )
}

pub(crate) fn aws_component_document_id(service: &str, content_sha256: &str) -> String {
    format!("aws-{service}-{content_sha256}")
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

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct RefreshLeaseDocument {
    pub id: String,
    pub document_type: String,
    pub cache_partition: String,
    pub cache_key_sha256: String,
    pub owner_token: String,
    pub leased_at: String,
    pub expires_at: String,
    pub completed_at: Option<String>,
    pub status: RefreshLeaseStatus,
    pub ttl: i32,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum RefreshLeaseStatus {
    InProgress,
    Succeeded { snapshot_id: String },
    Failed { error: ProviderError },
}

impl RefreshLeaseDocument {
    pub fn new(
        cache_key_sha256: &str,
        owner_token: &str,
        now: OffsetDateTime,
    ) -> Result<Self, PricingCacheDocumentError> {
        let expires_at = now + Duration::seconds(i64::from(REFRESH_LEASE_TTL_SECONDS));
        let document = Self {
            id: format!("{REFRESH_LEASE_ID_PREFIX}{cache_key_sha256}"),
            document_type: REFRESH_LEASE_DOCUMENT_TYPE.to_owned(),
            cache_partition: PRICING_CACHE_PARTITION.to_owned(),
            cache_key_sha256: cache_key_sha256.to_owned(),
            owner_token: owner_token.to_owned(),
            leased_at: format_timestamp(now)?,
            expires_at: format_timestamp(expires_at)?,
            completed_at: None,
            status: RefreshLeaseStatus::InProgress,
            ttl: REFRESH_LEASE_TTL_SECONDS,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), PricingCacheDocumentError> {
        let leased_at = parse_timestamp(&self.leased_at)?;
        let expires_at = parse_timestamp(&self.expires_at)?;
        let valid_owner = Uuid::parse_str(&self.owner_token)
            .ok()
            .is_some_and(|owner| owner.to_string() == self.owner_token);
        if !valid_hash(&self.cache_key_sha256)
            || self.id != format!("{REFRESH_LEASE_ID_PREFIX}{}", self.cache_key_sha256)
            || self.document_type != REFRESH_LEASE_DOCUMENT_TYPE
            || self.cache_partition != PRICING_CACHE_PARTITION
            || !valid_owner
            || expires_at - leased_at != Duration::seconds(i64::from(REFRESH_LEASE_TTL_SECONDS))
            || self.ttl != REFRESH_LEASE_TTL_SECONDS
        {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        match (&self.status, &self.completed_at) {
            (RefreshLeaseStatus::InProgress, None) => Ok(()),
            (RefreshLeaseStatus::Succeeded { snapshot_id }, Some(completed_at))
                if valid_snapshot_id(snapshot_id)
                    && (leased_at..=expires_at).contains(&parse_timestamp(completed_at)?) =>
            {
                Ok(())
            }
            (RefreshLeaseStatus::Failed { .. }, Some(completed_at))
                if (leased_at..=expires_at).contains(&parse_timestamp(completed_at)?) =>
            {
                Ok(())
            }
            _ => Err(PricingCacheDocumentError::EnvelopeMismatch),
        }
    }

    pub fn decision(
        &self,
        request_started_at: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<Option<RefreshLeaseDecision>, PricingCacheDocumentError> {
        self.validate()?;
        match &self.status {
            RefreshLeaseStatus::InProgress => {
                if parse_timestamp(&self.expires_at)? <= now {
                    Ok(None)
                } else {
                    Ok(Some(RefreshLeaseDecision::Pending))
                }
            }
            RefreshLeaseStatus::Succeeded { snapshot_id } => {
                if self.completed_for(request_started_at)? {
                    Ok(Some(RefreshLeaseDecision::Succeeded(snapshot_id.clone())))
                } else {
                    Ok(None)
                }
            }
            RefreshLeaseStatus::Failed { error } => {
                if self.completed_for(request_started_at)? {
                    Ok(Some(RefreshLeaseDecision::Failed(*error)))
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub fn complete(
        &self,
        owner_token: &str,
        outcome: &RefreshLeaseOutcome,
        now: OffsetDateTime,
    ) -> Result<Self, PricingCacheDocumentError> {
        self.validate()?;
        if self.owner_token != owner_token
            || self.status != RefreshLeaseStatus::InProgress
            || parse_timestamp(&self.expires_at)? <= now
        {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        let mut completed = self.clone();
        completed.completed_at = Some(format_timestamp(now)?);
        completed.status = match outcome {
            RefreshLeaseOutcome::Succeeded(snapshot_id) if valid_snapshot_id(snapshot_id) => {
                RefreshLeaseStatus::Succeeded {
                    snapshot_id: snapshot_id.clone(),
                }
            }
            RefreshLeaseOutcome::Failed(error) => RefreshLeaseStatus::Failed { error: *error },
            RefreshLeaseOutcome::Succeeded(_) => {
                return Err(PricingCacheDocumentError::EnvelopeMismatch);
            }
        };
        completed.validate()?;
        Ok(completed)
    }

    pub fn matches_outcome(
        &self,
        owner_token: &str,
        outcome: &RefreshLeaseOutcome,
    ) -> Result<bool, PricingCacheDocumentError> {
        self.validate()?;
        let matches = match (&self.status, outcome) {
            (
                RefreshLeaseStatus::Succeeded {
                    snapshot_id: stored,
                },
                RefreshLeaseOutcome::Succeeded(expected),
            ) => stored == expected,
            (
                RefreshLeaseStatus::Failed { error: stored },
                RefreshLeaseOutcome::Failed(expected),
            ) => stored == expected,
            _ => false,
        };
        Ok(self.owner_token == owner_token && matches)
    }

    fn completed_for(
        &self,
        request_started_at: OffsetDateTime,
    ) -> Result<bool, PricingCacheDocumentError> {
        self.completed_at
            .as_deref()
            .map(parse_timestamp)
            .transpose()
            .map(|completed_at| completed_at.is_some_and(|value| value >= request_started_at))
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct RefreshQuotaDocument {
    pub id: String,
    pub document_type: String,
    pub cache_partition: String,
    pub identity_sha256: String,
    pub window_started_at: String,
    pub expires_at: String,
    pub limit: u32,
    pub count: u32,
    pub operation_tokens: Vec<String>,
    pub ttl: i32,
}

pub(crate) enum RefreshQuotaMutation {
    Allowed,
    Limited(u64),
    Replace(RefreshQuotaDocument),
}

impl RefreshQuotaDocument {
    pub fn new(
        identity_sha256: &str,
        operation_token: &str,
        limit: u32,
        now: OffsetDateTime,
    ) -> Result<Self, PricingCacheDocumentError> {
        validate_quota_inputs(identity_sha256, operation_token, limit)?;
        let document = Self {
            id: format!("{REFRESH_QUOTA_ID_PREFIX}{identity_sha256}"),
            document_type: REFRESH_QUOTA_DOCUMENT_TYPE.to_owned(),
            cache_partition: PRICING_CACHE_PARTITION.to_owned(),
            identity_sha256: identity_sha256.to_owned(),
            window_started_at: format_timestamp(now)?,
            expires_at: format_timestamp(
                now + Duration::seconds(i64::from(REFRESH_QUOTA_TTL_SECONDS)),
            )?,
            limit,
            count: 1,
            operation_tokens: vec![operation_token.to_owned()],
            ttl: REFRESH_QUOTA_TTL_SECONDS,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), PricingCacheDocumentError> {
        let window_started_at = parse_timestamp(&self.window_started_at)?;
        let expires_at = parse_timestamp(&self.expires_at)?;
        let operations = self.operation_tokens.iter().collect::<BTreeSet<_>>();
        let valid_operations = operations.len() == self.operation_tokens.len()
            && self.operation_tokens.iter().all(|operation| {
                Uuid::parse_str(operation)
                    .ok()
                    .is_some_and(|value| value.to_string() == *operation)
            });
        if !valid_hash(&self.identity_sha256)
            || self.id != format!("{REFRESH_QUOTA_ID_PREFIX}{}", self.identity_sha256)
            || self.document_type != REFRESH_QUOTA_DOCUMENT_TYPE
            || self.cache_partition != PRICING_CACHE_PARTITION
            || self.limit == 0
            || self.limit > MAX_PROVIDER_REFRESHES_PER_HOUR
            || self.count == 0
            || usize::try_from(self.count).ok() != Some(self.operation_tokens.len())
            || self.count > self.limit
            || !valid_operations
            || expires_at - window_started_at
                != Duration::seconds(i64::from(REFRESH_QUOTA_TTL_SECONDS))
            || !(1..=REFRESH_QUOTA_TTL_SECONDS).contains(&self.ttl)
        {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        Ok(())
    }

    pub fn consume(
        &self,
        identity_sha256: &str,
        operation_token: &str,
        limit: u32,
        now: OffsetDateTime,
    ) -> Result<RefreshQuotaMutation, PricingCacheDocumentError> {
        self.validate()?;
        validate_quota_inputs(identity_sha256, operation_token, limit)?;
        if self.identity_sha256 != identity_sha256 {
            return Err(PricingCacheDocumentError::EnvelopeMismatch);
        }
        if self
            .operation_tokens
            .iter()
            .any(|applied| applied == operation_token)
        {
            return Ok(RefreshQuotaMutation::Allowed);
        }
        let expires_at = parse_timestamp(&self.expires_at)?;
        if expires_at <= now {
            return Self::new(identity_sha256, operation_token, limit, now)
                .map(RefreshQuotaMutation::Replace);
        }
        let retry_after = remaining_seconds(expires_at, now)?;
        if self.count >= self.limit.min(limit) {
            return Ok(RefreshQuotaMutation::Limited(retry_after));
        }
        let mut updated = self.clone();
        updated.count += 1;
        updated.operation_tokens.push(operation_token.to_owned());
        updated.ttl =
            i32::try_from(retry_after).map_err(|_| PricingCacheDocumentError::EnvelopeMismatch)?;
        updated.validate()?;
        Ok(RefreshQuotaMutation::Replace(updated))
    }
}

fn validate_quota_inputs(
    identity_sha256: &str,
    operation_token: &str,
    limit: u32,
) -> Result<(), PricingCacheDocumentError> {
    let valid_operation = Uuid::parse_str(operation_token)
        .ok()
        .is_some_and(|value| value.to_string() == operation_token);
    if valid_hash(identity_sha256)
        && valid_operation
        && (1..=MAX_PROVIDER_REFRESHES_PER_HOUR).contains(&limit)
    {
        Ok(())
    } else {
        Err(PricingCacheDocumentError::EnvelopeMismatch)
    }
}

fn remaining_seconds(
    expires_at: OffsetDateTime,
    now: OffsetDateTime,
) -> Result<u64, PricingCacheDocumentError> {
    let remaining = expires_at - now;
    if remaining <= Duration::ZERO {
        return Ok(0);
    }
    let whole_seconds = remaining.whole_seconds();
    let rounded_seconds = if remaining > Duration::seconds(whole_seconds) {
        whole_seconds + 1
    } else {
        whole_seconds
    };
    u64::try_from(rounded_seconds.min(i64::from(REFRESH_QUOTA_TTL_SECONDS)))
        .map_err(|_| PricingCacheDocumentError::EnvelopeMismatch)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_snapshot_id(value: &str) -> bool {
    value
        .strip_prefix("aws-")
        .or_else(|| value.strip_prefix("azure-"))
        .is_some_and(valid_hash)
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, PricingCacheDocumentError> {
    let timestamp = OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| PricingCacheDocumentError::EnvelopeMismatch)?;
    if timestamp.offset() != time::UtcOffset::UTC {
        return Err(PricingCacheDocumentError::EnvelopeMismatch);
    }
    Ok(timestamp)
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, PricingCacheDocumentError> {
    value
        .format(&Rfc3339)
        .map_err(|_| PricingCacheDocumentError::EnvelopeMismatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::local_fixture;

    #[test]
    fn split_aws_documents_round_trip_only_when_envelopes_and_hashes_match() {
        let (aws, azure) = local_fixture::load().expect("valid local snapshots");
        let expected_aws_id = aws.metadata.snapshot_id.clone();
        let expected_azure_id = azure.metadata.snapshot_id.clone();

        let documents = AwsPriceDocuments::new(&aws).expect("split AWS documents");
        let state: AwsSnapshotStateDocument = round_trip(documents.state);
        let ec2: AwsEc2PriceDocument = round_trip(documents.ec2);
        let rds: AwsRdsPriceDocument = round_trip(documents.rds);
        let ebs: AwsEbsPriceDocument = round_trip(documents.ebs);
        let azure: AzureSnapshotDocument = serde_json::from_slice(
            &serde_json::to_vec(&AzureSnapshotDocument::new(azure))
                .expect("serialize Azure document"),
        )
        .expect("deserialize Azure document");

        assert_eq!(
            state
                .into_snapshot(ec2, rds, ebs)
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
        let mut documents = AwsPriceDocuments::new(&aws).expect("split AWS documents");
        documents.state.source_region = "us-east-1".to_owned();
        assert!(matches!(
            documents.state.validate(),
            Err(PricingCacheDocumentError::EnvelopeMismatch)
        ));

        let mut tampered_ec2 = AwsPriceDocuments::new(&aws)
            .expect("split AWS documents")
            .ec2;
        tampered_ec2.records[0].rate.compute_hourly.0 += rust_decimal::Decimal::ONE;
        assert!(matches!(
            tampered_ec2.validate(),
            Err(PricingCacheDocumentError::EnvelopeMismatch)
        ));

        let mut tampered_provenance = AwsPriceDocuments::new(&aws)
            .expect("split AWS documents")
            .ec2;
        tampered_provenance.records[0].provenance.source_version =
            Some("tampered-publication".to_owned());
        assert!(matches!(
            tampered_provenance.validate(),
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

    #[test]
    fn unchanged_service_core_data_reuses_component_ids() {
        let (first, _) = local_fixture::load().expect("valid local snapshots");
        let mut second_ec2 = first.ec2_rates.clone();
        second_ec2[0].provenance.source_version = Some("new-publication".to_owned());
        let second = AwsPriceSnapshot::create(
            SnapshotCreationMetadata {
                status: first.metadata.status,
                retrieved_at: "2026-08-08T07:41:09Z".to_owned(),
                source_published_at: first.metadata.source_published_at.clone(),
                currency: first.metadata.currency.clone(),
                source_urls: first.metadata.source_urls.clone(),
                parser_schema_version: first.metadata.parser_schema_version.clone(),
                warnings: first.metadata.warnings.clone(),
            },
            first.source_region.clone(),
            second_ec2,
            first.rds_rates.clone(),
            first.ebs_rates.clone(),
        )
        .expect("second snapshot");

        let first_documents = AwsPriceDocuments::new(&first).expect("first documents");
        let second_documents = AwsPriceDocuments::new(&second).expect("second documents");
        assert_eq!(first_documents.ec2.id, second_documents.ec2.id);
        assert_eq!(first_documents.rds.id, second_documents.rds.id);
        assert_eq!(first_documents.ebs.id, second_documents.ebs.id);
        assert_ne!(first.metadata.snapshot_id, second.metadata.snapshot_id);

        let (state, persisted) = AwsPriceDocuments::state_for_components(
            &second,
            &first_documents.ec2,
            &first_documents.rds,
            &first_documents.ebs,
        )
        .expect("state from retained components");
        assert_eq!(state.snapshot_id, first.metadata.snapshot_id);
        assert_eq!(persisted.metadata.retrieved_at, "2026-08-08T07:41:09Z");
    }

    #[test]
    fn changing_one_service_rotates_only_that_component_id() {
        let (first, _) = local_fixture::load().expect("valid local snapshots");
        let mut changed_ec2 = first.ec2_rates.clone();
        changed_ec2[0].rate.compute_hourly.0 += rust_decimal::Decimal::ONE;
        let second = AwsPriceSnapshot::create(
            creation_metadata(&first.metadata),
            first.source_region.clone(),
            changed_ec2,
            first.rds_rates.clone(),
            first.ebs_rates.clone(),
        )
        .expect("second snapshot");

        let first_documents = AwsPriceDocuments::new(&first).expect("first documents");
        let second_documents = AwsPriceDocuments::new(&second).expect("second documents");
        assert_ne!(first_documents.ec2.id, second_documents.ec2.id);
        assert_eq!(first_documents.rds.id, second_documents.rds.id);
        assert_eq!(first_documents.ebs.id, second_documents.ebs.id);
    }

    fn round_trip<T>(value: T) -> T
    where
        T: for<'de> Deserialize<'de> + Serialize,
    {
        serde_json::from_slice(&serde_json::to_vec(&value).expect("serialize document"))
            .expect("deserialize document")
    }

    #[test]
    fn refresh_lease_exposes_terminal_results_only_to_overlapping_requests() {
        let leased_at = timestamp("2026-08-10T12:00:00Z");
        let lease = RefreshLeaseDocument::new(
            &"a".repeat(64),
            "11111111-1111-4111-8111-111111111111",
            leased_at,
        )
        .expect("valid active lease");
        assert_eq!(
            lease
                .decision(timestamp("2026-08-10T11:59:59Z"), leased_at)
                .expect("active decision"),
            Some(RefreshLeaseDecision::Pending)
        );

        let completed = lease
            .complete(
                "11111111-1111-4111-8111-111111111111",
                &RefreshLeaseOutcome::Succeeded(format!("aws-{}", "b".repeat(64))),
                timestamp("2026-08-10T12:00:30Z"),
            )
            .expect("complete lease");
        assert!(matches!(
            completed
                .decision(
                    timestamp("2026-08-10T12:00:20Z"),
                    timestamp("2026-08-10T12:00:31Z")
                )
                .expect("overlapping request decision"),
            Some(RefreshLeaseDecision::Succeeded(_))
        ));
        assert_eq!(
            completed
                .decision(
                    timestamp("2026-08-10T12:00:31Z"),
                    timestamp("2026-08-10T12:00:31Z")
                )
                .expect("later request decision"),
            None
        );
    }

    #[test]
    fn refresh_lease_rejects_expired_ownership_and_tampering() {
        let leased_at = timestamp("2026-08-10T12:00:00Z");
        let lease = RefreshLeaseDocument::new(
            &"a".repeat(64),
            "11111111-1111-4111-8111-111111111111",
            leased_at,
        )
        .expect("valid active lease");
        assert_eq!(
            lease
                .decision(
                    timestamp("2026-08-10T12:00:00Z"),
                    timestamp("2026-08-10T12:02:30Z")
                )
                .expect("expired decision"),
            None
        );
        assert!(
            lease
                .complete(
                    "11111111-1111-4111-8111-111111111111",
                    &RefreshLeaseOutcome::Failed(ProviderError::SchemaChanged),
                    timestamp("2026-08-10T12:02:30Z")
                )
                .is_err()
        );

        let mut tampered = lease;
        tampered.ttl = 151;
        assert!(tampered.validate().is_err());

        let mut future_completion = RefreshLeaseDocument::new(
            &"a".repeat(64),
            "11111111-1111-4111-8111-111111111111",
            leased_at,
        )
        .expect("valid active lease");
        future_completion.status = RefreshLeaseStatus::Failed {
            error: ProviderError::TemporarilyUnavailable,
        };
        future_completion.completed_at = Some("2026-08-10T12:02:31Z".to_owned());
        assert!(future_completion.validate().is_err());
    }

    #[test]
    fn refresh_quota_is_idempotent_bounded_and_resets_after_expiry() {
        let identity = "a".repeat(64);
        let first_operation = "11111111-1111-4111-8111-111111111111";
        let second_operation = "22222222-2222-4222-8222-222222222222";
        let third_operation = "33333333-3333-4333-8333-333333333333";
        let started_at = timestamp("2026-08-10T12:00:00Z");
        let document = RefreshQuotaDocument::new(&identity, first_operation, 2, started_at)
            .expect("valid quota document");
        assert!(matches!(
            document
                .consume(&identity, first_operation, 2, started_at)
                .expect("idempotent operation"),
            RefreshQuotaMutation::Allowed
        ));

        let updated = match document
            .consume(
                &identity,
                second_operation,
                2,
                timestamp("2026-08-10T12:00:00.5Z"),
            )
            .expect("second operation")
        {
            RefreshQuotaMutation::Replace(updated) => updated,
            _ => panic!("second operation must update the counter"),
        };
        assert_eq!(updated.count, 2);
        assert_eq!(updated.ttl, 3_600);
        assert!(matches!(
            updated
                .consume(
                    &identity,
                    second_operation,
                    2,
                    timestamp("2026-08-10T12:00:01Z")
                )
                .expect("retried operation"),
            RefreshQuotaMutation::Allowed
        ));
        assert!(matches!(
            updated
                .consume(
                    &identity,
                    third_operation,
                    2,
                    timestamp("2026-08-10T12:00:01Z")
                )
                .expect("limited operation"),
            RefreshQuotaMutation::Limited(3_599)
        ));

        let reset = match updated
            .consume(
                &identity,
                third_operation,
                2,
                timestamp("2026-08-10T13:00:00Z"),
            )
            .expect("reset operation")
        {
            RefreshQuotaMutation::Replace(reset) => reset,
            _ => panic!("expired window must be replaced"),
        };
        assert_eq!(reset.count, 1);
        assert_eq!(reset.operation_tokens, vec![third_operation]);
    }

    #[test]
    fn refresh_quota_merges_an_operation_after_a_conditional_write_conflict() {
        let identity = "a".repeat(64);
        let first_operation = "11111111-1111-4111-8111-111111111111";
        let competing_operation = "22222222-2222-4222-8222-222222222222";
        let retried_operation = "33333333-3333-4333-8333-333333333333";
        let now = timestamp("2026-08-10T12:00:00Z");
        let original = RefreshQuotaDocument::new(&identity, first_operation, 3, now)
            .expect("valid quota document");
        let winner = match original
            .consume(&identity, competing_operation, 3, now)
            .expect("competing operation")
        {
            RefreshQuotaMutation::Replace(updated) => updated,
            _ => panic!("competing operation must update the counter"),
        };
        let merged = match winner
            .consume(&identity, retried_operation, 3, now)
            .expect("operation retried after conflict")
        {
            RefreshQuotaMutation::Replace(updated) => updated,
            _ => panic!("retried operation must merge with the winner"),
        };

        assert_eq!(merged.count, 3);
        assert_eq!(
            merged.operation_tokens,
            vec![first_operation, competing_operation, retried_operation]
        );
        assert!(matches!(
            merged
                .consume(&identity, retried_operation, 3, now)
                .expect("replayed merged operation"),
            RefreshQuotaMutation::Allowed
        ));
    }

    #[test]
    fn refresh_quota_rejects_tampered_or_unbounded_documents() {
        let identity = "a".repeat(64);
        let operation = "11111111-1111-4111-8111-111111111111";
        let mut document =
            RefreshQuotaDocument::new(&identity, operation, 2, timestamp("2026-08-10T12:00:00Z"))
                .expect("valid quota document");
        document.operation_tokens.push(operation.to_owned());
        document.count += 1;
        assert!(document.validate().is_err());
        assert!(
            RefreshQuotaDocument::new(
                &identity,
                operation,
                MAX_PROVIDER_REFRESHES_PER_HOUR + 1,
                timestamp("2026-08-10T12:00:00Z")
            )
            .is_err()
        );

        let other = RefreshQuotaDocument::new(
            &"b".repeat(64),
            "22222222-2222-4222-8222-222222222222",
            2,
            timestamp("2026-08-10T12:00:00Z"),
        )
        .expect("independent identity quota");
        assert_ne!(document.id, other.id);
    }

    fn timestamp(value: &str) -> OffsetDateTime {
        OffsetDateTime::parse(value, &Rfc3339).expect("valid test timestamp")
    }
}
