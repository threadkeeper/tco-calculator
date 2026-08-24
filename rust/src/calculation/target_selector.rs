use std::cmp::Ordering;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::decimal::DecimalValue;

const NGGP_IOPS_PER_VCORE: u64 = 1_600;
const NGGP_MAX_IOPS: u64 = 80_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CapabilityCatalog {
    pub schema_version: String,
    pub candidates: Vec<TargetCandidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TargetCandidate {
    pub configuration_key: String,
    pub azure_region: String,
    pub service_tier: ServiceTier,
    pub hardware_family: String,
    pub vcores: u32,
    pub zone_redundant: bool,
    pub included_memory_gb: DecimalValue,
    pub supported_memory_gb: Vec<DecimalValue>,
    pub storage_architecture: String,
    pub maximum_storage_gb: Option<DecimalValue>,
    pub source_url: String,
    pub reviewed_date: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    NextGenerationGeneralPurpose,
    BusinessCritical,
}

#[derive(Clone, Copy, Debug)]
pub struct TargetSelectionRequest<'a> {
    pub azure_region: &'a str,
    pub source_vcpu: u32,
    pub source_memory_gb: DecimalValue,
    pub required_storage_gb: DecimalValue,
    pub source_max_iops: u64,
    pub workbook_parity_mode: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingStatus {
    Mapped,
    NoMapping,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TargetSelection {
    pub mapping_status: MappingStatus,
    pub requested_tier: ServiceTier,
    pub nggp_iops_limit: u64,
    pub selected: Option<SelectedTarget>,
    pub candidates: Vec<CandidateEvaluation>,
    pub outcome_reasons: Vec<SelectionReason>,
    pub storage_escalation: Option<StorageEscalation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SelectedTarget {
    pub configuration_key: String,
    pub azure_region: String,
    pub service_tier: ServiceTier,
    pub hardware_family: String,
    pub vcores: u32,
    pub zone_redundant: bool,
    pub included_memory_gb: DecimalValue,
    pub selected_memory_gb: DecimalValue,
    pub additional_memory_gb: DecimalValue,
    pub storage_architecture: String,
    pub maximum_storage_gb: Option<DecimalValue>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CandidateEvaluation {
    pub configuration_key: String,
    pub vcores: u32,
    pub selected_memory_gb: Option<DecimalValue>,
    pub eligible: bool,
    pub rejection_reasons: Vec<SelectionReason>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SelectionReason {
    pub code: SelectionReasonCode,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionReasonCode {
    RegionUnavailable,
    TierUnavailable,
    ZoneRedundancyUnsupported,
    HardwareFamilyUnsupported,
    InsufficientVcores,
    InsufficientMemory,
    InsufficientStorage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageEscalation {
    pub rejected_configuration_key: String,
    pub rejected_maximum_storage_gb: DecimalValue,
    pub selected_configuration_key: String,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TargetSelectionError {
    #[error("source vCPU must be greater than zero")]
    InvalidSourceVcpu,
    #[error("source memory must be greater than zero")]
    InvalidSourceMemory,
    #[error("required storage size must not be negative")]
    InvalidRequiredStorage,
}

struct EligibleCandidate<'a> {
    candidate: &'a TargetCandidate,
    selected_memory_gb: DecimalValue,
}

pub fn select_target(
    catalog: &CapabilityCatalog,
    request: TargetSelectionRequest<'_>,
) -> Result<TargetSelection, TargetSelectionError> {
    validate_request(request)?;

    let nggp_without_storage = eligible_candidates(
        catalog,
        request,
        ServiceTier::NextGenerationGeneralPurpose,
        false,
    );
    let threshold_vcores = nggp_without_storage
        .first()
        .map_or(request.source_vcpu, |candidate| candidate.candidate.vcores);
    let nggp_iops_limit = u64::from(threshold_vcores)
        .saturating_mul(NGGP_IOPS_PER_VCORE)
        .min(NGGP_MAX_IOPS);
    let requested_tier = if request.source_max_iops <= nggp_iops_limit {
        ServiceTier::NextGenerationGeneralPurpose
    } else {
        ServiceTier::BusinessCritical
    };

    let without_storage = eligible_candidates(catalog, request, requested_tier, false);
    let with_storage = eligible_candidates(catalog, request, requested_tier, true);
    let capacity_fallback = if with_storage.is_empty() {
        closest_capacity_candidate(catalog, request, requested_tier, true)
    } else {
        None
    };
    let selected_candidate = with_storage.first().or(capacity_fallback.as_ref());
    let selected = selected_candidate.map(selected_target);
    let storage_escalation = storage_escalation(&without_storage, &with_storage, request);
    let candidates = candidate_evaluations(catalog, request, requested_tier);
    let outcome_reasons = capacity_fallback.as_ref().map_or_else(
        || outcome_reasons(catalog, request, requested_tier, &with_storage),
        |fallback| capacity_fallback_reasons(request, fallback),
    );

    Ok(TargetSelection {
        mapping_status: if selected.is_some() {
            MappingStatus::Mapped
        } else {
            MappingStatus::NoMapping
        },
        requested_tier,
        nggp_iops_limit,
        selected,
        candidates,
        outcome_reasons,
        storage_escalation,
    })
}

fn validate_request(request: TargetSelectionRequest<'_>) -> Result<(), TargetSelectionError> {
    if request.source_vcpu == 0 {
        return Err(TargetSelectionError::InvalidSourceVcpu);
    }
    if request.source_memory_gb.0 <= Decimal::ZERO {
        return Err(TargetSelectionError::InvalidSourceMemory);
    }
    if request.required_storage_gb.0 < Decimal::ZERO {
        return Err(TargetSelectionError::InvalidRequiredStorage);
    }
    Ok(())
}

fn eligible_candidates<'a>(
    catalog: &'a CapabilityCatalog,
    request: TargetSelectionRequest<'_>,
    tier: ServiceTier,
    enforce_storage: bool,
) -> Vec<EligibleCandidate<'a>> {
    let mut candidates = catalog
        .candidates
        .iter()
        .filter_map(|candidate| {
            let (selected_memory_gb, rejection_reasons) =
                evaluate_candidate(candidate, request, tier, enforce_storage);
            if rejection_reasons.is_empty() {
                selected_memory_gb.map(|selected_memory_gb| EligibleCandidate {
                    candidate,
                    selected_memory_gb,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| compare_candidates(left, right, request));
    candidates
}

fn closest_capacity_candidate<'a>(
    catalog: &'a CapabilityCatalog,
    request: TargetSelectionRequest<'_>,
    tier: ServiceTier,
    enforce_storage: bool,
) -> Option<EligibleCandidate<'a>> {
    let mut candidates = catalog
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.azure_region == request.azure_region && candidate.service_tier == tier
        })
        .filter_map(|candidate| {
            let (_, rejection_reasons) = evaluate_candidate(candidate, request, tier, false);
            if rejection_reasons.iter().any(|reason| {
                !matches!(
                    reason.code,
                    SelectionReasonCode::InsufficientVcores
                        | SelectionReasonCode::InsufficientMemory
                )
            }) {
                return None;
            }

            let selected_memory_gb = candidate
                .supported_memory_gb
                .iter()
                .copied()
                .filter(|memory| memory.0 >= request.source_memory_gb.0)
                .min()
                .or_else(|| candidate.supported_memory_gb.iter().copied().max())?;
            Some(EligibleCandidate {
                candidate,
                selected_memory_gb,
            })
        })
        .collect::<Vec<_>>();

    let maximum_vcores = candidates
        .iter()
        .map(|candidate| candidate.candidate.vcores)
        .max()?;
    let maximum_memory_gb = candidates
        .iter()
        .map(|candidate| candidate.selected_memory_gb)
        .max()?;
    let cpu_capacity_exceeded = request.source_vcpu > maximum_vcores;
    let memory_capacity_exceeded = request.source_memory_gb.0 > maximum_memory_gb.0;
    if !cpu_capacity_exceeded && !memory_capacity_exceeded {
        return None;
    }

    if enforce_storage {
        candidates.retain(|candidate| {
            candidate
                .candidate
                .maximum_storage_gb
                .is_none_or(|maximum| maximum.0 >= request.required_storage_gb.0)
        });
    }

    candidates.sort_by(|left, right| {
        let cpu_capacity_order = if cpu_capacity_exceeded {
            right.candidate.vcores.cmp(&left.candidate.vcores)
        } else {
            Ordering::Equal
        };
        let memory_capacity_order = if memory_capacity_exceeded {
            right.selected_memory_gb.cmp(&left.selected_memory_gb)
        } else {
            Ordering::Equal
        };

        cpu_capacity_order
            .then(memory_capacity_order)
            .then_with(|| {
                left.candidate
                    .vcores
                    .abs_diff(request.source_vcpu)
                    .cmp(&right.candidate.vcores.abs_diff(request.source_vcpu))
            })
            .then_with(|| {
                decimal_distance(left.selected_memory_gb.0, request.source_memory_gb.0).cmp(
                    &decimal_distance(right.selected_memory_gb.0, request.source_memory_gb.0),
                )
            })
            .then_with(|| {
                left.candidate
                    .configuration_key
                    .cmp(&right.candidate.configuration_key)
            })
    });
    candidates.into_iter().next()
}

fn decimal_distance(left: Decimal, right: Decimal) -> Decimal {
    if left >= right {
        left - right
    } else {
        right - left
    }
}

fn capacity_fallback_reasons(
    request: TargetSelectionRequest<'_>,
    fallback: &EligibleCandidate<'_>,
) -> Vec<SelectionReason> {
    let mut reasons = Vec::new();
    let tier = service_tier_label(fallback.candidate.service_tier);
    if fallback.candidate.vcores < request.source_vcpu {
        reasons.push(reason(
            SelectionReasonCode::InsufficientVcores,
            &format!(
                "Azure SQL Managed Instance does not offer the source requirement of {} vCores in the {tier} tier on {} hardware. The closest available configuration provides {} vCores; this capacity-limited match has been applied and requires workload validation.",
                request.source_vcpu,
                fallback.candidate.hardware_family,
                fallback.candidate.vcores
            ),
        ));
    }
    if fallback.selected_memory_gb.0 < request.source_memory_gb.0 {
        reasons.push(reason(
            SelectionReasonCode::InsufficientMemory,
            &format!(
                "Azure SQL Managed Instance does not offer the source requirement of {} GB memory in the {tier} tier on {} hardware. The closest available configuration provides {} GB; this capacity-limited match has been applied and requires workload validation.",
                request.source_memory_gb,
                fallback.candidate.hardware_family,
                fallback.selected_memory_gb
            ),
        ));
    }
    reasons
}

fn service_tier_label(tier: ServiceTier) -> &'static str {
    match tier {
        ServiceTier::NextGenerationGeneralPurpose => "Next-generation General Purpose",
        ServiceTier::BusinessCritical => "Business Critical",
    }
}

fn evaluate_candidate(
    candidate: &TargetCandidate,
    request: TargetSelectionRequest<'_>,
    tier: ServiceTier,
    enforce_storage: bool,
) -> (Option<DecimalValue>, Vec<SelectionReason>) {
    let mut reasons = Vec::new();

    if candidate.azure_region != request.azure_region || candidate.service_tier != tier {
        return (None, reasons);
    }
    if candidate.zone_redundant {
        reasons.push(reason(
            SelectionReasonCode::ZoneRedundancyUnsupported,
            "Zone-redundant targets are not eligible in v1.",
        ));
    }
    if request.workbook_parity_mode
        && tier == ServiceTier::NextGenerationGeneralPurpose
        && !matches!(
            candidate.hardware_family.as_str(),
            "Premium Series" | "Premium Series Memory Optimized"
        )
    {
        reasons.push(reason(
            SelectionReasonCode::HardwareFamilyUnsupported,
            "Workbook-parity NGGP selection requires Premium-series hardware.",
        ));
    }
    if candidate.vcores < request.source_vcpu {
        reasons.push(reason(
            SelectionReasonCode::InsufficientVcores,
            &format!(
                "Candidate has {} vCores but the source requires {}.",
                candidate.vcores, request.source_vcpu
            ),
        ));
    }

    let selected_memory_gb = candidate
        .supported_memory_gb
        .iter()
        .copied()
        .filter(|memory| memory.0 >= request.source_memory_gb.0)
        .min();
    if selected_memory_gb.is_none() {
        reasons.push(reason(
            SelectionReasonCode::InsufficientMemory,
            &format!(
                "No supported memory value meets the source requirement of {} GB.",
                request.source_memory_gb
            ),
        ));
    }

    if enforce_storage
        && candidate
            .maximum_storage_gb
            .is_some_and(|maximum| maximum.0 < request.required_storage_gb.0)
    {
        reasons.push(reason(
            SelectionReasonCode::InsufficientStorage,
            &format!(
                "Candidate storage limit is below the required {} GB.",
                request.required_storage_gb
            ),
        ));
    }

    (selected_memory_gb, reasons)
}

fn compare_candidates(
    left: &EligibleCandidate<'_>,
    right: &EligibleCandidate<'_>,
    request: TargetSelectionRequest<'_>,
) -> Ordering {
    let source_vcpu = Decimal::from(request.source_vcpu);
    let left_vcpu_overage = (Decimal::from(left.candidate.vcores) - source_vcpu) / source_vcpu;
    let right_vcpu_overage = (Decimal::from(right.candidate.vcores) - source_vcpu) / source_vcpu;
    let left_memory_overage =
        (left.selected_memory_gb.0 - request.source_memory_gb.0) / request.source_memory_gb.0;
    let right_memory_overage =
        (right.selected_memory_gb.0 - request.source_memory_gb.0) / request.source_memory_gb.0;

    left_vcpu_overage
        .cmp(&right_vcpu_overage)
        .then_with(|| left_memory_overage.cmp(&right_memory_overage))
        .then_with(|| {
            tier_priority(left.candidate.service_tier)
                .cmp(&tier_priority(right.candidate.service_tier))
        })
        .then_with(|| left.candidate.vcores.cmp(&right.candidate.vcores))
        .then_with(|| {
            left.candidate
                .configuration_key
                .cmp(&right.candidate.configuration_key)
        })
}

fn tier_priority(tier: ServiceTier) -> u8 {
    match tier {
        ServiceTier::NextGenerationGeneralPurpose => 0,
        ServiceTier::BusinessCritical => 1,
    }
}

fn selected_target(candidate: &EligibleCandidate<'_>) -> SelectedTarget {
    SelectedTarget {
        configuration_key: candidate.candidate.configuration_key.clone(),
        azure_region: candidate.candidate.azure_region.clone(),
        service_tier: candidate.candidate.service_tier,
        hardware_family: candidate.candidate.hardware_family.clone(),
        vcores: candidate.candidate.vcores,
        zone_redundant: candidate.candidate.zone_redundant,
        included_memory_gb: candidate.candidate.included_memory_gb,
        selected_memory_gb: candidate.selected_memory_gb,
        additional_memory_gb: DecimalValue(
            (candidate.selected_memory_gb.0 - candidate.candidate.included_memory_gb.0)
                .max(Decimal::ZERO),
        ),
        storage_architecture: candidate.candidate.storage_architecture.clone(),
        maximum_storage_gb: candidate.candidate.maximum_storage_gb,
    }
}

fn storage_escalation(
    without_storage: &[EligibleCandidate<'_>],
    with_storage: &[EligibleCandidate<'_>],
    request: TargetSelectionRequest<'_>,
) -> Option<StorageEscalation> {
    let preferred = without_storage.first()?;
    let selected = with_storage.first()?;
    if preferred.candidate.configuration_key == selected.candidate.configuration_key {
        return None;
    }
    let maximum = preferred.candidate.maximum_storage_gb?;
    if maximum.0 >= request.required_storage_gb.0 {
        return None;
    }

    Some(StorageEscalation {
        rejected_configuration_key: preferred.candidate.configuration_key.clone(),
        rejected_maximum_storage_gb: maximum,
        selected_configuration_key: selected.candidate.configuration_key.clone(),
    })
}

fn candidate_evaluations(
    catalog: &CapabilityCatalog,
    request: TargetSelectionRequest<'_>,
    tier: ServiceTier,
) -> Vec<CandidateEvaluation> {
    let mut evaluations = catalog
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.azure_region == request.azure_region && candidate.service_tier == tier
        })
        .map(|candidate| {
            let (selected_memory_gb, rejection_reasons) =
                evaluate_candidate(candidate, request, tier, true);
            CandidateEvaluation {
                configuration_key: candidate.configuration_key.clone(),
                vcores: candidate.vcores,
                selected_memory_gb,
                eligible: rejection_reasons.is_empty(),
                rejection_reasons,
            }
        })
        .collect::<Vec<_>>();

    evaluations.sort_by(|left, right| {
        right
            .eligible
            .cmp(&left.eligible)
            .then_with(|| left.vcores.cmp(&right.vcores))
            .then_with(|| left.configuration_key.cmp(&right.configuration_key))
    });
    evaluations
}

fn outcome_reasons(
    catalog: &CapabilityCatalog,
    request: TargetSelectionRequest<'_>,
    tier: ServiceTier,
    eligible: &[EligibleCandidate<'_>],
) -> Vec<SelectionReason> {
    if !eligible.is_empty() {
        return Vec::new();
    }
    if !catalog
        .candidates
        .iter()
        .any(|candidate| candidate.azure_region == request.azure_region)
    {
        return vec![reason(
            SelectionReasonCode::RegionUnavailable,
            &format!(
                "The capability catalog has no candidates in {}.",
                request.azure_region
            ),
        )];
    }
    if !catalog.candidates.iter().any(|candidate| {
        candidate.azure_region == request.azure_region && candidate.service_tier == tier
    }) {
        return vec![reason(
            SelectionReasonCode::TierUnavailable,
            "The requested service tier is not available in the selected region.",
        )];
    }
    Vec::new()
}

fn reason(code: SelectionReasonCode, detail: &str) -> SelectionReason {
    SelectionReason {
        code,
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn threshold_and_one_above_select_expected_tiers() {
        let catalog = catalog(vec![
            candidate(
                "nggp-8",
                ServiceTier::NextGenerationGeneralPurpose,
                8,
                "64",
                &["64"],
                Some("1024"),
            ),
            candidate(
                "bc-8",
                ServiceTier::BusinessCritical,
                8,
                "56",
                &["56", "64"],
                Some("1024"),
            ),
        ]);
        let mut request = request(12_800, "60", "100");

        let at_threshold = select_target(&catalog, request).expect("threshold selection");
        assert_eq!(
            at_threshold.requested_tier,
            ServiceTier::NextGenerationGeneralPurpose
        );
        assert_eq!(
            at_threshold
                .selected
                .expect("NGGP target")
                .configuration_key,
            "nggp-8"
        );

        request.source_max_iops = 12_801;
        let above_threshold = select_target(&catalog, request).expect("above-threshold selection");
        assert_eq!(
            above_threshold.requested_tier,
            ServiceTier::BusinessCritical
        );
        assert_eq!(
            above_threshold
                .selected
                .expect("BC target")
                .configuration_key,
            "bc-8"
        );
    }

    #[test]
    fn lack_of_nggp_memory_uses_nggp_capacity_fallback_instead_of_business_critical() {
        let catalog = catalog(vec![
            candidate(
                "nggp-8",
                ServiceTier::NextGenerationGeneralPurpose,
                8,
                "32",
                &["32"],
                Some("1024"),
            ),
            candidate(
                "bc-8",
                ServiceTier::BusinessCritical,
                8,
                "128",
                &["128"],
                Some("1024"),
            ),
        ]);

        let selection = select_target(&catalog, request(0, "96", "100")).expect("selection");

        assert_eq!(
            selection.requested_tier,
            ServiceTier::NextGenerationGeneralPurpose
        );
        assert_eq!(selection.mapping_status, MappingStatus::Mapped);
        assert_eq!(
            selection
                .selected
                .as_ref()
                .expect("NGGP fallback")
                .configuration_key,
            "nggp-8"
        );
        assert_eq!(selection.outcome_reasons.len(), 1);
        assert_eq!(
            selection.outcome_reasons[0].code,
            SelectionReasonCode::InsufficientMemory
        );
        assert!(
            selection.outcome_reasons[0]
                .detail
                .contains("closest available configuration provides 32 GB")
        );
    }

    #[test]
    fn very_large_aws_shape_uses_maximum_available_cores_and_memory() {
        let catalog = catalog(vec![
            candidate(
                "nggp-premium-128",
                ServiceTier::NextGenerationGeneralPurpose,
                128,
                "560",
                &["560"],
                Some("32768"),
            ),
            memory_optimized_candidate(
                "nggp-memory-optimized-128",
                ServiceTier::NextGenerationGeneralPurpose,
                128,
                "870.4",
                Some("32768"),
            ),
        ]);
        let mut selection_request = request(0, "1536", "100");
        selection_request.source_vcpu = 192;

        let selection = select_target(&catalog, selection_request).expect("selection");
        let selected = selection.selected.as_ref().expect("capacity fallback");

        assert_eq!(selection.mapping_status, MappingStatus::Mapped);
        assert_eq!(selected.configuration_key, "nggp-memory-optimized-128");
        assert_eq!(selected.vcores, 128);
        assert_eq!(selected.selected_memory_gb, decimal("870.4"));
        assert_eq!(selection.outcome_reasons.len(), 2);
        assert!(selection.outcome_reasons.iter().all(|reason| {
            reason
                .detail
                .contains("capacity-limited match has been applied")
        }));
    }

    #[test]
    fn storage_selects_the_next_valid_candidate_in_the_same_tier() {
        let catalog = catalog(vec![
            candidate(
                "nggp-8",
                ServiceTier::NextGenerationGeneralPurpose,
                8,
                "64",
                &["64"],
                Some("512"),
            ),
            candidate(
                "nggp-16",
                ServiceTier::NextGenerationGeneralPurpose,
                16,
                "128",
                &["128"],
                Some("4096"),
            ),
        ]);

        let selection = select_target(&catalog, request(0, "60", "800")).expect("selection");

        assert_eq!(
            selection
                .selected
                .as_ref()
                .expect("target")
                .configuration_key,
            "nggp-16"
        );
        assert_eq!(
            selection.storage_escalation,
            Some(StorageEscalation {
                rejected_configuration_key: "nggp-8".to_owned(),
                rejected_maximum_storage_gb: decimal("512"),
                selected_configuration_key: "nggp-16".to_owned()
            })
        );
    }

    #[test]
    fn storage_alone_never_switches_to_business_critical() {
        let catalog = catalog(vec![
            candidate(
                "nggp-8",
                ServiceTier::NextGenerationGeneralPurpose,
                8,
                "64",
                &["64"],
                Some("512"),
            ),
            candidate(
                "bc-8",
                ServiceTier::BusinessCritical,
                8,
                "64",
                &["64"],
                Some("4096"),
            ),
        ]);

        let selection = select_target(&catalog, request(0, "60", "800")).expect("selection");

        assert_eq!(
            selection.requested_tier,
            ServiceTier::NextGenerationGeneralPurpose
        );
        assert_eq!(selection.mapping_status, MappingStatus::NoMapping);
    }

    #[test]
    fn storage_failure_does_not_fall_back_to_an_undersized_candidate() {
        let catalog = catalog(vec![
            candidate(
                "nggp-4",
                ServiceTier::NextGenerationGeneralPurpose,
                4,
                "32",
                &["32"],
                Some("4096"),
            ),
            candidate(
                "nggp-8",
                ServiceTier::NextGenerationGeneralPurpose,
                8,
                "64",
                &["64"],
                Some("512"),
            ),
        ]);

        let selection = select_target(&catalog, request(0, "60", "800")).expect("selection");

        assert_eq!(selection.mapping_status, MappingStatus::NoMapping);
        assert!(selection.selected.is_none());
    }

    #[test]
    fn smallest_supported_memory_is_selected_and_additional_memory_is_exact() {
        let catalog = catalog(vec![candidate(
            "nggp-32",
            ServiceTier::NextGenerationGeneralPurpose,
            32,
            "224",
            &["320", "224", "256"],
            None,
        )]);
        let mut selection_request = request(0, "225", "100");
        selection_request.source_vcpu = 32;

        let selection = select_target(&catalog, selection_request).expect("selection");
        let selected = selection.selected.expect("target");

        assert_eq!(selected.selected_memory_gb, decimal("256"));
        assert_eq!(selected.additional_memory_gb, decimal("32"));
    }

    #[test]
    fn selected_target_preserves_zone_redundancy() {
        let mut target = candidate(
            "nggp-zr",
            ServiceTier::NextGenerationGeneralPurpose,
            8,
            "64",
            &["64"],
            None,
        );
        target.zone_redundant = true;

        let selected = selected_target(&EligibleCandidate {
            candidate: &target,
            selected_memory_gb: decimal("64"),
        });

        assert!(selected.zone_redundant);
    }

    fn catalog(candidates: Vec<TargetCandidate>) -> CapabilityCatalog {
        CapabilityCatalog {
            schema_version: "test".to_owned(),
            candidates,
        }
    }

    fn request(
        source_max_iops: u64,
        source_memory_gb: &str,
        required_storage_gb: &str,
    ) -> TargetSelectionRequest<'static> {
        TargetSelectionRequest {
            azure_region: "swedencentral",
            source_vcpu: 8,
            source_memory_gb: decimal(source_memory_gb),
            required_storage_gb: decimal(required_storage_gb),
            source_max_iops,
            workbook_parity_mode: true,
        }
    }

    fn candidate(
        configuration_key: &str,
        service_tier: ServiceTier,
        vcores: u32,
        included_memory_gb: &str,
        supported_memory_gb: &[&str],
        maximum_storage_gb: Option<&str>,
    ) -> TargetCandidate {
        TargetCandidate {
            configuration_key: configuration_key.to_owned(),
            azure_region: "swedencentral".to_owned(),
            service_tier,
            hardware_family: "Premium Series".to_owned(),
            vcores,
            zone_redundant: false,
            included_memory_gb: decimal(included_memory_gb),
            supported_memory_gb: supported_memory_gb
                .iter()
                .map(|value| decimal(value))
                .collect(),
            storage_architecture: if service_tier == ServiceTier::BusinessCritical {
                "BC local SSD"
            } else {
                "Remote LRS"
            }
            .to_owned(),
            maximum_storage_gb: maximum_storage_gb.map(decimal),
            source_url:
                "https://learn.microsoft.com/azure/azure-sql/managed-instance/resource-limits"
                    .to_owned(),
            reviewed_date: "2026-07-31".to_owned(),
        }
    }

    fn memory_optimized_candidate(
        configuration_key: &str,
        service_tier: ServiceTier,
        vcores: u32,
        memory_gb: &str,
        maximum_storage_gb: Option<&str>,
    ) -> TargetCandidate {
        let mut candidate = candidate(
            configuration_key,
            service_tier,
            vcores,
            memory_gb,
            &[memory_gb],
            maximum_storage_gb,
        );
        candidate.hardware_family = "Premium Series Memory Optimized".to_owned();
        candidate
    }

    fn decimal(value: &str) -> DecimalValue {
        DecimalValue(Decimal::from_str(value).expect("valid decimal"))
    }
}
