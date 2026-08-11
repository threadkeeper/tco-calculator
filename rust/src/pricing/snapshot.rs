use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    calculation::cost::{AzureRate, EbsRate, Ec2Rate, RdsRate},
    domain::resource::{EbsVolumeType, PurchaseOption, RdsDeployment},
};

use super::provider::{Provider, ResolutionStatus};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RateProvenance {
    pub source_url: String,
    pub effective_at: Option<String>,
    pub source_version: Option<String>,
    pub meter_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SnapshotMetadata {
    pub snapshot_id: String,
    pub provider: Provider,
    pub status: ResolutionStatus,
    pub retrieved_at: String,
    pub source_published_at: Option<String>,
    pub currency: String,
    pub source_urls: Vec<String>,
    pub parser_schema_version: String,
    pub content_sha256: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AwsPriceSnapshot {
    pub metadata: SnapshotMetadata,
    pub source_region: String,
    pub ec2_rates: Vec<AwsEc2RateRecord>,
    pub rds_rates: Vec<AwsRdsRateRecord>,
    pub ebs_rates: Vec<AwsEbsRateRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AwsServiceContentHashes {
    pub ec2: String,
    pub rds: String,
    pub ebs: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AzurePriceSnapshot {
    pub metadata: SnapshotMetadata,
    pub target_region: String,
    pub mi_rates: Vec<AzureMiRateRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AwsEc2RateRecord {
    pub stable_key: String,
    pub instance_type: String,
    #[serde(flatten)]
    pub rate: Ec2Rate,
    pub provenance: RateProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AwsRdsRateRecord {
    pub stable_key: String,
    pub instance_type: String,
    pub deployment: RdsDeployment,
    pub commercial_term: String,
    pub storage_class: String,
    #[serde(flatten)]
    pub rate: RdsRate,
    pub provenance: RateProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AwsEbsRateRecord {
    pub stable_key: String,
    #[serde(flatten)]
    pub rate: EbsRate,
    pub provenance: RateProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AzureMiRateRecord {
    pub stable_key: String,
    pub configuration_key: String,
    pub purchase_option: PurchaseOption,
    #[serde(flatten)]
    pub rate: AzureRate,
    pub provenance: RateProvenance,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SnapshotCreationMetadata {
    pub status: ResolutionStatus,
    pub retrieved_at: String,
    pub source_published_at: Option<String>,
    pub currency: String,
    pub source_urls: Vec<String>,
    pub parser_schema_version: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SnapshotError {
    #[error("snapshot retrieval time must be RFC 3339 UTC")]
    InvalidRetrievedAt,
    #[error("snapshot currency, scope, and parser schema version are required")]
    InvalidScope,
    #[error("only fresh, cached, or stale content can be stored as a snapshot")]
    UnusableStatus,
    #[error("snapshot rate keys must be unique and non-empty")]
    InvalidRateKeys,
    #[error("snapshot rate provenance must contain a source URL")]
    InvalidRateProvenance,
    #[error("AWS snapshot rates are invalid or incomplete")]
    InvalidAwsRates,
    #[error(
        "Azure SQL MI configurations require a complete and consistent eight-option price matrix"
    )]
    InvalidAzurePurchaseMatrix,
    #[error("snapshot content could not be canonicalized")]
    Canonicalization,
    #[error("stored snapshot metadata or content does not match its canonical snapshot")]
    StoredSnapshotMismatch,
}

impl AwsPriceSnapshot {
    pub fn create(
        mut metadata: SnapshotCreationMetadata,
        source_region: impl Into<String>,
        mut ec2_rates: Vec<AwsEc2RateRecord>,
        mut rds_rates: Vec<AwsRdsRateRecord>,
        mut ebs_rates: Vec<AwsEbsRateRecord>,
    ) -> Result<Self, SnapshotError> {
        let source_region = source_region.into();
        validate_creation_metadata(&metadata, &source_region)?;
        normalize_provenance(&mut ec2_rates, |record| &mut record.provenance);
        normalize_provenance(&mut rds_rates, |record| &mut record.provenance);
        normalize_provenance(&mut ebs_rates, |record| &mut record.provenance);
        ec2_rates.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
        rds_rates.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
        ebs_rates.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
        validate_unique_keys(
            ec2_rates
                .iter()
                .map(|record| record.stable_key.as_str())
                .chain(rds_rates.iter().map(|record| record.stable_key.as_str()))
                .chain(ebs_rates.iter().map(|record| record.stable_key.as_str())),
        )?;
        validate_rate_provenance(
            ec2_rates
                .iter()
                .map(|record| &record.provenance)
                .chain(rds_rates.iter().map(|record| &record.provenance))
                .chain(ebs_rates.iter().map(|record| &record.provenance)),
        )?;
        validate_aws_rates(&ec2_rates, &rds_rates, &ebs_rates)?;

        let canonical = AwsCanonicalPayload {
            provider: Provider::Aws,
            currency: &metadata.currency,
            source_region: &source_region,
            parser_schema_version: &metadata.parser_schema_version,
            ec2_rates: &ec2_rates,
            rds_rates: &rds_rates,
            ebs_rates: &ebs_rates,
        };
        let content_sha256 = hash_canonical(&canonical)?;
        let snapshot_id = format!("aws-{content_sha256}");
        normalize_creation_metadata(&mut metadata);

        Ok(Self {
            metadata: SnapshotMetadata {
                snapshot_id,
                provider: Provider::Aws,
                status: metadata.status,
                retrieved_at: metadata.retrieved_at,
                source_published_at: metadata.source_published_at,
                currency: metadata.currency,
                source_urls: metadata.source_urls,
                parser_schema_version: metadata.parser_schema_version,
                content_sha256,
                warnings: metadata.warnings,
            },
            source_region,
            ec2_rates,
            rds_rates,
            ebs_rates,
        })
    }

    pub fn matches_scope(&self, currency: &str, source_region: &str) -> bool {
        self.metadata.provider == Provider::Aws
            && self.metadata.currency == currency
            && self.source_region == source_region
    }

    pub fn ec2_rate(&self, instance_type: &str) -> Option<&AwsEc2RateRecord> {
        self.ec2_rates
            .iter()
            .find(|record| record.instance_type == instance_type)
    }

    pub fn rds_rate(
        &self,
        instance_type: &str,
        deployment: RdsDeployment,
        commercial_term: &str,
        storage_class: &str,
    ) -> Option<&AwsRdsRateRecord> {
        self.rds_rates.iter().find(|record| {
            record.instance_type == instance_type
                && record.deployment == deployment
                && record.commercial_term == commercial_term
                && record.storage_class == storage_class
        })
    }
}

impl AzurePriceSnapshot {
    pub fn create(
        mut metadata: SnapshotCreationMetadata,
        target_region: impl Into<String>,
        mut mi_rates: Vec<AzureMiRateRecord>,
    ) -> Result<Self, SnapshotError> {
        let target_region = target_region.into();
        validate_creation_metadata(&metadata, &target_region)?;
        normalize_provenance(&mut mi_rates, |record| &mut record.provenance);
        mi_rates.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
        validate_unique_keys(mi_rates.iter().map(|record| record.stable_key.as_str()))?;
        validate_rate_provenance(mi_rates.iter().map(|record| &record.provenance))?;
        validate_azure_purchase_matrices(&mi_rates)?;

        let canonical = AzureCanonicalPayload {
            provider: Provider::Azure,
            currency: &metadata.currency,
            target_region: &target_region,
            parser_schema_version: &metadata.parser_schema_version,
            mi_rates: &mi_rates,
        };
        let content_sha256 = hash_canonical(&canonical)?;
        let snapshot_id = format!("azure-{content_sha256}");
        normalize_creation_metadata(&mut metadata);

        Ok(Self {
            metadata: SnapshotMetadata {
                snapshot_id,
                provider: Provider::Azure,
                status: metadata.status,
                retrieved_at: metadata.retrieved_at,
                source_published_at: metadata.source_published_at,
                currency: metadata.currency,
                source_urls: metadata.source_urls,
                parser_schema_version: metadata.parser_schema_version,
                content_sha256,
                warnings: metadata.warnings,
            },
            target_region,
            mi_rates,
        })
    }

    pub fn matches_scope(&self, currency: &str, target_region: &str) -> bool {
        self.metadata.provider == Provider::Azure
            && self.metadata.currency == currency
            && self.target_region == target_region
    }

    pub fn mi_rate(
        &self,
        configuration_key: &str,
        purchase_option: PurchaseOption,
    ) -> Option<&AzureMiRateRecord> {
        self.mi_rates.iter().find(|record| {
            record.configuration_key == configuration_key
                && record.purchase_option == purchase_option
        })
    }

    pub fn has_complete_mi_rate_set(&self, configuration_key: &str) -> bool {
        PurchaseOption::ALL.iter().all(|purchase_option| {
            self.mi_rates
                .iter()
                .filter(|record| {
                    record.configuration_key == configuration_key
                        && record.purchase_option == *purchase_option
                })
                .count()
                == 1
        })
    }
}

pub fn utc_now_rfc3339() -> Result<String, SnapshotError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_| SnapshotError::Canonicalization)
}

pub fn validate_stored_aws_snapshot(
    snapshot: AwsPriceSnapshot,
) -> Result<AwsPriceSnapshot, SnapshotError> {
    let original = serde_json::to_value(&snapshot).map_err(|_| SnapshotError::Canonicalization)?;
    let AwsPriceSnapshot {
        metadata,
        source_region,
        ec2_rates,
        rds_rates,
        ebs_rates,
    } = snapshot;
    let rebuilt = AwsPriceSnapshot::create(
        creation_metadata(metadata),
        source_region,
        ec2_rates,
        rds_rates,
        ebs_rates,
    )?;
    if serde_json::to_value(&rebuilt).map_err(|_| SnapshotError::Canonicalization)? != original {
        return Err(SnapshotError::StoredSnapshotMismatch);
    }
    Ok(rebuilt)
}

pub fn validate_stored_azure_snapshot(
    snapshot: AzurePriceSnapshot,
) -> Result<AzurePriceSnapshot, SnapshotError> {
    let original = serde_json::to_value(&snapshot).map_err(|_| SnapshotError::Canonicalization)?;
    let AzurePriceSnapshot {
        metadata,
        target_region,
        mi_rates,
    } = snapshot;
    let rebuilt = AzurePriceSnapshot::create(creation_metadata(metadata), target_region, mi_rates)?;
    if serde_json::to_value(&rebuilt).map_err(|_| SnapshotError::Canonicalization)? != original {
        return Err(SnapshotError::StoredSnapshotMismatch);
    }
    Ok(rebuilt)
}

pub(crate) fn aws_service_content_hashes(
    snapshot: &AwsPriceSnapshot,
) -> Result<AwsServiceContentHashes, SnapshotError> {
    Ok(AwsServiceContentHashes {
        ec2: aws_ec2_content_hash(
            &snapshot.metadata.currency,
            &snapshot.source_region,
            &snapshot.metadata.parser_schema_version,
            &snapshot.ec2_rates,
        )?,
        rds: aws_rds_content_hash(
            &snapshot.metadata.currency,
            &snapshot.source_region,
            &snapshot.metadata.parser_schema_version,
            &snapshot.rds_rates,
        )?,
        ebs: aws_ebs_content_hash(
            &snapshot.metadata.currency,
            &snapshot.source_region,
            &snapshot.metadata.parser_schema_version,
            &snapshot.ebs_rates,
        )?,
    })
}

pub(crate) fn aws_ec2_content_hash(
    currency: &str,
    source_region: &str,
    parser_schema_version: &str,
    records: &[AwsEc2RateRecord],
) -> Result<String, SnapshotError> {
    hash_canonical(&AwsEc2ServiceCanonicalPayload {
        scope: aws_service_scope(currency, source_region, parser_schema_version),
        records: records
            .iter()
            .map(|record| AwsEc2CoreRecord {
                stable_key: &record.stable_key,
                instance_type: &record.instance_type,
                rate: &record.rate,
            })
            .collect(),
    })
}

pub(crate) fn aws_rds_content_hash(
    currency: &str,
    source_region: &str,
    parser_schema_version: &str,
    records: &[AwsRdsRateRecord],
) -> Result<String, SnapshotError> {
    hash_canonical(&AwsRdsServiceCanonicalPayload {
        scope: aws_service_scope(currency, source_region, parser_schema_version),
        records: records
            .iter()
            .map(|record| AwsRdsCoreRecord {
                stable_key: &record.stable_key,
                instance_type: &record.instance_type,
                deployment: record.deployment,
                commercial_term: &record.commercial_term,
                storage_class: &record.storage_class,
                rate: &record.rate,
            })
            .collect(),
    })
}

pub(crate) fn aws_ebs_content_hash(
    currency: &str,
    source_region: &str,
    parser_schema_version: &str,
    records: &[AwsEbsRateRecord],
) -> Result<String, SnapshotError> {
    hash_canonical(&AwsEbsServiceCanonicalPayload {
        scope: aws_service_scope(currency, source_region, parser_schema_version),
        records: records
            .iter()
            .map(|record| AwsEbsCoreRecord {
                stable_key: &record.stable_key,
                rate: &record.rate,
            })
            .collect(),
    })
}

fn aws_service_scope<'a>(
    currency: &'a str,
    source_region: &'a str,
    parser_schema_version: &'a str,
) -> AwsServiceCanonicalScope<'a> {
    AwsServiceCanonicalScope {
        provider: Provider::Aws,
        currency,
        source_region,
        parser_schema_version,
    }
}

fn creation_metadata(metadata: SnapshotMetadata) -> SnapshotCreationMetadata {
    SnapshotCreationMetadata {
        status: metadata.status,
        retrieved_at: metadata.retrieved_at,
        source_published_at: metadata.source_published_at,
        currency: metadata.currency,
        source_urls: metadata.source_urls,
        parser_schema_version: metadata.parser_schema_version,
        warnings: metadata.warnings,
    }
}

#[derive(Serialize)]
struct AwsCanonicalPayload<'a> {
    provider: Provider,
    currency: &'a str,
    source_region: &'a str,
    parser_schema_version: &'a str,
    ec2_rates: &'a [AwsEc2RateRecord],
    rds_rates: &'a [AwsRdsRateRecord],
    ebs_rates: &'a [AwsEbsRateRecord],
}

#[derive(Clone, Copy, Serialize)]
struct AwsServiceCanonicalScope<'a> {
    provider: Provider,
    currency: &'a str,
    source_region: &'a str,
    parser_schema_version: &'a str,
}

#[derive(Serialize)]
struct AwsEc2ServiceCanonicalPayload<'a> {
    #[serde(flatten)]
    scope: AwsServiceCanonicalScope<'a>,
    records: Vec<AwsEc2CoreRecord<'a>>,
}

#[derive(Serialize)]
struct AwsEc2CoreRecord<'a> {
    stable_key: &'a str,
    instance_type: &'a str,
    #[serde(flatten)]
    rate: &'a Ec2Rate,
}

#[derive(Serialize)]
struct AwsRdsServiceCanonicalPayload<'a> {
    #[serde(flatten)]
    scope: AwsServiceCanonicalScope<'a>,
    records: Vec<AwsRdsCoreRecord<'a>>,
}

#[derive(Serialize)]
struct AwsRdsCoreRecord<'a> {
    stable_key: &'a str,
    instance_type: &'a str,
    deployment: RdsDeployment,
    commercial_term: &'a str,
    storage_class: &'a str,
    #[serde(flatten)]
    rate: &'a RdsRate,
}

#[derive(Serialize)]
struct AwsEbsServiceCanonicalPayload<'a> {
    #[serde(flatten)]
    scope: AwsServiceCanonicalScope<'a>,
    records: Vec<AwsEbsCoreRecord<'a>>,
}

#[derive(Serialize)]
struct AwsEbsCoreRecord<'a> {
    stable_key: &'a str,
    #[serde(flatten)]
    rate: &'a EbsRate,
}

#[derive(Serialize)]
struct AzureCanonicalPayload<'a> {
    provider: Provider,
    currency: &'a str,
    target_region: &'a str,
    parser_schema_version: &'a str,
    mi_rates: &'a [AzureMiRateRecord],
}

fn validate_creation_metadata(
    metadata: &SnapshotCreationMetadata,
    scope: &str,
) -> Result<(), SnapshotError> {
    let parsed = OffsetDateTime::parse(&metadata.retrieved_at, &Rfc3339)
        .map_err(|_| SnapshotError::InvalidRetrievedAt)?;
    if parsed.offset() != time::UtcOffset::UTC {
        return Err(SnapshotError::InvalidRetrievedAt);
    }
    if metadata.currency.is_empty() || scope.is_empty() || metadata.parser_schema_version.is_empty()
    {
        return Err(SnapshotError::InvalidScope);
    }
    if metadata.status == ResolutionStatus::Unavailable {
        return Err(SnapshotError::UnusableStatus);
    }
    Ok(())
}

fn normalize_creation_metadata(metadata: &mut SnapshotCreationMetadata) {
    metadata.source_urls.sort();
    metadata.source_urls.dedup();
    metadata.warnings.sort();
    metadata.warnings.dedup();
}

fn normalize_provenance<T>(
    records: &mut [T],
    mut provenance: impl for<'a> FnMut(&'a mut T) -> &'a mut RateProvenance,
) {
    for record in records {
        let provenance = provenance(record);
        provenance.meter_ids.sort();
        provenance.meter_ids.dedup();
    }
}

fn validate_unique_keys<'a>(keys: impl Iterator<Item = &'a str>) -> Result<(), SnapshotError> {
    let mut seen = BTreeSet::new();
    for key in keys {
        if key.is_empty() || !seen.insert(key) {
            return Err(SnapshotError::InvalidRateKeys);
        }
    }
    Ok(())
}

fn validate_rate_provenance<'a>(
    provenance: impl Iterator<Item = &'a RateProvenance>,
) -> Result<(), SnapshotError> {
    if provenance
        .into_iter()
        .any(|value| value.source_url.is_empty())
    {
        return Err(SnapshotError::InvalidRateProvenance);
    }
    Ok(())
}

fn validate_aws_rates(
    ec2_rates: &[AwsEc2RateRecord],
    rds_rates: &[AwsRdsRateRecord],
    ebs_rates: &[AwsEbsRateRecord],
) -> Result<(), SnapshotError> {
    if ec2_rates.iter().any(|record| {
        record.instance_type.is_empty()
            || record.rate.source_vcpu == 0
            || record.rate.catalog_memory_gb.0 <= Decimal::ZERO
            || record.rate.compute_hourly.0 <= Decimal::ZERO
            || record
                .rate
                .standard_license_hourly
                .is_some_and(|rate| rate.0 <= Decimal::ZERO)
            || record
                .rate
                .enterprise_license_hourly
                .is_some_and(|rate| rate.0 <= Decimal::ZERO)
    }) || rds_rates.iter().any(|record| {
        record.instance_type.is_empty()
            || record.commercial_term.is_empty()
            || record.storage_class.is_empty()
            || record.rate.source_vcpu == 0
            || record.rate.catalog_memory_gb.0 <= Decimal::ZERO
            || record.rate.effective_compute_hourly.0 <= Decimal::ZERO
            || record.rate.storage_monthly_per_gb.0 <= Decimal::ZERO
            || record.rate.standard_license_core_hourly.0 <= Decimal::ZERO
            || record.rate.enterprise_license_core_hourly.0 <= Decimal::ZERO
    }) || ebs_rates.iter().any(|record| !valid_ebs_rate(&record.rate))
    {
        return Err(SnapshotError::InvalidAwsRates);
    }
    Ok(())
}

fn valid_ebs_rate(rate: &EbsRate) -> bool {
    if rate.capacity_monthly_per_gb.0 <= Decimal::ZERO
        || rate.included_throughput_mibps.0 < Decimal::ZERO
        || rate
            .throughput_monthly_per_mibps
            .is_some_and(|value| value.0 <= Decimal::ZERO)
    {
        return false;
    }
    match rate.volume_type {
        EbsVolumeType::Gp3 => {
            rate.included_iops == 3_000
                && rate
                    .iops_monthly_per_unit
                    .is_some_and(|value| value.0 > Decimal::ZERO)
                && rate.iops_tiers.is_empty()
                && rate.included_throughput_mibps.0 == Decimal::from(125_u32)
                && rate.throughput_monthly_per_mibps.is_some()
        }
        EbsVolumeType::Io2 => {
            rate.included_iops == 0
                && rate.iops_monthly_per_unit.is_none()
                && rate.iops_tiers.len() == 3
                && rate.iops_tiers[0].up_to_inclusive == Some(32_000)
                && rate.iops_tiers[1].up_to_inclusive == Some(64_000)
                && rate.iops_tiers[2].up_to_inclusive.is_none()
                && rate
                    .iops_tiers
                    .iter()
                    .all(|tier| tier.monthly_per_iops.0 > Decimal::ZERO)
        }
        EbsVolumeType::Ephemeral => false,
    }
}

fn validate_azure_purchase_matrices(records: &[AzureMiRateRecord]) -> Result<(), SnapshotError> {
    let mut configurations = BTreeMap::<&str, Vec<&AzureMiRateRecord>>::new();
    for record in records {
        if record.configuration_key.is_empty()
            || record.rate.compute_hourly.0 < Decimal::ZERO
            || record.rate.license_hourly.0 < Decimal::ZERO
            || record.rate.storage_monthly_per_gb.0 <= Decimal::ZERO
            || record.rate.additional_memory_per_gb_hourly.0 <= Decimal::ZERO
        {
            return Err(SnapshotError::InvalidAzurePurchaseMatrix);
        }
        configurations
            .entry(&record.configuration_key)
            .or_default()
            .push(record);
    }

    for configuration in configurations.values() {
        let purchase_options = configuration
            .iter()
            .map(|record| record.purchase_option)
            .collect::<BTreeSet<_>>();
        let first = configuration[0];
        if configuration.len() != PurchaseOption::ALL.len()
            || purchase_options.len() != PurchaseOption::ALL.len()
            || !PurchaseOption::ALL
                .iter()
                .all(|option| purchase_options.contains(option))
            || configuration.iter().any(|record| {
                record.rate.storage_monthly_per_gb != first.rate.storage_monthly_per_gb
                    || record.rate.additional_memory_per_gb_hourly
                        != first.rate.additional_memory_per_gb_hourly
            })
            || !valid_azure_option_pair(configuration, PurchaseOption::Payg, PurchaseOption::Ahb)
            || !valid_azure_option_pair(
                configuration,
                PurchaseOption::OneYear,
                PurchaseOption::AhbOneYear,
            )
            || !valid_azure_option_pair(
                configuration,
                PurchaseOption::ThreeYear,
                PurchaseOption::AhbThreeYear,
            )
            || !valid_azure_option_pair(
                configuration,
                PurchaseOption::SavingsOneYear,
                PurchaseOption::AhbSavingsOneYear,
            )
        {
            return Err(SnapshotError::InvalidAzurePurchaseMatrix);
        }
    }
    Ok(())
}

fn valid_azure_option_pair(
    configuration: &[&AzureMiRateRecord],
    licensed_option: PurchaseOption,
    ahb_option: PurchaseOption,
) -> bool {
    let licensed = configuration
        .iter()
        .find(|record| record.purchase_option == licensed_option);
    let ahb = configuration
        .iter()
        .find(|record| record.purchase_option == ahb_option);
    matches!((licensed, ahb), (Some(licensed), Some(ahb))
        if licensed.rate.compute_hourly.0 > Decimal::ZERO
            && licensed.rate.compute_hourly == ahb.rate.compute_hourly
            && licensed.rate.license_hourly.0 > Decimal::ZERO
            && ahb.rate.license_hourly.0 == Decimal::ZERO)
}

fn hash_canonical(value: &impl Serialize) -> Result<String, SnapshotError> {
    let bytes = serde_json::to_vec(value).map_err(|_| SnapshotError::Canonicalization)?;
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").map_err(|_| SnapshotError::Canonicalization)?;
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rust_decimal::Decimal;

    use super::*;
    use crate::{calculation::cost::Ec2Rate, domain::decimal::DecimalValue};

    #[test]
    fn aws_service_hashes_change_only_with_that_services_core_data() {
        let (snapshot, _) = crate::pricing::local_fixture::load().expect("valid local fixture");
        let original = aws_service_content_hashes(&snapshot).expect("service hashes");

        let mut provenance_only = snapshot.clone();
        provenance_only.ec2_rates[0].provenance.source_version = Some("new-version".to_owned());
        assert_eq!(
            aws_service_content_hashes(&provenance_only).expect("provenance-only hashes"),
            original
        );

        let mut changed_ec2 = snapshot;
        changed_ec2.ec2_rates[0].rate.compute_hourly.0 += Decimal::ONE;
        let changed = aws_service_content_hashes(&changed_ec2).expect("changed hashes");
        assert_ne!(changed.ec2, original.ec2);
        assert_eq!(changed.rds, original.rds);
        assert_eq!(changed.ebs, original.ebs);
    }

    #[test]
    fn equivalent_content_has_the_same_snapshot_id() {
        let first = AwsPriceSnapshot::create(
            metadata(vec!["https://second", "https://first"]),
            "eu-west-1",
            vec![ec2_record("b"), ec2_record("a")],
            Vec::new(),
            Vec::new(),
        )
        .expect("first snapshot");
        let second = AwsPriceSnapshot::create(
            metadata(vec!["https://first", "https://second"]),
            "eu-west-1",
            vec![ec2_record("a"), ec2_record("b")],
            Vec::new(),
            Vec::new(),
        )
        .expect("second snapshot");

        assert_eq!(first.metadata.snapshot_id, second.metadata.snapshot_id);
        assert_eq!(first.metadata.content_sha256.len(), 64);
        assert_eq!(
            first.metadata.snapshot_id,
            "aws-6fa77235b9f60fa8c7056d7176add990ec63ad471c49cb79405de79a4a321bd1"
        );
    }

    #[test]
    fn incomplete_azure_purchase_matrix_is_rejected() {
        let result = AzurePriceSnapshot::create(
            metadata(vec!["https://example.invalid/azure"]),
            "swedencentral",
            vec![AzureMiRateRecord {
                stable_key: "configuration|payg".to_owned(),
                configuration_key: "configuration".to_owned(),
                purchase_option: PurchaseOption::Payg,
                rate: AzureRate {
                    compute_hourly: decimal("1"),
                    license_hourly: decimal("1"),
                    storage_monthly_per_gb: decimal("0.1"),
                    additional_memory_per_gb_hourly: decimal("0.01"),
                },
                provenance: RateProvenance {
                    source_url: "https://example.invalid/azure".to_owned(),
                    effective_at: None,
                    source_version: None,
                    meter_ids: Vec::new(),
                },
            }],
        );

        assert!(matches!(
            result,
            Err(SnapshotError::InvalidAzurePurchaseMatrix)
        ));
    }

    #[test]
    fn retrieval_time_and_cache_status_do_not_change_content_id() {
        let mut alternate = metadata(Vec::new());
        alternate.status = ResolutionStatus::Stale;
        alternate.retrieved_at = "2026-02-01T00:00:00Z".to_owned();
        let first = AwsPriceSnapshot::create(
            metadata(Vec::new()),
            "eu-west-1",
            vec![ec2_record("a")],
            Vec::new(),
            Vec::new(),
        )
        .expect("first snapshot");
        let second = AwsPriceSnapshot::create(
            alternate,
            "eu-west-1",
            vec![ec2_record("a")],
            Vec::new(),
            Vec::new(),
        )
        .expect("second snapshot");

        assert_eq!(first.metadata.snapshot_id, second.metadata.snapshot_id);
    }

    fn metadata(source_urls: Vec<&str>) -> SnapshotCreationMetadata {
        SnapshotCreationMetadata {
            status: ResolutionStatus::Fresh,
            retrieved_at: "2026-01-01T00:00:00Z".to_owned(),
            source_published_at: None,
            currency: "USD".to_owned(),
            source_urls: source_urls.into_iter().map(str::to_owned).collect(),
            parser_schema_version: "test-v1".to_owned(),
            warnings: Vec::new(),
        }
    }

    fn ec2_record(stable_key: &str) -> AwsEc2RateRecord {
        AwsEc2RateRecord {
            stable_key: stable_key.to_owned(),
            instance_type: stable_key.to_owned(),
            rate: Ec2Rate {
                source_vcpu: 4,
                catalog_memory_gb: decimal("32"),
                compute_hourly: decimal("1.25"),
                standard_license_hourly: Some(decimal("0.48")),
                enterprise_license_hourly: Some(decimal("1.5")),
            },
            provenance: RateProvenance {
                source_url: "https://example.test".to_owned(),
                effective_at: Some("2026-01-01T00:00:00Z".to_owned()),
                source_version: Some("v1".to_owned()),
                meter_ids: vec!["meter-b".to_owned(), "meter-a".to_owned()],
            },
        }
    }

    fn decimal(value: &str) -> DecimalValue {
        DecimalValue(Decimal::from_str(value).expect("valid decimal"))
    }
}
