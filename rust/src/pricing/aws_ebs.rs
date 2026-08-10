use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    calculation::cost::{EbsRate, IopsPriceTier},
    domain::{decimal::DecimalValue, resource::EbsVolumeType},
    pricing::snapshot::{AwsEbsRateRecord, RateProvenance},
};

const GP3_INCLUDED_IOPS: u64 = 3_000;
const GP3_INCLUDED_THROUGHPUT_MIBPS: u64 = 125;

#[derive(Clone, Copy)]
pub struct EbsNormalizationContext<'a> {
    pub region_code: &'a str,
}

#[derive(Clone, Copy)]
pub struct EbsLeafPayload<'a> {
    pub source_url: &'a str,
    pub source_version: Option<&'a str>,
    pub effective_at: Option<&'a str>,
    pub body: &'a [u8],
}

#[derive(Debug)]
pub struct EbsNormalization {
    pub records: Vec<AwsEbsRateRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum EbsNormalizationError {
    #[error("EBS selected price leaf JSON is malformed")]
    MalformedJson,
    #[error("EBS selected price leaf has an unsupported offer code")]
    UnsupportedOffer,
    #[error("EBS price value or range is invalid")]
    InvalidValue,
    #[error("EBS selected price leaf returned an unsupported unit")]
    UnsupportedUnit,
    #[error("EBS selected price leaf returned conflicting component rates")]
    ConflictingComponent,
    #[error("EBS io2 IOPS tiers do not match the supported live tier ranges")]
    InvalidIopsTiers,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedLeaf {
    source_offer_code: String,
    dimensions: Vec<RawDimension>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ComponentKind {
    Capacity,
    Iops,
    Throughput,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDimension {
    volume_type: String,
    component: ComponentKind,
    term_type: String,
    rate_code: String,
    unit: String,
    price: serde_json::Value,
    #[serde(default)]
    begin_range: Option<u64>,
    #[serde(default)]
    end_range: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum VolumeType {
    Gp3,
    Io2,
}

impl VolumeType {
    fn as_key(self) -> &'static str {
        match self {
            Self::Gp3 => "gp3",
            Self::Io2 => "io2",
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
struct ComponentRate {
    monthly_rate: Decimal,
    meter_ids: BTreeSet<String>,
    source: SourceIdentity,
}

#[derive(Clone)]
struct RangedRate {
    begin_range: u64,
    end_range: Option<u64>,
    component: ComponentRate,
}

#[derive(Default)]
struct VolumeComponents {
    capacity: Option<ComponentRate>,
    iops: BTreeMap<u64, RangedRate>,
    throughput: Option<RangedRate>,
}

pub fn normalize_ebs_leaves(
    context: EbsNormalizationContext<'_>,
    leaves: &[EbsLeafPayload<'_>],
) -> Result<EbsNormalization, EbsNormalizationError> {
    if context.region_code.is_empty() {
        return Err(EbsNormalizationError::InvalidValue);
    }

    let mut volumes = BTreeMap::<VolumeType, VolumeComponents>::new();
    for payload in leaves {
        if payload.source_url.is_empty() {
            return Err(EbsNormalizationError::InvalidValue);
        }
        let leaf: SelectedLeaf = serde_json::from_slice(payload.body)
            .map_err(|_| EbsNormalizationError::MalformedJson)?;
        if leaf.source_offer_code != "AmazonEC2" {
            return Err(EbsNormalizationError::UnsupportedOffer);
        }
        let source = SourceIdentity {
            source_url: payload.source_url.to_owned(),
            source_version: payload.source_version.map(str::to_owned),
            effective_at: payload.effective_at.map(str::to_owned),
        };
        for dimension in leaf.dimensions {
            if dimension.term_type != "OnDemand" {
                return Err(EbsNormalizationError::InvalidValue);
            }
            let volume_type = parse_volume_type(&dimension.volume_type)?;
            let components = volumes.entry(volume_type).or_default();
            add_dimension(components, volume_type, source.clone(), dimension)?;
        }
    }

    let mut warnings = Vec::new();
    let mut records = Vec::new();
    for volume_type in [VolumeType::Gp3, VolumeType::Io2] {
        let Some(components) = volumes.remove(&volume_type) else {
            warnings.push(format!(
                "EBS {} price dimensions are unavailable.",
                volume_type.as_key()
            ));
            continue;
        };
        let record = match volume_type {
            VolumeType::Gp3 => build_gp3_record(context.region_code, components),
            VolumeType::Io2 => build_io2_record(context.region_code, components),
        }?;
        match record {
            Some(record) => records.push(record),
            None => warnings.push(format!(
                "EBS {} price dimensions are incomplete; no rate was normalized.",
                volume_type.as_key()
            )),
        }
    }

    warnings.sort();
    records.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
    Ok(EbsNormalization { records, warnings })
}

fn add_dimension(
    components: &mut VolumeComponents,
    volume_type: VolumeType,
    source: SourceIdentity,
    dimension: RawDimension,
) -> Result<(), EbsNormalizationError> {
    let monthly_rate = parse_nonnegative_decimal(&dimension.price)?;
    let mut meter_ids = BTreeSet::new();
    meter_ids.insert(dimension.rate_code);
    let component = ComponentRate {
        monthly_rate,
        meter_ids,
        source,
    };

    match dimension.component {
        ComponentKind::Capacity => {
            if dimension.unit != "GB-Mo"
                || dimension.begin_range.is_some()
                || dimension.end_range.is_some()
            {
                return Err(EbsNormalizationError::UnsupportedUnit);
            }
            merge_component(&mut components.capacity, component)
        }
        ComponentKind::Iops => {
            if dimension.unit != "IOPS-Mo" {
                return Err(EbsNormalizationError::UnsupportedUnit);
            }
            let begin_range = dimension
                .begin_range
                .ok_or(EbsNormalizationError::InvalidValue)?;
            if volume_type == VolumeType::Gp3
                && (begin_range != GP3_INCLUDED_IOPS || dimension.end_range.is_some())
            {
                return Err(EbsNormalizationError::InvalidValue);
            }
            merge_ranged_component(
                &mut components.iops,
                RangedRate {
                    begin_range,
                    end_range: dimension.end_range,
                    component,
                },
            )
        }
        ComponentKind::Throughput => {
            if dimension.unit != "MiBps-Mo" {
                return Err(EbsNormalizationError::UnsupportedUnit);
            }
            let begin_range = dimension
                .begin_range
                .ok_or(EbsNormalizationError::InvalidValue)?;
            if volume_type == VolumeType::Gp3
                && (begin_range != GP3_INCLUDED_THROUGHPUT_MIBPS || dimension.end_range.is_some())
            {
                return Err(EbsNormalizationError::InvalidValue);
            }
            merge_single_ranged_component(
                &mut components.throughput,
                RangedRate {
                    begin_range,
                    end_range: dimension.end_range,
                    component,
                },
            )
        }
    }
}

fn build_gp3_record(
    region_code: &str,
    mut components: VolumeComponents,
) -> Result<Option<AwsEbsRateRecord>, EbsNormalizationError> {
    let (Some(capacity), Some(iops), Some(throughput)) = (
        components.capacity.take(),
        components.iops.remove(&GP3_INCLUDED_IOPS),
        components.throughput.take(),
    ) else {
        return Ok(None);
    };
    if !components.iops.is_empty()
        || iops.end_range.is_some()
        || throughput.begin_range != GP3_INCLUDED_THROUGHPUT_MIBPS
        || throughput.end_range.is_some()
    {
        return Err(EbsNormalizationError::InvalidValue);
    }
    let source = common_source([
        &capacity.source,
        &iops.component.source,
        &throughput.component.source,
    ])?;
    let meter_ids = merged_meter_ids([
        &capacity.meter_ids,
        &iops.component.meter_ids,
        &throughput.component.meter_ids,
    ]);
    Ok(Some(AwsEbsRateRecord {
        stable_key: format!("{region_code}|gp3"),
        rate: EbsRate {
            volume_type: EbsVolumeType::Gp3,
            capacity_monthly_per_gb: DecimalValue(capacity.monthly_rate),
            included_iops: GP3_INCLUDED_IOPS,
            iops_monthly_per_unit: Some(DecimalValue(iops.component.monthly_rate)),
            iops_tiers: Vec::new(),
            included_throughput_mibps: DecimalValue(Decimal::from(GP3_INCLUDED_THROUGHPUT_MIBPS)),
            throughput_monthly_per_mibps: Some(DecimalValue(throughput.component.monthly_rate)),
        },
        provenance: provenance(source, meter_ids),
    }))
}

fn build_io2_record(
    region_code: &str,
    mut components: VolumeComponents,
) -> Result<Option<AwsEbsRateRecord>, EbsNormalizationError> {
    let Some(capacity) = components.capacity.take() else {
        return Ok(None);
    };
    if components.iops.is_empty() {
        return Ok(None);
    }
    let expected = [(0, Some(32_000)), (32_000, Some(64_000)), (64_000, None)];
    let mut tiers = Vec::new();
    let source = capacity.source.clone();
    let mut meter_ids = capacity.meter_ids.clone();
    for (begin, end) in expected {
        let tier = components
            .iops
            .remove(&begin)
            .ok_or(EbsNormalizationError::InvalidIopsTiers)?;
        if tier.end_range != end {
            return Err(EbsNormalizationError::InvalidIopsTiers);
        }
        if tier.component.source != source {
            return Err(EbsNormalizationError::ConflictingComponent);
        }
        tiers.push(IopsPriceTier {
            up_to_inclusive: end,
            monthly_per_iops: DecimalValue(tier.component.monthly_rate),
        });
        meter_ids.extend(tier.component.meter_ids);
    }
    if !components.iops.is_empty() {
        return Err(EbsNormalizationError::InvalidIopsTiers);
    }

    let (included_throughput_mibps, throughput_monthly_per_mibps) =
        if let Some(throughput) = components.throughput.as_ref() {
            if throughput.end_range.is_some() {
                return Err(EbsNormalizationError::InvalidValue);
            }
            if throughput.component.source != source {
                return Err(EbsNormalizationError::ConflictingComponent);
            }
            meter_ids.extend(throughput.component.meter_ids.iter().cloned());
            (
                DecimalValue(Decimal::from(throughput.begin_range)),
                Some(DecimalValue(throughput.component.monthly_rate)),
            )
        } else {
            (DecimalValue::ZERO, None)
        };

    Ok(Some(AwsEbsRateRecord {
        stable_key: format!("{region_code}|io2"),
        rate: EbsRate {
            volume_type: EbsVolumeType::Io2,
            capacity_monthly_per_gb: DecimalValue(capacity.monthly_rate),
            included_iops: 0,
            iops_monthly_per_unit: None,
            iops_tiers: tiers,
            included_throughput_mibps,
            throughput_monthly_per_mibps,
        },
        provenance: provenance(&source, meter_ids.into_iter().collect()),
    }))
}

fn merge_component(
    slot: &mut Option<ComponentRate>,
    incoming: ComponentRate,
) -> Result<(), EbsNormalizationError> {
    match slot {
        Some(existing)
            if existing.monthly_rate != incoming.monthly_rate
                || existing.source != incoming.source =>
        {
            Err(EbsNormalizationError::ConflictingComponent)
        }
        Some(existing) => {
            existing.meter_ids.extend(incoming.meter_ids);
            Ok(())
        }
        None => {
            *slot = Some(incoming);
            Ok(())
        }
    }
}

fn merge_ranged_component(
    components: &mut BTreeMap<u64, RangedRate>,
    incoming: RangedRate,
) -> Result<(), EbsNormalizationError> {
    match components.get_mut(&incoming.begin_range) {
        Some(existing)
            if existing.end_range != incoming.end_range
                || existing.component.monthly_rate != incoming.component.monthly_rate
                || existing.component.source != incoming.component.source =>
        {
            Err(EbsNormalizationError::ConflictingComponent)
        }
        Some(existing) => {
            existing
                .component
                .meter_ids
                .extend(incoming.component.meter_ids);
            Ok(())
        }
        None => {
            components.insert(incoming.begin_range, incoming);
            Ok(())
        }
    }
}

fn merge_single_ranged_component(
    slot: &mut Option<RangedRate>,
    incoming: RangedRate,
) -> Result<(), EbsNormalizationError> {
    match slot {
        Some(existing)
            if existing.begin_range != incoming.begin_range
                || existing.end_range != incoming.end_range
                || existing.component.monthly_rate != incoming.component.monthly_rate
                || existing.component.source != incoming.component.source =>
        {
            Err(EbsNormalizationError::ConflictingComponent)
        }
        Some(existing) => {
            existing
                .component
                .meter_ids
                .extend(incoming.component.meter_ids);
            Ok(())
        }
        None => {
            *slot = Some(incoming);
            Ok(())
        }
    }
}

fn common_source<'a>(
    sources: impl IntoIterator<Item = &'a SourceIdentity>,
) -> Result<&'a SourceIdentity, EbsNormalizationError> {
    let mut sources = sources.into_iter();
    let first = sources.next().ok_or(EbsNormalizationError::InvalidValue)?;
    if sources.any(|source| source != first) {
        return Err(EbsNormalizationError::ConflictingComponent);
    }
    Ok(first)
}

fn merged_meter_ids<'a>(sets: impl IntoIterator<Item = &'a BTreeSet<String>>) -> Vec<String> {
    sets.into_iter()
        .flat_map(|set| set.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn provenance(source: &SourceIdentity, meter_ids: Vec<String>) -> RateProvenance {
    RateProvenance {
        source_url: source.source_url.clone(),
        effective_at: source.effective_at.clone(),
        source_version: source.source_version.clone(),
        meter_ids,
    }
}

fn parse_volume_type(value: &str) -> Result<VolumeType, EbsNormalizationError> {
    match value {
        "gp3" => Ok(VolumeType::Gp3),
        "io2" => Ok(VolumeType::Io2),
        _ => Err(EbsNormalizationError::InvalidValue),
    }
}

fn parse_nonnegative_decimal(value: &serde_json::Value) -> Result<Decimal, EbsNormalizationError> {
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let value = Decimal::from_str(&raw).map_err(|_| EbsNormalizationError::InvalidValue)?;
    if value < Decimal::ZERO {
        return Err(EbsNormalizationError::InvalidValue);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_complete_gp3_and_io2_component_sets() {
        let body = leaf(&[
            dimension("gp3", "capacity", "GB-Mo", "0.08", None, None),
            dimension("gp3", "iops", "IOPS-Mo", "0.005", Some(3_000), None),
            dimension("gp3", "throughput", "MiBps-Mo", "0.04", Some(125), None),
            dimension("io2", "capacity", "GB-Mo", "0.125", None, None),
            dimension("io2", "iops", "IOPS-Mo", "0.065", Some(0), Some(32_000)),
            dimension(
                "io2",
                "iops",
                "IOPS-Mo",
                "0.046",
                Some(32_000),
                Some(64_000),
            ),
            dimension("io2", "iops", "IOPS-Mo", "0.032", Some(64_000), None),
        ]);

        let normalized = normalize_ebs_leaves(context(), &[payload(&body)])
            .expect("normalize selected EBS leaf");

        assert!(normalized.warnings.is_empty());
        assert_eq!(normalized.records.len(), 2);
        let gp3 = &normalized.records[0];
        assert_eq!(gp3.stable_key, "eu-west-1|gp3");
        assert_eq!(gp3.rate.included_iops, 3_000);
        assert_eq!(gp3.rate.included_throughput_mibps.to_string(), "125");
        let io2 = &normalized.records[1];
        assert_eq!(io2.stable_key, "eu-west-1|io2");
        assert_eq!(io2.rate.iops_tiers.len(), 3);
        assert_eq!(io2.rate.iops_tiers[0].up_to_inclusive, Some(32_000));
        assert_eq!(io2.rate.iops_tiers[1].up_to_inclusive, Some(64_000));
        assert_eq!(io2.rate.iops_tiers[2].up_to_inclusive, None);
    }

    #[test]
    fn rejects_shifted_io2_tier_boundaries() {
        let body = leaf(&[
            dimension("io2", "capacity", "GB-Mo", "0.125", None, None),
            dimension("io2", "iops", "IOPS-Mo", "0.065", Some(0), Some(31_999)),
            dimension(
                "io2",
                "iops",
                "IOPS-Mo",
                "0.046",
                Some(32_000),
                Some(64_000),
            ),
            dimension("io2", "iops", "IOPS-Mo", "0.032", Some(64_000), None),
        ]);

        assert!(matches!(
            normalize_ebs_leaves(context(), &[payload(&body)]),
            Err(EbsNormalizationError::InvalidIopsTiers)
        ));
    }

    #[test]
    fn incomplete_component_sets_remain_unavailable() {
        let body = leaf(&[dimension("gp3", "capacity", "GB-Mo", "0.08", None, None)]);

        let normalized = normalize_ebs_leaves(context(), &[payload(&body)])
            .expect("normalize incomplete selected EBS leaf");

        assert!(normalized.records.is_empty());
        assert_eq!(normalized.warnings.len(), 2);
    }

    fn context() -> EbsNormalizationContext<'static> {
        EbsNormalizationContext {
            region_code: "eu-west-1",
        }
    }

    fn payload(body: &[u8]) -> EbsLeafPayload<'_> {
        EbsLeafPayload {
            source_url: "https://example.invalid/ec2-selected-ebs-leaf",
            source_version: Some("test-v1"),
            effective_at: Some("2026-01-01T00:00:00Z"),
            body,
        }
    }

    fn leaf(dimensions: &[serde_json::Value]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "source_offer_code": "AmazonEC2",
            "dimensions": dimensions
        }))
        .expect("serialize selected EBS leaf")
    }

    fn dimension(
        volume_type: &str,
        component: &str,
        unit: &str,
        price: &str,
        begin_range: Option<u64>,
        end_range: Option<u64>,
    ) -> serde_json::Value {
        serde_json::json!({
            "volume_type": volume_type,
            "component": component,
            "term_type": "OnDemand",
            "rate_code": format!("{volume_type}-{component}-{begin_range:?}"),
            "unit": unit,
            "price": price,
            "begin_range": begin_range,
            "end_range": end_range
        })
    }
}
