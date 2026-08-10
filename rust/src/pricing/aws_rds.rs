use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    calculation::cost::RdsRate,
    domain::{decimal::DecimalValue, resource::RdsDeployment},
    pricing::snapshot::{AwsRdsRateRecord, RateProvenance},
};

const STANDARD_LICENSE_FALLBACK: Decimal = Decimal::from_parts(12, 0, 0, false, 2);
const ENTERPRISE_LICENSE_FALLBACK: Decimal = Decimal::from_parts(375, 0, 0, false, 3);

#[derive(Clone, Copy)]
pub struct RdsNormalizationContext<'a> {
    pub region_code: &'a str,
}

#[derive(Clone, Copy)]
pub struct RdsLeafPayload<'a> {
    pub source_url: &'a str,
    pub source_version: Option<&'a str>,
    pub effective_at: Option<&'a str>,
    pub body: &'a [u8],
}

#[derive(Debug)]
pub struct RdsNormalization {
    pub records: Vec<AwsRdsRateRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RdsNormalizationError {
    #[error("RDS selected price leaf JSON is malformed")]
    MalformedJson,
    #[error("RDS selected price leaf has an unsupported offer code")]
    UnsupportedOffer,
    #[error("RDS price or sizing value is invalid")]
    InvalidValue,
    #[error("RDS price leaf returned an unsupported price unit")]
    UnsupportedUnit,
    #[error("RDS Reserved term attributes are invalid")]
    InvalidCommercialTerm,
    #[error("RDS price leaf returned conflicting records for one normalized component")]
    ConflictingComponent,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedLeaf {
    source_offer_code: String,
    dimensions: Vec<RawDimension>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDimension {
    sku: String,
    product_family: String,
    #[serde(default)]
    database_engine: Option<String>,
    #[serde(default)]
    database_edition: Option<String>,
    #[serde(default)]
    license_model: Option<String>,
    #[serde(default)]
    license_type: Option<String>,
    #[serde(default)]
    deployment_model: Option<String>,
    #[serde(default)]
    deployment_option: Option<String>,
    #[serde(default)]
    instance_type: Option<String>,
    #[serde(default)]
    memory: Option<String>,
    #[serde(default)]
    vcpu: Option<String>,
    #[serde(default)]
    volume_name: Option<String>,
    #[serde(default)]
    volume_type: Option<String>,
    term_type: String,
    offer_term_code: String,
    #[serde(default)]
    lease_contract_length: Option<String>,
    #[serde(default)]
    purchase_option: Option<String>,
    #[serde(default)]
    offering_class: Option<String>,
    rate_code: String,
    unit: String,
    price: serde_json::Value,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OfferKey {
    sku: String,
    offer_term_code: String,
    commercial_term: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ComputeKey {
    instance_type: String,
    deployment: Deployment,
    commercial_term: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StorageKey {
    deployment: Deployment,
    storage_class: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Deployment {
    SingleAz,
    MultiAz,
}

impl Deployment {
    fn as_key(self) -> &'static str {
        match self {
            Self::SingleAz => "single-az",
            Self::MultiAz => "multi-az",
        }
    }

    fn as_domain(self) -> RdsDeployment {
        match self {
            Self::SingleAz => RdsDeployment::SingleAz,
            Self::MultiAz => RdsDeployment::MultiAz,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceIdentity {
    source_url: String,
    source_version: Option<String>,
    effective_at: Option<String>,
}

#[derive(Clone)]
struct ComputeAccumulator {
    key: ComputeKey,
    source_vcpu: u32,
    memory_gb: DecimalValue,
    recurring_hourly: Decimal,
    upfront: Decimal,
    meter_ids: BTreeSet<String>,
    source: SourceIdentity,
}

#[derive(Clone)]
struct ComputeComponent {
    source_vcpu: u32,
    memory_gb: DecimalValue,
    effective_hourly: Decimal,
    meter_ids: BTreeSet<String>,
    source: SourceIdentity,
}

#[derive(Clone)]
struct StorageComponent {
    monthly_per_gb: Decimal,
    meter_ids: BTreeSet<String>,
}

#[derive(Clone)]
struct LicenseComponent {
    core_hourly: Decimal,
    meter_ids: BTreeSet<String>,
}

pub fn normalize_rds_leaves(
    context: RdsNormalizationContext<'_>,
    leaves: &[RdsLeafPayload<'_>],
) -> Result<RdsNormalization, RdsNormalizationError> {
    if context.region_code.is_empty() {
        return Err(RdsNormalizationError::InvalidValue);
    }

    let mut compute_offers = BTreeMap::<OfferKey, ComputeAccumulator>::new();
    let mut storage = BTreeMap::<StorageKey, StorageComponent>::new();
    let mut standard_license = None;
    let mut enterprise_license = None;

    for payload in leaves {
        if payload.source_url.is_empty() {
            return Err(RdsNormalizationError::InvalidValue);
        }
        let leaf: SelectedLeaf = serde_json::from_slice(payload.body)
            .map_err(|_| RdsNormalizationError::MalformedJson)?;
        match leaf.source_offer_code.as_str() {
            "AmazonRDS" => {
                for dimension in &leaf.dimensions {
                    if is_compute_dimension(dimension) {
                        add_compute_dimension(&mut compute_offers, payload, dimension)?;
                    } else if is_storage_dimension(dimension) {
                        add_storage_dimension(&mut storage, dimension)?;
                    }
                }
            }
            "AmazonRDSOCPULicenseFees" => {
                for dimension in &leaf.dimensions {
                    add_license_dimension(
                        dimension,
                        &mut standard_license,
                        &mut enterprise_license,
                    )?;
                }
            }
            _ => return Err(RdsNormalizationError::UnsupportedOffer),
        }
    }

    let compute = collapse_compute_offers(compute_offers)?;
    let mut warnings = Vec::new();
    let standard_license = license_or_fallback(
        standard_license,
        STANDARD_LICENSE_FALLBACK,
        "Standard",
        &mut warnings,
    );
    let enterprise_license = license_or_fallback(
        enterprise_license,
        ENTERPRISE_LICENSE_FALLBACK,
        "Enterprise",
        &mut warnings,
    );

    let mut records = Vec::new();
    for (compute_key, compute_component) in compute {
        let mut matched_storage = false;
        for (storage_key, storage_component) in &storage {
            if storage_key.deployment != compute_key.deployment {
                continue;
            }
            matched_storage = true;
            let mut meter_ids = compute_component.meter_ids.clone();
            meter_ids.extend(storage_component.meter_ids.iter().cloned());
            meter_ids.extend(standard_license.meter_ids.iter().cloned());
            meter_ids.extend(enterprise_license.meter_ids.iter().cloned());

            records.push(AwsRdsRateRecord {
                stable_key: format!(
                    "{}|{}|{}|{}|{}",
                    context.region_code,
                    compute_key.instance_type,
                    compute_key.deployment.as_key(),
                    compute_key.commercial_term,
                    storage_key.storage_class
                ),
                instance_type: compute_key.instance_type.clone(),
                deployment: compute_key.deployment.as_domain(),
                commercial_term: compute_key.commercial_term.clone(),
                storage_class: storage_key.storage_class.clone(),
                rate: RdsRate {
                    source_vcpu: compute_component.source_vcpu,
                    catalog_memory_gb: compute_component.memory_gb,
                    effective_compute_hourly: DecimalValue(compute_component.effective_hourly),
                    storage_monthly_per_gb: DecimalValue(storage_component.monthly_per_gb),
                    standard_license_core_hourly: DecimalValue(standard_license.core_hourly),
                    enterprise_license_core_hourly: DecimalValue(enterprise_license.core_hourly),
                },
                provenance: RateProvenance {
                    source_url: compute_component.source.source_url.clone(),
                    effective_at: compute_component.source.effective_at.clone(),
                    source_version: compute_component.source.source_version.clone(),
                    meter_ids: meter_ids.into_iter().collect(),
                },
            });
        }
        if !matched_storage {
            warnings.push(format!(
                "RDS {} {} has no matching SQL Server storage rate.",
                compute_key.instance_type,
                compute_key.deployment.as_key()
            ));
        }
    }

    warnings.sort();
    warnings.dedup();
    records.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
    Ok(RdsNormalization { records, warnings })
}

fn is_compute_dimension(dimension: &RawDimension) -> bool {
    dimension.product_family == "Database Instance"
        && dimension.database_engine.as_deref() == Some("SQL Server")
        && matches!(
            dimension.database_edition.as_deref(),
            Some("Standard" | "Enterprise")
        )
        && (dimension.deployment_model.as_deref() == Some("Custom")
            || dimension.license_model.as_deref() == Some("Bring your own media"))
}

fn is_storage_dimension(dimension: &RawDimension) -> bool {
    dimension.product_family == "Database Storage"
        && dimension.database_engine.as_deref() == Some("SQL Server")
        && matches!(
            dimension.database_edition.as_deref(),
            Some("Standard" | "Enterprise")
        )
}

fn add_compute_dimension(
    offers: &mut BTreeMap<OfferKey, ComputeAccumulator>,
    payload: &RdsLeafPayload<'_>,
    dimension: &RawDimension,
) -> Result<(), RdsNormalizationError> {
    let instance_type = required(dimension.instance_type.as_deref())?;
    let deployment = parse_deployment(required(dimension.deployment_option.as_deref())?)?;
    let commercial_term = commercial_term(dimension)?;
    let source_vcpu = required(dimension.vcpu.as_deref())?
        .parse::<u32>()
        .map_err(|_| RdsNormalizationError::InvalidValue)?;
    let memory_gb = parse_memory(required(dimension.memory.as_deref())?)?;
    if source_vcpu == 0 {
        return Err(RdsNormalizationError::InvalidValue);
    }
    let price = parse_nonnegative_decimal(&dimension.price)?;
    let (recurring_hourly, upfront) = match dimension.unit.as_str() {
        "Hrs" => (price, Decimal::ZERO),
        "Quantity" if dimension.term_type == "Reserved" => (Decimal::ZERO, price),
        _ => return Err(RdsNormalizationError::UnsupportedUnit),
    };
    let key = OfferKey {
        sku: dimension.sku.clone(),
        offer_term_code: dimension.offer_term_code.clone(),
        commercial_term: commercial_term.clone(),
    };
    let source = SourceIdentity {
        source_url: payload.source_url.to_owned(),
        source_version: payload.source_version.map(str::to_owned),
        effective_at: payload.effective_at.map(str::to_owned),
    };
    let component = offers.entry(key).or_insert_with(|| ComputeAccumulator {
        key: ComputeKey {
            instance_type: instance_type.to_owned(),
            deployment,
            commercial_term,
        },
        source_vcpu,
        memory_gb,
        recurring_hourly: Decimal::ZERO,
        upfront: Decimal::ZERO,
        meter_ids: BTreeSet::new(),
        source: source.clone(),
    });
    if component.key.instance_type != instance_type
        || component.key.deployment != deployment
        || component.source_vcpu != source_vcpu
        || component.memory_gb != memory_gb
        || component.source != source
    {
        return Err(RdsNormalizationError::ConflictingComponent);
    }
    component.recurring_hourly += recurring_hourly;
    component.upfront += upfront;
    component.meter_ids.insert(dimension.rate_code.clone());
    Ok(())
}

fn add_storage_dimension(
    storage: &mut BTreeMap<StorageKey, StorageComponent>,
    dimension: &RawDimension,
) -> Result<(), RdsNormalizationError> {
    if dimension.term_type != "OnDemand" || dimension.unit != "GB-Mo" {
        return Err(RdsNormalizationError::UnsupportedUnit);
    }
    let deployment = parse_deployment(required(dimension.deployment_option.as_deref())?)?;
    let storage_class = storage_class(dimension)?;
    let monthly_per_gb = parse_nonnegative_decimal(&dimension.price)?;
    let component = storage
        .entry(StorageKey {
            deployment,
            storage_class,
        })
        .or_insert_with(|| StorageComponent {
            monthly_per_gb,
            meter_ids: BTreeSet::new(),
        });
    if component.monthly_per_gb != monthly_per_gb {
        return Err(RdsNormalizationError::ConflictingComponent);
    }
    component.meter_ids.insert(dimension.rate_code.clone());
    Ok(())
}

fn add_license_dimension(
    dimension: &RawDimension,
    standard: &mut Option<LicenseComponent>,
    enterprise: &mut Option<LicenseComponent>,
) -> Result<(), RdsNormalizationError> {
    if dimension.product_family != "Optimized License"
        || dimension.license_type.as_deref() != Some("SQLServer")
    {
        return Ok(());
    }
    if dimension.term_type != "OnDemand" || dimension.unit != "vCPU-Hour" {
        return Err(RdsNormalizationError::UnsupportedUnit);
    }
    let slot = match dimension.database_edition.as_deref() {
        Some("Standard") => standard,
        Some("Enterprise") => enterprise,
        _ => return Ok(()),
    };
    let core_hourly = parse_nonnegative_decimal(&dimension.price)?;
    if core_hourly <= Decimal::ZERO {
        return Err(RdsNormalizationError::InvalidValue);
    }
    match slot {
        Some(component) if component.core_hourly != core_hourly => {
            return Err(RdsNormalizationError::ConflictingComponent);
        }
        Some(component) => {
            component.meter_ids.insert(dimension.rate_code.clone());
        }
        None => {
            let mut meter_ids = BTreeSet::new();
            meter_ids.insert(dimension.rate_code.clone());
            *slot = Some(LicenseComponent {
                core_hourly,
                meter_ids,
            });
        }
    }
    Ok(())
}

fn collapse_compute_offers(
    offers: BTreeMap<OfferKey, ComputeAccumulator>,
) -> Result<BTreeMap<ComputeKey, ComputeComponent>, RdsNormalizationError> {
    let mut compute = BTreeMap::<ComputeKey, ComputeComponent>::new();
    for offer in offers.into_values() {
        let effective_hourly = offer.recurring_hourly
            + offer.upfront / commercial_term_hours(&offer.key.commercial_term)?;
        let component = compute
            .entry(offer.key)
            .or_insert_with(|| ComputeComponent {
                source_vcpu: offer.source_vcpu,
                memory_gb: offer.memory_gb,
                effective_hourly,
                meter_ids: BTreeSet::new(),
                source: offer.source.clone(),
            });
        if component.source_vcpu != offer.source_vcpu
            || component.memory_gb != offer.memory_gb
            || component.effective_hourly != effective_hourly
            || component.source != offer.source
        {
            return Err(RdsNormalizationError::ConflictingComponent);
        }
        component.meter_ids.extend(offer.meter_ids);
    }
    Ok(compute)
}

fn commercial_term(dimension: &RawDimension) -> Result<String, RdsNormalizationError> {
    match dimension.term_type.as_str() {
        "OnDemand" => Ok("on-demand".to_owned()),
        "Reserved" => {
            let lease = match required(dimension.lease_contract_length.as_deref())? {
                "1yr" => "1yr",
                "3yr" => "3yr",
                _ => return Err(RdsNormalizationError::InvalidCommercialTerm),
            };
            let purchase = normalize_token(required(dimension.purchase_option.as_deref())?)?;
            let offering = normalize_token(required(dimension.offering_class.as_deref())?)?;
            Ok(format!("reserved-{lease}-{offering}-{purchase}"))
        }
        _ => Err(RdsNormalizationError::InvalidCommercialTerm),
    }
}

fn commercial_term_hours(term: &str) -> Result<Decimal, RdsNormalizationError> {
    if term == "on-demand" {
        return Ok(Decimal::ONE);
    }
    if term.starts_with("reserved-1yr-") {
        return Ok(Decimal::from(8_760_u32));
    }
    if term.starts_with("reserved-3yr-") {
        return Ok(Decimal::from(26_280_u32));
    }
    Err(RdsNormalizationError::InvalidCommercialTerm)
}

fn normalize_token(value: &str) -> Result<String, RdsNormalizationError> {
    let mut normalized = String::new();
    let mut previous_separator = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator && !normalized.is_empty() {
            normalized.push('-');
            previous_separator = true;
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    if normalized.is_empty() {
        return Err(RdsNormalizationError::InvalidCommercialTerm);
    }
    Ok(normalized)
}

fn parse_deployment(value: &str) -> Result<Deployment, RdsNormalizationError> {
    if value == "Single-AZ" {
        Ok(Deployment::SingleAz)
    } else if value.starts_with("Multi-AZ") {
        Ok(Deployment::MultiAz)
    } else {
        Err(RdsNormalizationError::InvalidValue)
    }
}

fn storage_class(dimension: &RawDimension) -> Result<String, RdsNormalizationError> {
    if let Some(name) = dimension
        .volume_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return normalize_token(name).map_err(|_| RdsNormalizationError::InvalidValue);
    }
    match dimension.volume_type.as_deref() {
        Some("General Purpose") | Some("General Purpose (SSD)") => Ok("gp2".to_owned()),
        Some("General Purpose-GP3") | Some("General Purpose-GP3 SSD") => Ok("gp3".to_owned()),
        Some("Provisioned IOPS") | Some("Provisioned IOPS (SSD)") => Ok("io1".to_owned()),
        Some("Provisioned IOPS-IO2") => Ok("io2".to_owned()),
        Some("Magnetic") => Ok("magnetic".to_owned()),
        _ => Err(RdsNormalizationError::InvalidValue),
    }
}

fn parse_memory(value: &str) -> Result<DecimalValue, RdsNormalizationError> {
    let raw = value
        .strip_suffix(" GiB")
        .ok_or(RdsNormalizationError::InvalidValue)?;
    let memory = Decimal::from_str(raw).map_err(|_| RdsNormalizationError::InvalidValue)?;
    if memory <= Decimal::ZERO {
        return Err(RdsNormalizationError::InvalidValue);
    }
    Ok(DecimalValue(memory))
}

fn parse_nonnegative_decimal(value: &serde_json::Value) -> Result<Decimal, RdsNormalizationError> {
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let value = Decimal::from_str(&raw).map_err(|_| RdsNormalizationError::InvalidValue)?;
    if value < Decimal::ZERO {
        return Err(RdsNormalizationError::InvalidValue);
    }
    Ok(value)
}

fn required(value: Option<&str>) -> Result<&str, RdsNormalizationError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or(RdsNormalizationError::InvalidValue)
}

fn license_or_fallback(
    component: Option<LicenseComponent>,
    fallback: Decimal,
    edition: &str,
    warnings: &mut Vec<String>,
) -> LicenseComponent {
    component.unwrap_or_else(|| {
        warnings.push(format!(
            "RDS {edition} OCPU license meter is unavailable; using the workbook-compatible fallback of {fallback} USD per source vCPU-hour."
        ));
        LicenseComponent {
            core_hourly: fallback,
            meter_ids: BTreeSet::new(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_byom_compute_storage_and_ocpu_license_rates() {
        let rds = leaf(
            "AmazonRDS",
            &[
                compute(
                    "standard-sku",
                    "base-standard",
                    "Standard",
                    "Hrs",
                    "5.104",
                    None,
                ),
                compute(
                    "enterprise-sku",
                    "base-enterprise",
                    "Enterprise",
                    "Hrs",
                    "5.104",
                    None,
                ),
                storage("gp3-standard", "Standard", "0.127"),
                storage("gp3-enterprise", "Enterprise", "0.127"),
            ],
        );
        let licenses = leaf(
            "AmazonRDSOCPULicenseFees",
            &[
                license("standard-license", "Standard", "0.12"),
                license("enterprise-license", "Enterprise", "0.375"),
            ],
        );

        let normalized = normalize_rds_leaves(
            context(),
            &[payload(&rds, "AmazonRDS"), payload(&licenses, "OCPU")],
        )
        .expect("normalize selected RDS leaves");

        assert_eq!(normalized.records.len(), 1);
        assert!(normalized.warnings.is_empty());
        let record = &normalized.records[0];
        assert_eq!(
            record.stable_key,
            "eu-west-1|db.m6i.8xlarge|single-az|on-demand|gp3"
        );
        assert_eq!(record.rate.effective_compute_hourly.to_string(), "5.104");
        assert_eq!(record.rate.storage_monthly_per_gb.to_string(), "0.127");
        assert_eq!(record.rate.standard_license_core_hourly.to_string(), "0.12");
        assert_eq!(
            record.rate.enterprise_license_core_hourly.to_string(),
            "0.375"
        );
        assert_eq!(record.provenance.meter_ids.len(), 6);
    }

    #[test]
    fn amortizes_reserved_upfront_and_uses_explicit_license_fallbacks() {
        let rds = leaf(
            "AmazonRDS",
            &[
                compute(
                    "reserved-sku",
                    "reserved-hourly",
                    "Standard",
                    "Hrs",
                    "2.00",
                    Some(("1yr", "All Upfront", "standard")),
                ),
                compute(
                    "reserved-sku",
                    "reserved-upfront",
                    "Standard",
                    "Quantity",
                    "8760.00",
                    Some(("1yr", "All Upfront", "standard")),
                ),
                storage("gp3", "Standard", "0.127"),
            ],
        );

        let normalized = normalize_rds_leaves(context(), &[payload(&rds, "AmazonRDS")])
            .expect("normalize Reserved RDS leaf");

        assert_eq!(normalized.records.len(), 1);
        assert_eq!(normalized.warnings.len(), 2);
        assert_eq!(
            normalized.records[0].commercial_term,
            "reserved-1yr-standard-all-upfront"
        );
        assert_eq!(
            normalized.records[0]
                .rate
                .effective_compute_hourly
                .to_string(),
            "3.00"
        );
    }

    #[test]
    fn rejects_non_hourly_compute_dimensions() {
        let rds = leaf(
            "AmazonRDS",
            &[compute(
                "bad-sku", "bad-unit", "Standard", "Requests", "1.00", None,
            )],
        );

        assert!(matches!(
            normalize_rds_leaves(context(), &[payload(&rds, "AmazonRDS")]),
            Err(RdsNormalizationError::UnsupportedUnit)
        ));
    }

    fn context() -> RdsNormalizationContext<'static> {
        RdsNormalizationContext {
            region_code: "eu-west-1",
        }
    }

    fn payload<'a>(body: &'a [u8], version: &'static str) -> RdsLeafPayload<'a> {
        RdsLeafPayload {
            source_url: "https://example.invalid/rds-selected-leaf",
            source_version: Some(version),
            effective_at: Some("2026-01-01T00:00:00Z"),
            body,
        }
    }

    fn leaf(offer: &str, dimensions: &[serde_json::Value]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "source_offer_code": offer,
            "dimensions": dimensions
        }))
        .expect("serialize selected RDS leaf")
    }

    fn compute(
        sku: &str,
        rate_code: &str,
        edition: &str,
        unit: &str,
        price: &str,
        reserved: Option<(&str, &str, &str)>,
    ) -> serde_json::Value {
        let (term_type, lease, purchase, offering) = match reserved {
            Some((lease, purchase, offering)) => {
                ("Reserved", Some(lease), Some(purchase), Some(offering))
            }
            None => ("OnDemand", None, None, None),
        };
        serde_json::json!({
            "sku": sku,
            "product_family": "Database Instance",
            "database_engine": "SQL Server",
            "database_edition": edition,
            "license_model": "NA",
            "deployment_model": "Custom",
            "deployment_option": "Single-AZ",
            "instance_type": "db.m6i.8xlarge",
            "memory": "128 GiB",
            "vcpu": "32",
            "term_type": term_type,
            "offer_term_code": "offer",
            "lease_contract_length": lease,
            "purchase_option": purchase,
            "offering_class": offering,
            "rate_code": rate_code,
            "unit": unit,
            "price": price
        })
    }

    fn storage(rate_code: &str, edition: &str, price: &str) -> serde_json::Value {
        serde_json::json!({
            "sku": format!("storage-{edition}"),
            "product_family": "Database Storage",
            "database_engine": "SQL Server",
            "database_edition": edition,
            "deployment_option": "Single-AZ",
            "volume_name": "gp3",
            "volume_type": "General Purpose-GP3",
            "term_type": "OnDemand",
            "offer_term_code": "offer",
            "rate_code": rate_code,
            "unit": "GB-Mo",
            "price": price
        })
    }

    fn license(rate_code: &str, edition: &str, price: &str) -> serde_json::Value {
        serde_json::json!({
            "sku": format!("license-{edition}"),
            "product_family": "Optimized License",
            "database_edition": edition,
            "license_type": "SQLServer",
            "term_type": "OnDemand",
            "offer_term_code": "offer",
            "rate_code": rate_code,
            "unit": "vCPU-Hour",
            "price": price
        })
    }
}
