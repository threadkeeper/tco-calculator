use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    calculation::cost::Ec2Rate,
    domain::decimal::DecimalValue,
    pricing::snapshot::{AwsEc2RateRecord, RateProvenance},
};

#[derive(Clone, Copy)]
pub struct Ec2NormalizationContext<'a> {
    pub region_code: &'a str,
    pub location: &'a str,
    pub effective_at: Option<&'a str>,
    pub source_version: Option<&'a str>,
}

#[derive(Clone, Copy)]
pub struct Ec2LeafPayload<'a> {
    pub source_url: &'a str,
    pub body: &'a [u8],
}

#[derive(Debug)]
pub struct Ec2Normalization {
    pub records: Vec<AwsEc2RateRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum Ec2NormalizationError {
    #[error("EC2 calculator leaf JSON is malformed")]
    MalformedJson,
    #[error("EC2 calculator leaf does not contain the requested location")]
    MissingLocation,
    #[error("EC2 calculator rate code is malformed")]
    InvalidRateCode,
    #[error("EC2 calculator price or sizing value is invalid")]
    InvalidValue,
    #[error("EC2 calculator returned an unsupported price unit")]
    UnsupportedUnit,
    #[error("EC2 calculator returned conflicting records for one instance component")]
    ConflictingComponent,
}

#[derive(Deserialize)]
struct Ec2Leaf {
    regions: BTreeMap<String, BTreeMap<String, RawDimension>>,
}

#[derive(Clone, Deserialize)]
struct RawDimension {
    #[serde(rename = "rateCode")]
    rate_code: String,
    price: serde_json::Value,
    #[serde(rename = "Unit")]
    unit: String,
    #[serde(rename = "Instance Type")]
    instance_type: String,
    #[serde(rename = "Memory")]
    memory: String,
    #[serde(rename = "vCPU")]
    vcpu: String,
    #[serde(rename = "Physical Processor")]
    physical_processor: String,
    #[serde(rename = "Operating System")]
    operating_system: String,
    #[serde(rename = "Pre Installed S/W")]
    preinstalled_software: String,
    #[serde(rename = "TermType")]
    term_type: String,
    #[serde(rename = "Tenancy")]
    tenancy: String,
    #[serde(rename = "Current Generation")]
    current_generation: String,
    #[serde(rename = "License Model")]
    license_model: String,
}

#[derive(Clone)]
struct SkuComponent {
    instance_type: String,
    software: SoftwareComponent,
    source_vcpu: u32,
    memory_gb: DecimalValue,
    hourly: Decimal,
    meter_ids: BTreeSet<String>,
    source_url: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SoftwareComponent {
    Compute,
    Standard,
    Enterprise,
}

#[derive(Default)]
struct InstanceComponents {
    source_vcpu: Option<u32>,
    memory_gb: Option<DecimalValue>,
    compute: Option<SkuComponent>,
    standard: Option<SkuComponent>,
    enterprise: Option<SkuComponent>,
}

pub fn normalize_ec2_leaves(
    context: Ec2NormalizationContext<'_>,
    leaves: &[Ec2LeafPayload<'_>],
) -> Result<Ec2Normalization, Ec2NormalizationError> {
    if context.region_code.is_empty() || context.location.is_empty() {
        return Err(Ec2NormalizationError::InvalidValue);
    }

    let mut sku_components = BTreeMap::<String, SkuComponent>::new();
    for payload in leaves {
        let leaf: Ec2Leaf = serde_json::from_slice(payload.body)
            .map_err(|_| Ec2NormalizationError::MalformedJson)?;
        let dimensions = leaf
            .regions
            .get(context.location)
            .ok_or(Ec2NormalizationError::MissingLocation)?;
        for dimension in dimensions
            .values()
            .filter(|dimension| is_supported(dimension))
        {
            let (sku, _) = parse_rate_code(&dimension.rate_code)?;
            let software = software_component(&dimension.preinstalled_software)
                .ok_or(Ec2NormalizationError::InvalidValue)?;
            let source_vcpu = dimension
                .vcpu
                .parse::<u32>()
                .map_err(|_| Ec2NormalizationError::InvalidValue)?;
            let memory_gb = parse_memory(&dimension.memory)?;
            if source_vcpu == 0 || dimension.instance_type.is_empty() {
                return Err(Ec2NormalizationError::InvalidValue);
            }
            if dimension.unit != "Hrs" {
                return Err(Ec2NormalizationError::UnsupportedUnit);
            }
            let price = parse_decimal(&dimension.price)?;
            if price < Decimal::ZERO {
                return Err(Ec2NormalizationError::InvalidValue);
            }

            let component = sku_components
                .entry(sku.to_owned())
                .or_insert_with(|| SkuComponent {
                    instance_type: dimension.instance_type.clone(),
                    software,
                    source_vcpu,
                    memory_gb,
                    hourly: Decimal::ZERO,
                    meter_ids: BTreeSet::new(),
                    source_url: payload.source_url.to_owned(),
                });
            if component.instance_type != dimension.instance_type
                || component.software != software
                || component.source_vcpu != source_vcpu
                || component.memory_gb != memory_gb
                || component.source_url != payload.source_url
            {
                return Err(Ec2NormalizationError::ConflictingComponent);
            }
            component.hourly += price;
            component.meter_ids.insert(dimension.rate_code.clone());
        }
    }

    let mut instances = BTreeMap::<String, InstanceComponents>::new();
    for component in sku_components.into_values() {
        let instance = instances
            .entry(component.instance_type.clone())
            .or_default();
        if instance
            .source_vcpu
            .is_some_and(|value| value != component.source_vcpu)
            || instance
                .memory_gb
                .is_some_and(|value| value != component.memory_gb)
        {
            return Err(Ec2NormalizationError::ConflictingComponent);
        }
        instance.source_vcpu = Some(component.source_vcpu);
        instance.memory_gb = Some(component.memory_gb);
        let slot = match component.software {
            SoftwareComponent::Compute => &mut instance.compute,
            SoftwareComponent::Standard => &mut instance.standard,
            SoftwareComponent::Enterprise => &mut instance.enterprise,
        };
        if slot.replace(component).is_some() {
            return Err(Ec2NormalizationError::ConflictingComponent);
        }
    }

    let fallback_rates = regional_license_fallbacks(&instances)?;
    let mut warnings = Vec::new();
    let mut records = Vec::new();
    for (instance_type, components) in instances {
        let Some(compute) = components.compute else {
            continue;
        };
        let source_vcpu = components
            .source_vcpu
            .ok_or(Ec2NormalizationError::InvalidValue)?;
        let memory_gb = components
            .memory_gb
            .ok_or(Ec2NormalizationError::InvalidValue)?;
        let billable_cores = Decimal::from(source_vcpu.max(4));
        let standard_license_hourly = license_delta(&compute, components.standard.as_ref())?
            .or_else(|| fallback_rates.standard.map(|rate| billable_cores * rate));
        let enterprise_license_hourly = license_delta(&compute, components.enterprise.as_ref())?
            .or_else(|| fallback_rates.enterprise.map(|rate| billable_cores * rate));
        if components.standard.is_none() && standard_license_hourly.is_some() {
            warnings.push(format!(
                "EC2 Standard SQL rate for {instance_type} uses the regional four-core-minimum fallback."
            ));
        }
        if components.enterprise.is_none() && enterprise_license_hourly.is_some() {
            warnings.push(format!(
                "EC2 Enterprise SQL rate for {instance_type} uses the regional four-core-minimum fallback."
            ));
        }

        let mut meter_ids = compute.meter_ids.into_iter().collect::<Vec<_>>();
        meter_ids.extend(
            components
                .standard
                .iter()
                .flat_map(|component| component.meter_ids.iter().cloned()),
        );
        meter_ids.extend(
            components
                .enterprise
                .iter()
                .flat_map(|component| component.meter_ids.iter().cloned()),
        );
        meter_ids.sort();
        meter_ids.dedup();

        records.push(AwsEc2RateRecord {
            stable_key: format!(
                "{}|on-demand|shared|windows|{}",
                context.region_code, instance_type
            ),
            instance_type,
            rate: Ec2Rate {
                source_vcpu,
                catalog_memory_gb: memory_gb,
                compute_hourly: DecimalValue(compute.hourly),
                standard_license_hourly: standard_license_hourly.map(DecimalValue),
                enterprise_license_hourly: enterprise_license_hourly.map(DecimalValue),
            },
            provenance: RateProvenance {
                source_url: compute.source_url,
                effective_at: context.effective_at.map(str::to_owned),
                source_version: context.source_version.map(str::to_owned),
                meter_ids,
            },
        });
    }
    warnings.sort();
    records.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
    Ok(Ec2Normalization { records, warnings })
}

fn is_supported(dimension: &RawDimension) -> bool {
    dimension.operating_system == "Windows"
        && dimension.tenancy == "Shared"
        && dimension.current_generation == "Yes"
        && dimension.term_type == "OnDemand"
        && dimension.license_model == "No License required"
        && matches!(
            dimension.preinstalled_software.as_str(),
            "NA" | "SQL Std" | "SQL Ent"
        )
        && !dimension.physical_processor.contains("Graviton")
        && !dimension.physical_processor.contains("ARM")
}

fn software_component(value: &str) -> Option<SoftwareComponent> {
    match value {
        "NA" => Some(SoftwareComponent::Compute),
        "SQL Std" => Some(SoftwareComponent::Standard),
        "SQL Ent" => Some(SoftwareComponent::Enterprise),
        _ => None,
    }
}

fn parse_rate_code(value: &str) -> Result<(&str, &str), Ec2NormalizationError> {
    let mut parts = value.split('.');
    let sku = parts.next().filter(|part| !part.is_empty());
    let offer = parts.next().filter(|part| !part.is_empty());
    match (sku, offer) {
        (Some(sku), Some(offer)) => Ok((sku, offer)),
        _ => Err(Ec2NormalizationError::InvalidRateCode),
    }
}

fn parse_memory(value: &str) -> Result<DecimalValue, Ec2NormalizationError> {
    let raw = value
        .strip_suffix(" GiB")
        .ok_or(Ec2NormalizationError::InvalidValue)?;
    let memory = Decimal::from_str(raw).map_err(|_| Ec2NormalizationError::InvalidValue)?;
    if memory <= Decimal::ZERO {
        return Err(Ec2NormalizationError::InvalidValue);
    }
    Ok(DecimalValue(memory))
}

fn parse_decimal(value: &serde_json::Value) -> Result<Decimal, Ec2NormalizationError> {
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    Decimal::from_str(&raw).map_err(|_| Ec2NormalizationError::InvalidValue)
}

#[derive(Default)]
struct LicenseFallbacks {
    standard: Option<Decimal>,
    enterprise: Option<Decimal>,
}

fn regional_license_fallbacks(
    instances: &BTreeMap<String, InstanceComponents>,
) -> Result<LicenseFallbacks, Ec2NormalizationError> {
    let mut standard = Vec::new();
    let mut enterprise = Vec::new();
    for components in instances.values() {
        let Some(compute) = components.compute.as_ref() else {
            continue;
        };
        let source_vcpu = components
            .source_vcpu
            .ok_or(Ec2NormalizationError::InvalidValue)?;
        let billable_cores = Decimal::from(source_vcpu.max(4));
        if let Some(delta) = license_delta(compute, components.standard.as_ref())? {
            standard.push(delta / billable_cores);
        }
        if let Some(delta) = license_delta(compute, components.enterprise.as_ref())? {
            enterprise.push(delta / billable_cores);
        }
    }
    Ok(LicenseFallbacks {
        standard: standard.into_iter().min(),
        enterprise: enterprise.into_iter().min(),
    })
}

fn license_delta(
    compute: &SkuComponent,
    licensed: Option<&SkuComponent>,
) -> Result<Option<Decimal>, Ec2NormalizationError> {
    let Some(licensed) = licensed else {
        return Ok(None);
    };
    let delta = licensed.hourly - compute.hourly;
    if delta <= Decimal::ZERO {
        return Err(Ec2NormalizationError::InvalidValue);
    }
    Ok(Some(delta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_components_and_filters_arm_shapes() {
        let body = leaf(&[
            dimension("base", "m.test", "NA", "2.00", "Hrs", "Intel Xeon", 8),
            dimension("std", "m.test", "SQL Std", "3.20", "Hrs", "Intel Xeon", 8),
            dimension("ent", "m.test", "SQL Ent", "5.00", "Hrs", "Intel Xeon", 8),
            dimension("arm", "m.arm", "NA", "1.00", "Hrs", "AWS Graviton", 8),
        ]);

        let normalized =
            normalize_ec2_leaves(context(), &[payload(&body)]).expect("normalize synthetic leaf");

        assert_eq!(normalized.records.len(), 1);
        let rate = normalized.records[0].rate;
        assert_eq!(rate.compute_hourly.to_string(), "2.00");
        assert_eq!(
            rate.standard_license_hourly.map(|value| value.to_string()),
            Some("1.20".to_owned())
        );
        assert_eq!(
            rate.enterprise_license_hourly
                .map(|value| value.to_string()),
            Some("3.00".to_owned())
        );
        assert!(normalized.warnings.is_empty());
    }

    #[test]
    fn missing_shape_license_uses_regional_four_core_fallback() {
        let body = leaf(&[
            dimension("small-base", "m.small", "NA", "1.00", "Hrs", "Intel", 2),
            dimension("small-std", "m.small", "SQL Std", "2.00", "Hrs", "Intel", 2),
            dimension("large-base", "m.large", "NA", "2.00", "Hrs", "Intel", 8),
        ]);

        let normalized =
            normalize_ec2_leaves(context(), &[payload(&body)]).expect("normalize fallback leaf");
        let large = normalized
            .records
            .iter()
            .find(|record| record.instance_type == "m.large")
            .expect("large shape");

        assert_eq!(
            large
                .rate
                .standard_license_hourly
                .map(|value| value.to_string()),
            Some("2.00".to_owned())
        );
        assert_eq!(normalized.warnings.len(), 1);
    }

    #[test]
    fn rejects_unknown_units_on_supported_rows() {
        let body = leaf(&[dimension(
            "base", "m.test", "NA", "2.00", "Requests", "Intel", 8,
        )]);

        assert!(matches!(
            normalize_ec2_leaves(context(), &[payload(&body)]),
            Err(Ec2NormalizationError::UnsupportedUnit)
        ));
    }

    fn context() -> Ec2NormalizationContext<'static> {
        Ec2NormalizationContext {
            region_code: "eu-west-1",
            location: "EU (Ireland)",
            effective_at: Some("2026-01-01T00:00:00Z"),
            source_version: Some("test-v1"),
        }
    }

    fn payload(body: &[u8]) -> Ec2LeafPayload<'_> {
        Ec2LeafPayload {
            source_url: "https://example.invalid/ec2-leaf",
            body,
        }
    }

    fn leaf(dimensions: &[serde_json::Value]) -> Vec<u8> {
        let dimensions = dimensions
            .iter()
            .enumerate()
            .map(|(index, dimension)| (index.to_string(), dimension.clone()))
            .collect::<serde_json::Map<_, _>>();
        serde_json::to_vec(&serde_json::json!({
            "regions": { "EU (Ireland)": dimensions }
        }))
        .expect("serialize leaf")
    }

    fn dimension(
        sku: &str,
        instance_type: &str,
        software: &str,
        price: &str,
        unit: &str,
        processor: &str,
        vcpu: u32,
    ) -> serde_json::Value {
        serde_json::json!({
            "rateCode": format!("{sku}.offer.dimension"),
            "price": price,
            "Unit": unit,
            "Instance Type": instance_type,
            "Memory": "64 GiB",
            "vCPU": vcpu.to_string(),
            "Physical Processor": processor,
            "Operating System": "Windows",
            "Pre Installed S/W": software,
            "TermType": "OnDemand",
            "Tenancy": "Shared",
            "Current Generation": "Yes",
            "License Model": "No License required"
        })
    }
}
