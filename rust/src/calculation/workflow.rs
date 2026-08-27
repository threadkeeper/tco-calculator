use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        decimal::DecimalValue,
        project::{EditableProject, ProjectSettings, ValidationIssue},
        resource::{
            EbsVolumeType, Ec2Resource, Ec2VmResource, OnPremResource, ProjectType, PurchaseOption,
            RdsResource, Resource, VmBurstPolicy, VmDiskRole, VmHighFrequencyRequirement,
            VmInstanceStoreUse, VmPurchaseOption,
        },
    },
    pricing::{
        provider::{Provider, ResolutionStatus},
        snapshot::{AwsPriceSnapshot, AzureManagedDiskPriceDimension, AzurePriceSnapshot},
        warnings::relevant_for_resources,
    },
};

use super::{
    cost::{
        AzureCostBreakdown, AzureManagedDiskRateSet, AzureRate, CostError, OnPremExplanation,
        SavingsBreakdown, SourceCostBreakdown, azure_mi_billable_storage_gb,
        azure_mi_configured_storage_gb, calculate_azure, calculate_azure_managed_disk_monthly,
        calculate_azure_vm, calculate_ec2_source, calculate_ec2_vm_source,
        calculate_on_prem_source, calculate_rds_source, calculate_savings, source_max_iops,
    },
    sql_payg::{self, SqlPaygAnalysis, SqlPaygInput},
    target_selector::{
        CapabilityCatalog, MappingStatus, TargetSelection, TargetSelectionError,
        TargetSelectionRequest, select_target,
    },
    vm_target_selector::{
        ManagedDiskCatalog, SelectedManagedDisk, SelectedVmTarget, SourceClass,
        VmCapabilityCatalog, VmDiskPriceDimension, VmDiskPriceKey, VmPriceAvailability,
        VmRecommendationStatus, VmSelectionReason, VmSelectionReasonCode, VmTargetSelection,
        VmTargetSelectionError, VmTargetSelectionRequest, VmVolumeRequirement,
        classify_source_instance, select_vm_target,
    },
};

#[derive(Clone)]
pub struct CalculationEngine {
    capabilities: Arc<CapabilityCatalog>,
    vm_capabilities: Arc<VmCapabilityCatalog>,
    managed_disk_capabilities: Arc<ManagedDiskCatalog>,
    formula_version: String,
}

pub struct CalculationInput<'a> {
    pub settings: &'a ProjectSettings,
    pub resources: &'a [Resource],
    pub aws_snapshot: Option<&'a AwsPriceSnapshot>,
    pub azure_snapshot: Option<&'a AzurePriceSnapshot>,
    pub expected_formula_version: Option<&'a str>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CalculationRevision {
    pub formula_version: String,
    pub aws_snapshot_id: Option<String>,
    pub azure_snapshot_id: Option<String>,
    pub resource_results: Vec<ResourceCalculation>,
    pub portfolio_totals: PortfolioTotals,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_payg_analysis: Option<SqlPaygAnalysis>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResourceCalculation {
    pub resource_id: Uuid,
    #[serde(default)]
    pub storage_inputs: StorageInputs,
    pub mapping_status: Option<MappingStatus>,
    pub aws_pricing_status: PricingStatus,
    pub azure_pricing_status: PricingStatus,
    pub target_selection: Option<TargetSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vm_target_selection: Option<VmTargetSelection>,
    pub source_costs: Option<SourceCostBreakdown>,
    pub azure_costs: Option<AzureCostBreakdown>,
    pub purchase_option_discounts: Option<PurchaseOptionDiscounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vm_purchase_option_pricing: Option<Vec<VmPurchaseOptionPricing>>,
    pub savings: Option<SavingsBreakdown>,
    pub explanation_steps: Vec<ExplanationStep>,
    pub unresolved_components: Vec<UnresolvedComponent>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct StorageInputs {
    pub sql_data_gb_per_instance: DecimalValue,
    pub persistent_ebs_gb_per_instance: DecimalValue,
    pub azure_storage_gb_per_instance: DecimalValue,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PurchaseOptionDiscounts {
    pub payg: DecimalValue,
    pub one_year_reserved: DecimalValue,
    pub three_year_reserved: DecimalValue,
    pub one_year_savings_plan: DecimalValue,
    pub azure_hybrid_benefit: DecimalValue,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VmPurchaseOptionPricing {
    pub purchase_option: VmPurchaseOption,
    pub available: bool,
    pub compute_discount: Option<DecimalValue>,
    pub license_discount: Option<DecimalValue>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingStatus {
    Fresh,
    Cached,
    Stale,
    Unavailable,
    NotRequired,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortfolioTotals {
    pub aws_all_rows_total: Option<DecimalValue>,
    pub aws_mapped_rows_total: DecimalValue,
    pub azure_mapped_rows_total: DecimalValue,
    pub required_portfolio_adjustment: DecimalValue,
    pub selected_parity_adjustment: DecimalValue,
    pub portfolio_after_selected_parity: DecimalValue,
    pub portfolio_difference: DecimalValue,
    pub comparable_resource_count: usize,
    pub no_mapping_resource_count: usize,
    pub price_unavailable_resource_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExplanationStep {
    pub code: String,
    pub message: String,
    pub values: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnresolvedComponent {
    pub provider: Option<Provider>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum CalculationError {
    #[error("capability catalog must contain at least one candidate")]
    EmptyCapabilityCatalog,
    #[error("formula version does not match the server calculation version")]
    FormulaVersionMismatch,
    #[error("calculation input failed validation")]
    Validation(Vec<ValidationIssue>),
    #[error("{0:?} price snapshot does not match the project scope")]
    SnapshotScopeMismatch(Provider),
    #[error("mapped target selection did not include a selected target")]
    InvalidTargetSelection,
    #[error(transparent)]
    TargetSelection(#[from] TargetSelectionError),
    #[error(transparent)]
    VmTargetSelection(#[from] VmTargetSelectionError),
    #[error(transparent)]
    Cost(#[from] CostError),
    #[error(transparent)]
    SqlPayg(#[from] sql_payg::SqlPaygError),
}

struct ResolvedSource {
    source_vcpu: Option<u32>,
    source_max_iops: Option<u64>,
    costs: Option<SourceCostBreakdown>,
    pricing_status: PricingStatus,
    explanation_steps: Vec<ExplanationStep>,
    unresolved_components: Vec<UnresolvedComponent>,
}

impl CalculationEngine {
    pub fn new(
        capabilities: Arc<CapabilityCatalog>,
        formula_version: impl Into<String>,
    ) -> Result<Self, CalculationError> {
        if capabilities.candidates.is_empty() {
            return Err(CalculationError::EmptyCapabilityCatalog);
        }

        Ok(Self {
            capabilities,
            vm_capabilities: Arc::new(VmCapabilityCatalog {
                schema_version: "unconfigured".to_owned(),
                candidates: Vec::new(),
            }),
            managed_disk_capabilities: Arc::new(ManagedDiskCatalog {
                schema_version: "unconfigured".to_owned(),
                offers: Vec::new(),
            }),
            formula_version: formula_version.into(),
        })
    }

    pub fn with_vm_catalogs(
        capabilities: Arc<CapabilityCatalog>,
        vm_capabilities: Arc<VmCapabilityCatalog>,
        managed_disk_capabilities: Arc<ManagedDiskCatalog>,
        formula_version: impl Into<String>,
    ) -> Result<Self, CalculationError> {
        let mut engine = Self::new(capabilities, formula_version)?;
        engine.vm_capabilities = vm_capabilities;
        engine.managed_disk_capabilities = managed_disk_capabilities;
        Ok(engine)
    }

    pub fn calculate(
        &self,
        input: CalculationInput<'_>,
    ) -> Result<CalculationRevision, CalculationError> {
        self.validate_input(&input)?;
        if input.settings.project_type == ProjectType::SqlPayg {
            return self.calculate_sql_payg(&input);
        }
        let mut resource_results = Vec::with_capacity(input.resources.len());
        for resource in input.resources {
            resource_results.push(self.calculate_resource(resource, &input)?);
        }

        let portfolio_totals =
            calculate_portfolio(&resource_results, input.settings.selected_parity_adjustment);
        let mut warnings = input
            .aws_snapshot
            .map(|snapshot| relevant_for_resources(&snapshot.metadata.warnings, input.resources))
            .unwrap_or_default();
        warnings.extend(
            input
                .azure_snapshot
                .map(|snapshot| {
                    relevant_for_resources(&snapshot.metadata.warnings, input.resources)
                })
                .unwrap_or_default(),
        );
        if input
            .aws_snapshot
            .is_some_and(|snapshot| snapshot.metadata.status == ResolutionStatus::Stale)
        {
            warnings.push(
                "AWS pricing snapshot is stale but still within the usable window.".to_owned(),
            );
        }
        if input
            .azure_snapshot
            .is_some_and(|snapshot| snapshot.metadata.status == ResolutionStatus::Stale)
        {
            warnings.push(
                "Azure pricing snapshot is stale but still within the usable window.".to_owned(),
            );
        }
        warnings.sort();
        warnings.dedup();

        Ok(CalculationRevision {
            formula_version: self.formula_version.clone(),
            aws_snapshot_id: input
                .aws_snapshot
                .map(|snapshot| snapshot.metadata.snapshot_id.clone()),
            azure_snapshot_id: input
                .azure_snapshot
                .map(|snapshot| snapshot.metadata.snapshot_id.clone()),
            resource_results,
            portfolio_totals,
            warnings,
            sql_payg_analysis: None,
        })
    }

    fn calculate_sql_payg(
        &self,
        input: &CalculationInput<'_>,
    ) -> Result<CalculationRevision, CalculationError> {
        let settings = input.settings.sql_payg.as_ref().ok_or_else(|| {
            CalculationError::Validation(vec![ValidationIssue {
                pointer: "/settings/sql_payg".to_owned(),
                code: "required",
                message: "SQL Pay As You Go licensing inputs are required.".to_owned(),
            }])
        })?;
        let analysis = sql_payg::calculate(SqlPaygInput {
            enterprise_licensed_cores: settings.enterprise_licensed_cores,
            standard_licensed_cores: settings.standard_licensed_cores,
            software_assurance_annual_usd: settings.software_assurance_annual_usd,
            annual_hours: input.settings.default_annual_hours,
            applied_payg_discount: input.settings.selected_parity_adjustment,
        })?;
        let source_total = analysis.software_assurance_annual_usd;
        let payg_gross_total = analysis.payg_gross_annual_usd;
        let payg_net_total = analysis.payg_net_annual_usd;

        Ok(CalculationRevision {
            formula_version: self.formula_version.clone(),
            aws_snapshot_id: None,
            azure_snapshot_id: None,
            resource_results: Vec::new(),
            portfolio_totals: PortfolioTotals {
                aws_all_rows_total: Some(source_total),
                aws_mapped_rows_total: source_total,
                azure_mapped_rows_total: payg_gross_total,
                required_portfolio_adjustment: analysis.required_payg_discount,
                selected_parity_adjustment: analysis.applied_payg_discount,
                portfolio_after_selected_parity: payg_net_total,
                portfolio_difference: DecimalValue(payg_net_total.0 - source_total.0),
                comparable_resource_count: 1,
                no_mapping_resource_count: 0,
                price_unavailable_resource_count: 0,
            },
            warnings: vec![
                "The estimate compares annual Software Assurance or renewal spend with Azure Arc SQL Server PAYG licensing at the entered utilization and applied discount; perpetual acquisition cost is excluded as a sunk cost.".to_owned(),
                "Agreement, entitlement, true-up, buyout, passive replica, outsourcing, and edition-specific terms must be confirmed against the customer's current contract and Microsoft Product Terms.".to_owned(),
            ],
            sql_payg_analysis: Some(analysis),
        })
    }

    fn validate_input(&self, input: &CalculationInput<'_>) -> Result<(), CalculationError> {
        if input
            .expected_formula_version
            .is_some_and(|expected| expected != self.formula_version)
        {
            return Err(CalculationError::FormulaVersionMismatch);
        }

        let validation_project = EditableProject {
            name: "Calculation".to_owned(),
            description: None,
            settings: input.settings.clone(),
            resources: input.resources.to_vec(),
            aws_price_snapshot_id: input
                .aws_snapshot
                .map(|snapshot| snapshot.metadata.snapshot_id.clone()),
            azure_price_snapshot_id: input
                .azure_snapshot
                .map(|snapshot| snapshot.metadata.snapshot_id.clone()),
        };
        let issues = validation_project.validate();
        if !issues.is_empty() {
            return Err(CalculationError::Validation(issues));
        }

        if let Some(snapshot) = input.aws_snapshot {
            let source_region = input
                .settings
                .aws_region
                .as_deref()
                .ok_or(CalculationError::SnapshotScopeMismatch(Provider::Aws))?;
            if !snapshot.matches_scope(&input.settings.currency, source_region) {
                return Err(CalculationError::SnapshotScopeMismatch(Provider::Aws));
            }
        }
        if let Some(snapshot) = input.azure_snapshot
            && !snapshot.matches_scope(&input.settings.currency, &input.settings.azure_region)
        {
            return Err(CalculationError::SnapshotScopeMismatch(Provider::Azure));
        }

        Ok(())
    }

    fn calculate_resource(
        &self,
        resource: &Resource,
        input: &CalculationInput<'_>,
    ) -> Result<ResourceCalculation, CalculationError> {
        let shared = resource.shared();
        let storage_inputs = storage_inputs(resource);
        if let Resource::Ec2Vm(resource) = resource {
            return self.calculate_ec2_vm_resource(resource, storage_inputs, input);
        }
        let mut source = match resource {
            Resource::Ec2(resource) => resolve_ec2_source(resource, input),
            Resource::Rds(resource) => resolve_rds_source(resource, input),
            Resource::OnPrem(resource) => resolve_on_prem_source(resource, input.settings)?,
            Resource::Ec2Vm(_) => unreachable!("EC2 VM resources use the VM calculation path"),
        };

        let Some(source_vcpu) = source.source_vcpu else {
            source.unresolved_components.push(UnresolvedComponent {
                provider: Some(Provider::Aws),
                code: "source_metadata_unavailable".to_owned(),
                message:
                    "Source sizing metadata is unavailable, so target mapping was not evaluated."
                        .to_owned(),
            });
            return Ok(ResourceCalculation {
                resource_id: shared.id,
                storage_inputs,
                mapping_status: None,
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: PricingStatus::Unavailable,
                target_selection: None,
                vm_target_selection: None,
                source_costs: source.costs,
                azure_costs: None,
                purchase_option_discounts: None,
                vm_purchase_option_pricing: None,
                savings: None,
                explanation_steps: source.explanation_steps,
                unresolved_components: source.unresolved_components,
            });
        };
        let source_max_iops = source.source_max_iops.unwrap_or(0);
        let target_selection = select_target(
            &self.capabilities,
            TargetSelectionRequest {
                azure_region: &input.settings.azure_region,
                source_vcpu,
                source_memory_gb: shared.source_ram_gb_per_instance,
                required_storage_gb: storage_inputs.azure_storage_gb_per_instance,
                source_max_iops,
                workbook_parity_mode: true,
            },
        )?;
        source.explanation_steps.push(source_input_step(
            source_vcpu,
            source_max_iops,
            resource,
            storage_inputs,
        ));
        if let Some(source_costs) = source.costs.as_ref()
            && let Some(step) =
                source_cost_formula_step(resource, source_vcpu, input.settings, source_costs)
        {
            source.explanation_steps.push(step);
        }

        if target_selection.mapping_status == MappingStatus::NoMapping {
            return Ok(ResourceCalculation {
                resource_id: shared.id,
                storage_inputs,
                mapping_status: Some(MappingStatus::NoMapping),
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: PricingStatus::NotRequired,
                target_selection: Some(target_selection),
                vm_target_selection: None,
                source_costs: source.costs,
                azure_costs: None,
                purchase_option_discounts: None,
                vm_purchase_option_pricing: None,
                savings: None,
                explanation_steps: source.explanation_steps,
                unresolved_components: source.unresolved_components,
            });
        }

        let selected = target_selection
            .selected
            .as_ref()
            .ok_or(CalculationError::InvalidTargetSelection)?;
        let (
            azure_costs,
            purchase_option_discounts,
            azure_pricing_status,
            azure_unresolved,
            azure_steps,
        ) = resolve_azure_costs(
            selected,
            resource,
            storage_inputs.azure_storage_gb_per_instance,
            input,
        );
        source.unresolved_components.extend(azure_unresolved);
        source.explanation_steps.extend(azure_steps);
        let savings =
            source
                .costs
                .as_ref()
                .zip(azure_costs.as_ref())
                .map(|(source_costs, azure_costs)| {
                    calculate_savings(
                        source_costs,
                        azure_costs,
                        input.settings.selected_parity_adjustment,
                    )
                });
        if let Some(((source_costs, azure_costs), savings)) = source
            .costs
            .as_ref()
            .zip(azure_costs.as_ref())
            .zip(savings.as_ref())
        {
            source.explanation_steps.extend([
                savings_formula_step(source_costs, azure_costs, savings),
                parity_formula_step(source_costs, azure_costs, savings),
            ]);
        }

        Ok(ResourceCalculation {
            resource_id: shared.id,
            storage_inputs,
            mapping_status: Some(MappingStatus::Mapped),
            aws_pricing_status: source.pricing_status,
            azure_pricing_status,
            target_selection: Some(target_selection),
            vm_target_selection: None,
            source_costs: source.costs,
            azure_costs,
            purchase_option_discounts,
            vm_purchase_option_pricing: None,
            savings,
            explanation_steps: source.explanation_steps,
            unresolved_components: source.unresolved_components,
        })
    }

    fn calculate_ec2_vm_resource(
        &self,
        resource: &Ec2VmResource,
        mut storage_inputs: StorageInputs,
        input: &CalculationInput<'_>,
    ) -> Result<ResourceCalculation, CalculationError> {
        let mut source = resolve_ec2_vm_source(resource, input);
        source.explanation_steps.push(vm_assumptions_step(resource));

        let Some(source_vcpu) = source.source_vcpu else {
            source.unresolved_components.push(UnresolvedComponent {
                provider: Some(Provider::Aws),
                code: "source_metadata_unavailable".to_owned(),
                message:
                    "Source sizing metadata is unavailable, so VM target mapping was not evaluated."
                        .to_owned(),
            });
            return Ok(ResourceCalculation {
                resource_id: resource.shared.id,
                storage_inputs,
                mapping_status: None,
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: PricingStatus::Unavailable,
                target_selection: None,
                vm_target_selection: None,
                source_costs: source.costs,
                azure_costs: None,
                purchase_option_discounts: None,
                vm_purchase_option_pricing: None,
                savings: None,
                explanation_steps: source.explanation_steps,
                unresolved_components: source.unresolved_components,
            });
        };
        let Some(source_class) = classify_source_instance(&resource.instance_type) else {
            source.unresolved_components.push(UnresolvedComponent {
                provider: None,
                code: "source_class_unsupported".to_owned(),
                message: format!(
                    "{} is not in the reviewed EC2 VM family mapping policy.",
                    resource.instance_type
                ),
            });
            return Ok(ResourceCalculation {
                resource_id: resource.shared.id,
                storage_inputs,
                mapping_status: None,
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: PricingStatus::NotRequired,
                target_selection: None,
                vm_target_selection: None,
                source_costs: source.costs,
                azure_costs: None,
                purchase_option_discounts: None,
                vm_purchase_option_pricing: None,
                savings: None,
                explanation_steps: source.explanation_steps,
                unresolved_components: source.unresolved_components,
            });
        };

        let mut effective_source_class = source_class;
        let mut recommendation_status = VmRecommendationStatus::Recommended;
        let mut semantic_reasons = Vec::new();
        if source_class == SourceClass::Burstable {
            match resource.requirements.burst_policy {
                VmBurstPolicy::ConfirmedBurstCompatible | VmBurstPolicy::NotApplicable => {}
                VmBurstPolicy::RequiresSustainedCpu => {
                    effective_source_class = SourceClass::GeneralPurpose;
                }
                VmBurstPolicy::Unknown => {
                    effective_source_class = SourceClass::GeneralPurpose;
                    recommendation_status = VmRecommendationStatus::CapacityFitReviewRequired;
                    semantic_reasons.push(VmSelectionReason {
                        code: VmSelectionReasonCode::BurstPolicyReviewRequired,
                        detail: "Burst suitability is unknown, so a conservative D-series capacity fit was evaluated and requires review."
                            .to_owned(),
                    });
                }
            }
        }
        if source_class == SourceClass::HighFrequencyMemoryOptimized {
            recommendation_status = VmRecommendationStatus::CapacityFitReviewRequired;
            let detail = match resource.requirements.high_frequency_requirement {
                VmHighFrequencyRequirement::Required => {
                    "Per-core performance is required; the selected E-series target is capacity-only and is not a performance-equivalence recommendation."
                }
                VmHighFrequencyRequirement::Unknown => {
                    "Per-core performance requirements are unknown; the selected E-series target is capacity-only and requires review."
                }
                VmHighFrequencyRequirement::CapacityFitAccepted
                | VmHighFrequencyRequirement::NotApplicable => {
                    "The E-series target is a capacity fit only; per-core performance equivalence is not claimed."
                }
            };
            semantic_reasons.push(VmSelectionReason {
                code: VmSelectionReasonCode::HighFrequencyReviewRequired,
                detail: detail.to_owned(),
            });
        }

        let volume_requirements = resource
            .volumes
            .iter()
            .filter(|volume| volume.volume_type != EbsVolumeType::Ephemeral)
            .map(|volume| VmVolumeRequirement {
                volume_id: volume.id,
                label: volume.label.clone(),
                role: volume.role,
                capacity_gb: volume.capacity_gb,
                iops: volume.provisioned_iops.unwrap_or(0),
                throughput_mbps: volume.throughput_mibps.unwrap_or(DecimalValue::ZERO),
            })
            .collect::<Vec<_>>();
        source.explanation_steps.push(vm_source_input_step(
            resource,
            source_vcpu,
            effective_source_class,
            &volume_requirements,
        ));
        if let Some(source_costs) = source.costs.as_ref() {
            source.explanation_steps.push(vm_source_cost_formula_step(
                resource,
                input.settings,
                source_costs,
            ));
        }

        let unconfirmed_volume_role = volume_requirements
            .iter()
            .any(|volume| volume.role == VmDiskRole::Unknown);
        let local_capacity = resource
            .requirements
            .required_local_temp_disk_gb
            .unwrap_or(DecimalValue::ZERO);
        let incomplete_reason = if unconfirmed_volume_role {
            Some(VmSelectionReason {
                code: VmSelectionReasonCode::VolumeRoleUnconfirmed,
                detail: "Every persistent volume must be confirmed as the OS or a data disk before target selection."
                    .to_owned(),
            })
        } else if resource.requirements.instance_store_use == VmInstanceStoreUse::Unknown {
            Some(VmSelectionReason {
                code: VmSelectionReasonCode::InstanceStoreReviewRequired,
                detail: "Source instance-store use is unknown and must be confirmed before target selection."
                    .to_owned(),
            })
        } else if resource.requirements.instance_store_use == VmInstanceStoreUse::Used
            && (local_capacity.0 <= Decimal::ZERO
                || resource
                    .requirements
                    .ephemeral_data_loss_acceptable
                    .is_none())
        {
            Some(VmSelectionReason {
                code: VmSelectionReasonCode::InstanceStoreReviewRequired,
                detail: "Used instance storage requires positive capacity and an explicit ephemeral data-loss decision."
                    .to_owned(),
            })
        } else {
            None
        };
        if let Some(incomplete_reason) = incomplete_reason {
            semantic_reasons.push(incomplete_reason);
            return Ok(ResourceCalculation {
                resource_id: resource.shared.id,
                storage_inputs,
                mapping_status: None,
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: PricingStatus::NotRequired,
                target_selection: None,
                vm_target_selection: Some(VmTargetSelection {
                    mapping_status: MappingStatus::NoMapping,
                    recommendation_status: VmRecommendationStatus::Incomplete,
                    requested_lineage: effective_source_class.target_lineage(),
                    selected: None,
                    candidates: Vec::new(),
                    outcome_reasons: semantic_reasons,
                }),
                source_costs: source.costs,
                azure_costs: None,
                purchase_option_discounts: None,
                vm_purchase_option_pricing: None,
                savings: None,
                explanation_steps: source.explanation_steps,
                unresolved_components: source.unresolved_components,
            });
        }
        if resource.requirements.instance_store_use == VmInstanceStoreUse::Used
            && resource.requirements.ephemeral_data_loss_acceptable == Some(false)
        {
            semantic_reasons.push(VmSelectionReason {
                code: VmSelectionReasonCode::EphemeralDataLossIncompatible,
                detail: "The workload cannot tolerate ephemeral data loss, so an Azure temporary disk cannot satisfy the declared instance-store requirement."
                    .to_owned(),
            });
            return Ok(ResourceCalculation {
                resource_id: resource.shared.id,
                storage_inputs,
                mapping_status: Some(MappingStatus::NoMapping),
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: PricingStatus::NotRequired,
                target_selection: None,
                vm_target_selection: Some(VmTargetSelection {
                    mapping_status: MappingStatus::NoMapping,
                    recommendation_status: VmRecommendationStatus::NoEligibleTarget,
                    requested_lineage: effective_source_class.target_lineage(),
                    selected: None,
                    candidates: Vec::new(),
                    outcome_reasons: semantic_reasons,
                }),
                source_costs: source.costs,
                azure_costs: None,
                purchase_option_discounts: None,
                vm_purchase_option_pricing: None,
                savings: None,
                explanation_steps: source.explanation_steps,
                unresolved_components: source.unresolved_components,
            });
        }

        let price_availability = input.azure_snapshot.map(vm_price_availability);
        let mut target_selection = select_vm_target(
            &self.vm_capabilities,
            &self.managed_disk_capabilities,
            &VmTargetSelectionRequest {
                azure_region: &input.settings.azure_region,
                source_class: effective_source_class,
                minimum_vcpu: source_vcpu,
                minimum_memory_gb: resource.shared.source_ram_gb_per_instance,
                requires_local_temp_disk: resource.requirements.instance_store_use
                    == VmInstanceStoreUse::Used,
                minimum_local_temp_disk_gb: local_capacity,
                volumes: &volume_requirements,
                requested_target_arm_sku: resource.requirements.requested_target_arm_sku.as_deref(),
                price_availability: price_availability.as_ref(),
            },
        )?;
        if target_selection.mapping_status == MappingStatus::Mapped {
            target_selection.recommendation_status = recommendation_status;
        }
        target_selection.outcome_reasons.extend(semantic_reasons);

        if target_selection.mapping_status == MappingStatus::NoMapping {
            let price_blocked = target_selection.candidates.iter().any(|candidate| {
                !candidate.rejection_reasons.is_empty()
                    && candidate.rejection_reasons.iter().all(|reason| {
                        reason.code == VmSelectionReasonCode::PriceUnavailable
                    })
            });
            if price_blocked {
                let message = "No technically eligible reviewed Azure VM has complete PAYG VM and managed-disk prices in the coherent snapshot."
                    .to_owned();
                target_selection.recommendation_status = VmRecommendationStatus::Incomplete;
                target_selection.outcome_reasons.push(VmSelectionReason {
                    code: VmSelectionReasonCode::PriceUnavailable,
                    detail: message.clone(),
                });
                source.unresolved_components.push(UnresolvedComponent {
                    provider: Some(Provider::Azure),
                    code: "azure_vm_price_unavailable".to_owned(),
                    message,
                });
            }
            return Ok(ResourceCalculation {
                resource_id: resource.shared.id,
                storage_inputs,
                mapping_status: Some(MappingStatus::NoMapping),
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: if price_blocked {
                    PricingStatus::Unavailable
                } else {
                    PricingStatus::NotRequired
                },
                target_selection: None,
                vm_target_selection: Some(target_selection),
                source_costs: source.costs,
                azure_costs: None,
                purchase_option_discounts: None,
                vm_purchase_option_pricing: None,
                savings: None,
                explanation_steps: source.explanation_steps,
                unresolved_components: source.unresolved_components,
            });
        }

        let selected = target_selection
            .selected
            .as_ref()
            .ok_or(CalculationError::InvalidTargetSelection)?;
        let vm_purchase_option_pricing = input
            .azure_snapshot
            .map(|snapshot| vm_purchase_option_pricing(snapshot, &selected.arm_sku_name));
        storage_inputs.azure_storage_gb_per_instance =
            DecimalValue(selected.disks.iter().map(|disk| disk.capacity_gb.0).sum());
        let (azure_costs, azure_pricing_status, unresolved, explanation_steps) =
            resolve_azure_vm_costs(selected, resource, input);
        if azure_costs.is_none() {
            target_selection.recommendation_status = VmRecommendationStatus::Incomplete;
            target_selection.outcome_reasons.push(VmSelectionReason {
                code: VmSelectionReasonCode::PriceUnavailable,
                detail: "The selected VM or one of its managed disks lacks a complete coherent target price."
                    .to_owned(),
            });
        }
        source.unresolved_components.extend(unresolved);
        source.explanation_steps.extend(explanation_steps);
        let savings =
            source
                .costs
                .as_ref()
                .zip(azure_costs.as_ref())
                .map(|(source_costs, azure_costs)| {
                    calculate_savings(
                        source_costs,
                        azure_costs,
                        input.settings.selected_parity_adjustment,
                    )
                });
        if let Some(((source_costs, azure_costs), savings)) = source
            .costs
            .as_ref()
            .zip(azure_costs.as_ref())
            .zip(savings.as_ref())
        {
            source.explanation_steps.extend([
                savings_formula_step(source_costs, azure_costs, savings),
                parity_formula_step(source_costs, azure_costs, savings),
            ]);
        }

        Ok(ResourceCalculation {
            resource_id: resource.shared.id,
            storage_inputs,
            mapping_status: Some(MappingStatus::Mapped),
            aws_pricing_status: source.pricing_status,
            azure_pricing_status,
            target_selection: None,
            vm_target_selection: Some(target_selection),
            source_costs: source.costs,
            azure_costs,
            purchase_option_discounts: None,
            vm_purchase_option_pricing,
            savings,
            explanation_steps: source.explanation_steps,
            unresolved_components: source.unresolved_components,
        })
    }
}

fn vm_price_availability(snapshot: &AzurePriceSnapshot) -> VmPriceAvailability {
    VmPriceAvailability {
        vm_hourly_rates: snapshot
            .vm_rates
            .iter()
            .filter(|record| record.purchase_option == VmPurchaseOption::Payg)
            .map(|record| (record.arm_sku_name.to_ascii_lowercase(), record.hourly_rate))
            .collect(),
        managed_disk_dimensions: snapshot
            .managed_disk_rates
            .iter()
            .map(|record| VmDiskPriceKey {
                offer_key: record.offer_key.clone(),
                tier_key: record.tier_key.clone(),
                dimension: match record.dimension {
                    AzureManagedDiskPriceDimension::CapacityTier => {
                        VmDiskPriceDimension::CapacityTier
                    }
                    AzureManagedDiskPriceDimension::CapacityGb => VmDiskPriceDimension::CapacityGb,
                    AzureManagedDiskPriceDimension::AdditionalIops => {
                        VmDiskPriceDimension::AdditionalIops
                    }
                    AzureManagedDiskPriceDimension::AdditionalThroughput => {
                        VmDiskPriceDimension::AdditionalThroughput
                    }
                },
            })
            .collect::<BTreeSet<_>>(),
    }
}

fn vm_purchase_option_pricing(
    snapshot: &AzurePriceSnapshot,
    arm_sku_name: &str,
) -> Vec<VmPurchaseOptionPricing> {
    let payg = snapshot.vm_rate(arm_sku_name, VmPurchaseOption::Payg);
    VmPurchaseOption::ALL
        .into_iter()
        .map(|purchase_option| {
            let rate = snapshot.vm_rate(arm_sku_name, purchase_option);
            let discounts = payg.zip(rate).map(|(payg, rate)| {
                let payg_compute = DecimalValue(payg.hourly_rate.0 - payg.license_hourly.0);
                let option_compute =
                    DecimalValue(rate.hourly_rate.0 - rate.license_hourly.0);
                (
                    rate_discount(payg_compute, option_compute),
                    rate_discount(payg.license_hourly, rate.license_hourly),
                )
            });
            VmPurchaseOptionPricing {
                purchase_option,
                available: rate.is_some(),
                compute_discount: discounts.map(|(compute, _)| compute),
                license_discount: discounts.map(|(_, license)| license),
            }
        })
        .collect()
}

fn storage_inputs(resource: &Resource) -> StorageInputs {
    let sql_data_gb_per_instance = resource
        .sql()
        .map_or(DecimalValue::ZERO, |sql| sql.sql_data_gb_per_instance);
    let persistent_ebs_gb_per_instance = match resource {
        Resource::Ec2(resource) => DecimalValue(
            resource
                .volumes
                .iter()
                .filter(|volume| {
                    volume.volume_type != crate::domain::resource::EbsVolumeType::Ephemeral
                })
                .map(|volume| volume.capacity_gb.0)
                .sum(),
        ),
        Resource::Ec2Vm(resource) => DecimalValue(
            resource
                .volumes
                .iter()
                .filter(|volume| {
                    volume.volume_type != crate::domain::resource::EbsVolumeType::Ephemeral
                })
                .map(|volume| volume.capacity_gb.0)
                .sum(),
        ),
        Resource::Rds(_) | Resource::OnPrem(_) => DecimalValue::ZERO,
    };
    let azure_storage_gb_per_instance = if matches!(resource, Resource::Ec2Vm(_)) {
        persistent_ebs_gb_per_instance
    } else {
        azure_mi_configured_storage_gb(DecimalValue(
            sql_data_gb_per_instance.0 + persistent_ebs_gb_per_instance.0,
        ))
    };
    StorageInputs {
        sql_data_gb_per_instance,
        persistent_ebs_gb_per_instance,
        azure_storage_gb_per_instance,
    }
}

fn resolve_ec2_source(resource: &Ec2Resource, input: &CalculationInput<'_>) -> ResolvedSource {
    let Some(snapshot) = input.aws_snapshot else {
        return unavailable_source(Provider::Aws, "aws_snapshot_unavailable");
    };
    let Some(record) = snapshot.ec2_rate(&resource.instance_type) else {
        return unavailable_source(Provider::Aws, "ec2_rate_unavailable");
    };
    let maximum_iops = match source_max_iops(resource) {
        Ok(value) => value,
        Err(error) => {
            return ResolvedSource {
                source_vcpu: Some(record.rate.source_vcpu),
                source_max_iops: None,
                costs: None,
                pricing_status: PricingStatus::Unavailable,
                explanation_steps: vec![source_provenance_step(
                    &record.provenance.source_url,
                    &snapshot.metadata.retrieved_at,
                )],
                unresolved_components: vec![cost_unresolved(Provider::Aws, error)],
            };
        }
    };
    let ebs_rates = snapshot
        .ebs_rates
        .iter()
        .map(|record| record.rate.clone())
        .collect::<Vec<_>>();
    match calculate_ec2_source(resource, record.rate, &ebs_rates, input.settings) {
        Ok(costs) => ResolvedSource {
            source_vcpu: Some(record.rate.source_vcpu),
            source_max_iops: Some(maximum_iops),
            costs: Some(costs),
            pricing_status: pricing_status(snapshot.metadata.status),
            explanation_steps: vec![source_provenance_step(
                &record.provenance.source_url,
                &snapshot.metadata.retrieved_at,
            )],
            unresolved_components: Vec::new(),
        },
        Err(error) => ResolvedSource {
            source_vcpu: Some(record.rate.source_vcpu),
            source_max_iops: Some(maximum_iops),
            costs: None,
            pricing_status: PricingStatus::Unavailable,
            explanation_steps: vec![source_provenance_step(
                &record.provenance.source_url,
                &snapshot.metadata.retrieved_at,
            )],
            unresolved_components: vec![cost_unresolved(Provider::Aws, error)],
        },
    }
}

fn resolve_rds_source(resource: &RdsResource, input: &CalculationInput<'_>) -> ResolvedSource {
    let Some(snapshot) = input.aws_snapshot else {
        return unavailable_source(Provider::Aws, "aws_snapshot_unavailable");
    };
    let Some(record) = snapshot.rds_rate(
        &resource.instance_type,
        resource.deployment,
        &resource.commercial_term,
        &resource.storage_class,
    ) else {
        return unavailable_source(Provider::Aws, "rds_rate_unavailable");
    };
    match calculate_rds_source(resource, record.rate, input.settings) {
        Ok(costs) => ResolvedSource {
            source_vcpu: Some(record.rate.source_vcpu),
            source_max_iops: Some(resource.source_max_iops),
            costs: Some(costs),
            pricing_status: pricing_status(snapshot.metadata.status),
            explanation_steps: vec![source_provenance_step(
                &record.provenance.source_url,
                &snapshot.metadata.retrieved_at,
            )],
            unresolved_components: Vec::new(),
        },
        Err(error) => ResolvedSource {
            source_vcpu: Some(record.rate.source_vcpu),
            source_max_iops: Some(resource.source_max_iops),
            costs: None,
            pricing_status: PricingStatus::Unavailable,
            explanation_steps: vec![source_provenance_step(
                &record.provenance.source_url,
                &snapshot.metadata.retrieved_at,
            )],
            unresolved_components: vec![cost_unresolved(Provider::Aws, error)],
        },
    }
}

fn resolve_ec2_vm_source(resource: &Ec2VmResource, input: &CalculationInput<'_>) -> ResolvedSource {
    let Some(snapshot) = input.aws_snapshot else {
        return unavailable_source(Provider::Aws, "aws_snapshot_unavailable");
    };
    let Some(record) = snapshot.ec2_rate(&resource.instance_type) else {
        return unavailable_source(Provider::Aws, "ec2_vm_rate_unavailable");
    };
    let ebs_rates = snapshot
        .ebs_rates
        .iter()
        .map(|record| record.rate.clone())
        .collect::<Vec<_>>();
    let source_max_iops = resource
        .volumes
        .iter()
        .filter(|volume| volume.volume_type != EbsVolumeType::Ephemeral)
        .filter_map(|volume| volume.provisioned_iops)
        .fold(0_u64, u64::saturating_add);
    match calculate_ec2_vm_source(resource, record.rate, &ebs_rates, input.settings) {
        Ok(costs) => ResolvedSource {
            source_vcpu: Some(record.rate.source_vcpu),
            source_max_iops: Some(source_max_iops),
            costs: Some(costs),
            pricing_status: pricing_status(snapshot.metadata.status),
            explanation_steps: vec![source_provenance_step(
                &record.provenance.source_url,
                &snapshot.metadata.retrieved_at,
            )],
            unresolved_components: Vec::new(),
        },
        Err(error) => ResolvedSource {
            source_vcpu: Some(record.rate.source_vcpu),
            source_max_iops: Some(source_max_iops),
            costs: None,
            pricing_status: PricingStatus::Unavailable,
            explanation_steps: vec![source_provenance_step(
                &record.provenance.source_url,
                &snapshot.metadata.retrieved_at,
            )],
            unresolved_components: vec![cost_unresolved(Provider::Aws, error)],
        },
    }
}

fn resolve_on_prem_source(
    resource: &OnPremResource,
    settings: &ProjectSettings,
) -> Result<ResolvedSource, CostError> {
    let result = calculate_on_prem_source(resource, settings)?;
    let explanation_steps = vec![
        on_prem_power_step(&result.explanation, resource),
        on_prem_cost_formula_step(resource, settings, &result.costs, &result.explanation),
    ];
    Ok(ResolvedSource {
        source_vcpu: Some(resource.source_vcpu),
        source_max_iops: Some(resource.source_max_iops),
        costs: Some(result.costs),
        pricing_status: PricingStatus::NotRequired,
        explanation_steps,
        unresolved_components: Vec::new(),
    })
}

fn resolve_azure_costs(
    selected: &super::target_selector::SelectedTarget,
    resource: &Resource,
    azure_storage_gb_per_instance: DecimalValue,
    input: &CalculationInput<'_>,
) -> (
    Option<AzureCostBreakdown>,
    Option<PurchaseOptionDiscounts>,
    PricingStatus,
    Vec<UnresolvedComponent>,
    Vec<ExplanationStep>,
) {
    let Some(snapshot) = input.azure_snapshot else {
        return (
            None,
            None,
            PricingStatus::Unavailable,
            vec![UnresolvedComponent {
                provider: Some(Provider::Azure),
                code: "azure_snapshot_unavailable".to_owned(),
                message: "A usable Azure price snapshot is required for target cost.".to_owned(),
            }],
            Vec::new(),
        );
    };
    if !snapshot.has_complete_mi_rate_set(&selected.configuration_key) {
        return (
            None,
            None,
            PricingStatus::Unavailable,
            vec![UnresolvedComponent {
                provider: Some(Provider::Azure),
                code: "azure_price_set_incomplete".to_owned(),
                message: "The selected target does not have all eight purchase options and required component prices."
                    .to_owned(),
            }],
            Vec::new(),
        );
    }
    let Some(sql) = resource.sql() else {
        return (
            None,
            None,
            PricingStatus::Unavailable,
            vec![UnresolvedComponent {
                provider: Some(Provider::Azure),
                code: "azure_rate_unavailable".to_owned(),
                message: "SQL Managed Instance rates do not apply to this workload.".to_owned(),
            }],
            Vec::new(),
        );
    };
    let Some(record) = snapshot.mi_rate(&selected.configuration_key, sql.mi_purchase_option) else {
        return (
            None,
            None,
            PricingStatus::Unavailable,
            vec![UnresolvedComponent {
                provider: Some(Provider::Azure),
                code: "azure_rate_unavailable".to_owned(),
                message: "The selected target does not have a complete purchase-option rate."
                    .to_owned(),
            }],
            Vec::new(),
        );
    };
    match calculate_azure(
        resource.shared().quantity,
        resource.shared().annual_hours_per_instance,
        azure_storage_gb_per_instance,
        selected.included_memory_gb,
        selected.selected_memory_gb,
        record.rate,
        input.settings,
    ) {
        Ok(costs) => {
            let purchase_option_discounts =
                purchase_option_discounts(snapshot, &selected.configuration_key);
            let explanation_steps = vec![
                target_provenance_step(
                    &record.provenance.source_url,
                    &snapshot.metadata.retrieved_at,
                ),
                azure_cost_formula_step(
                    selected,
                    resource,
                    azure_storage_gb_per_instance,
                    record.rate,
                    input.settings,
                    &costs,
                ),
            ];
            (
                Some(costs),
                purchase_option_discounts,
                pricing_status(snapshot.metadata.status),
                Vec::new(),
                explanation_steps,
            )
        }
        Err(error) => (
            None,
            None,
            PricingStatus::Unavailable,
            vec![cost_unresolved(Provider::Azure, error)],
            Vec::new(),
        ),
    }
}

fn resolve_azure_vm_costs(
    selected: &SelectedVmTarget,
    resource: &Ec2VmResource,
    input: &CalculationInput<'_>,
) -> (
    Option<AzureCostBreakdown>,
    PricingStatus,
    Vec<UnresolvedComponent>,
    Vec<ExplanationStep>,
) {
    let Some(snapshot) = input.azure_snapshot else {
        return (
            None,
            PricingStatus::Unavailable,
            vec![UnresolvedComponent {
                provider: Some(Provider::Azure),
                code: "azure_snapshot_unavailable".to_owned(),
                message: "A usable Azure price snapshot is required for VM target cost.".to_owned(),
            }],
            Vec::new(),
        );
    };
    let Some(vm_rate) = snapshot.vm_rate(
        &selected.arm_sku_name,
        resource.vm_purchase_option,
    ) else {
        return (
            None,
            PricingStatus::Unavailable,
            vec![UnresolvedComponent {
                provider: Some(Provider::Azure),
                code: "azure_vm_purchase_option_unavailable".to_owned(),
                message: format!(
                    "The selected Azure VM {} does not have an exact rate for purchase option {}.",
                    selected.arm_sku_name,
                    resource.vm_purchase_option.as_str()
                ),
            }],
            Vec::new(),
        );
    };

    let mut monthly_disks = Decimal::ZERO;
    let mut disk_source_urls = Vec::new();
    for disk in &selected.disks {
        match resolve_managed_disk_monthly(snapshot, disk) {
            Ok((cost, source_urls)) => {
                monthly_disks += cost.0;
                disk_source_urls.extend(source_urls);
            }
            Err(error) => {
                return (
                    None,
                    PricingStatus::Unavailable,
                    vec![cost_unresolved(Provider::Azure, error)],
                    Vec::new(),
                );
            }
        }
    }
    disk_source_urls.sort();
    disk_source_urls.dedup();

    match calculate_azure_vm(
        resource.shared.quantity,
        resource.shared.annual_hours_per_instance,
        vm_rate.hourly_rate,
        vm_rate.license_hourly,
        DecimalValue(monthly_disks),
        input.settings,
    ) {
        Ok(costs) => (
            Some(costs.clone()),
            pricing_status(snapshot.metadata.status),
            Vec::new(),
            vec![
                target_provenance_step(
                    &vm_rate.provenance.source_url,
                    &snapshot.metadata.retrieved_at,
                ),
                vm_target_selection_step(selected, &disk_source_urls),
                vm_azure_cost_formula_step(
                    selected,
                    resource,
                    vm_rate.hourly_rate,
                    vm_rate.license_hourly,
                    DecimalValue(monthly_disks),
                    input.settings,
                    &costs,
                ),
            ],
        ),
        Err(error) => (
            None,
            PricingStatus::Unavailable,
            vec![cost_unresolved(Provider::Azure, error)],
            Vec::new(),
        ),
    }
}

fn resolve_managed_disk_monthly(
    snapshot: &AzurePriceSnapshot,
    disk: &SelectedManagedDisk,
) -> Result<(DecimalValue, Vec<String>), CostError> {
    let (capacity_dimension, tier_key) = if let Some(tier_key) = disk.tier_key.as_deref() {
        (AzureManagedDiskPriceDimension::CapacityTier, Some(tier_key))
    } else {
        (AzureManagedDiskPriceDimension::CapacityGb, None)
    };
    let capacity = snapshot
        .managed_disk_rate(&disk.offer_key, tier_key, capacity_dimension)
        .ok_or(CostError::MissingAzureManagedDiskRate)?;
    let mut source_urls = vec![capacity.provenance.source_url.clone()];
    let (iops_rate, throughput_rate) = if disk.tier_key.is_some() {
        (None, None)
    } else {
        let iops = snapshot
            .managed_disk_rate(
                &disk.offer_key,
                None,
                AzureManagedDiskPriceDimension::AdditionalIops,
            )
            .ok_or(CostError::MissingAzureManagedDiskRate)?;
        let throughput = snapshot
            .managed_disk_rate(
                &disk.offer_key,
                None,
                AzureManagedDiskPriceDimension::AdditionalThroughput,
            )
            .ok_or(CostError::MissingAzureManagedDiskRate)?;
        source_urls.extend([
            iops.provenance.source_url.clone(),
            throughput.provenance.source_url.clone(),
        ]);
        (
            Some(iops.normalized_monthly_rate),
            Some(throughput.normalized_monthly_rate),
        )
    };
    let cost = calculate_azure_managed_disk_monthly(
        disk,
        AzureManagedDiskRateSet {
            capacity_monthly: capacity.normalized_monthly_rate,
            additional_iops_monthly_per_unit: iops_rate,
            additional_throughput_monthly_per_mbps: throughput_rate,
        },
    )?;
    Ok((cost, source_urls))
}

fn purchase_option_discounts(
    snapshot: &AzurePriceSnapshot,
    configuration_key: &str,
) -> Option<PurchaseOptionDiscounts> {
    let rate = |purchase_option| {
        snapshot
            .mi_rate(configuration_key, purchase_option)
            .map(|record| record.rate)
    };
    let payg = rate(PurchaseOption::Payg)?;
    let ahb = rate(PurchaseOption::Ahb)?;
    let one_year = rate(PurchaseOption::OneYear)?;
    let three_year = rate(PurchaseOption::ThreeYear)?;
    let savings_one_year = rate(PurchaseOption::SavingsOneYear)?;

    Some(PurchaseOptionDiscounts {
        payg: DecimalValue::ZERO,
        one_year_reserved: rate_discount(payg.compute_hourly, one_year.compute_hourly),
        three_year_reserved: rate_discount(payg.compute_hourly, three_year.compute_hourly),
        one_year_savings_plan: rate_discount(payg.compute_hourly, savings_one_year.compute_hourly),
        azure_hybrid_benefit: rate_discount(payg.license_hourly, ahb.license_hourly),
    })
}

fn rate_discount(baseline: DecimalValue, applied: DecimalValue) -> DecimalValue {
    if baseline.0 == Decimal::ZERO {
        return DecimalValue::ZERO;
    }
    DecimalValue(
        (Decimal::ONE - applied.0 / baseline.0)
            .max(Decimal::ZERO)
            .min(Decimal::ONE),
    )
}

fn calculate_portfolio(
    results: &[ResourceCalculation],
    selected_adjustment: DecimalValue,
) -> PortfolioTotals {
    let all_source_available = results.iter().all(|result| result.source_costs.is_some());
    let aws_all_rows_total = all_source_available.then(|| {
        DecimalValue(
            results
                .iter()
                .filter_map(|result| result.source_costs.as_ref())
                .map(|costs| costs.total.0)
                .sum(),
        )
    });
    let comparable = results
        .iter()
        .filter_map(|result| {
            result
                .source_costs
                .as_ref()
                .zip(result.azure_costs.as_ref())
        })
        .collect::<Vec<_>>();
    let aws_mapped_rows_total = comparable
        .iter()
        .map(|(source, _)| source.total.0)
        .sum::<Decimal>();
    let azure_mapped_rows_total = comparable
        .iter()
        .map(|(_, azure)| azure.total_before_parity.0)
        .sum::<Decimal>();
    let required_portfolio_adjustment = if azure_mapped_rows_total == Decimal::ZERO {
        Decimal::ZERO
    } else {
        Decimal::ONE - aws_mapped_rows_total / azure_mapped_rows_total
    };
    let portfolio_after_selected_parity =
        azure_mapped_rows_total * (Decimal::ONE - selected_adjustment.0);

    PortfolioTotals {
        aws_all_rows_total,
        aws_mapped_rows_total: DecimalValue(aws_mapped_rows_total),
        azure_mapped_rows_total: DecimalValue(azure_mapped_rows_total),
        required_portfolio_adjustment: DecimalValue(required_portfolio_adjustment),
        selected_parity_adjustment: selected_adjustment,
        portfolio_after_selected_parity: DecimalValue(portfolio_after_selected_parity),
        portfolio_difference: DecimalValue(portfolio_after_selected_parity - aws_mapped_rows_total),
        comparable_resource_count: comparable.len(),
        no_mapping_resource_count: results
            .iter()
            .filter(|result| result.mapping_status == Some(MappingStatus::NoMapping))
            .count(),
        price_unavailable_resource_count: results
            .iter()
            .filter(|result| {
                result.aws_pricing_status == PricingStatus::Unavailable
                    || result.azure_pricing_status == PricingStatus::Unavailable
            })
            .count(),
    }
}

fn unavailable_source(provider: Provider, code: &'static str) -> ResolvedSource {
    ResolvedSource {
        source_vcpu: None,
        source_max_iops: None,
        costs: None,
        pricing_status: PricingStatus::Unavailable,
        explanation_steps: Vec::new(),
        unresolved_components: vec![UnresolvedComponent {
            provider: Some(provider),
            code: code.to_owned(),
            message: "A usable source price record is not available.".to_owned(),
        }],
    }
}

fn pricing_status(status: ResolutionStatus) -> PricingStatus {
    match status {
        ResolutionStatus::Fresh => PricingStatus::Fresh,
        ResolutionStatus::Cached => PricingStatus::Cached,
        ResolutionStatus::Stale => PricingStatus::Stale,
        ResolutionStatus::Unavailable => PricingStatus::Unavailable,
    }
}

fn cost_unresolved(provider: Provider, error: CostError) -> UnresolvedComponent {
    UnresolvedComponent {
        provider: Some(provider),
        code: "incomplete_rate_set".to_owned(),
        message: error.to_string(),
    }
}

fn source_input_step(
    source_vcpu: u32,
    source_max_iops: u64,
    resource: &Resource,
    storage_inputs: StorageInputs,
) -> ExplanationStep {
    let shared = resource.shared();
    ExplanationStep {
        code: "source_inputs".to_owned(),
        message: "Effective source sizing inputs were applied without consolidation.".to_owned(),
        values: BTreeMap::from([
            ("source_vcpu".to_owned(), source_vcpu.to_string()),
            (
                "source_ram_gb".to_owned(),
                shared.source_ram_gb_per_instance.to_string(),
            ),
            (
                "sql_data_gb".to_owned(),
                storage_inputs.sql_data_gb_per_instance.to_string(),
            ),
            (
                "persistent_ebs_gb".to_owned(),
                storage_inputs.persistent_ebs_gb_per_instance.to_string(),
            ),
            (
                "azure_required_storage_gb".to_owned(),
                (storage_inputs.sql_data_gb_per_instance.0
                    + storage_inputs.persistent_ebs_gb_per_instance.0)
                    .to_string(),
            ),
            (
                "azure_configured_storage_gb".to_owned(),
                storage_inputs.azure_storage_gb_per_instance.to_string(),
            ),
            ("source_max_iops".to_owned(), source_max_iops.to_string()),
            ("quantity".to_owned(), shared.quantity.to_string()),
            (
                "annual_hours_per_instance".to_owned(),
                shared.annual_hours_per_instance.to_string(),
            ),
        ]),
    }
}

fn vm_assumptions_step(resource: &Ec2VmResource) -> ExplanationStep {
    let requirements = &resource.requirements;
    ExplanationStep {
        code: "vm_assumptions".to_owned(),
        message: "The approved first-release EC2 VM assumptions were applied and remain explicit."
            .to_owned(),
        values: BTreeMap::from([
            ("instance_type".to_owned(), resource.instance_type.clone()),
            ("source_operating_system".to_owned(), "windows".to_owned()),
            ("source_tenancy".to_owned(), "shared".to_owned()),
            ("source_purchase_option".to_owned(), "on_demand".to_owned()),
            (
                "target_license".to_owned(),
                "windows_license_included".to_owned(),
            ),
            ("azure_hybrid_benefit".to_owned(), "false".to_owned()),
            (
                "burst_policy".to_owned(),
                vm_burst_policy_name(requirements.burst_policy).to_owned(),
            ),
            (
                "source_instance_store".to_owned(),
                vm_instance_store_name(requirements.instance_store_use).to_owned(),
            ),
            (
                "required_local_temp_disk_gb".to_owned(),
                requirements
                    .required_local_temp_disk_gb
                    .map_or_else(String::new, |value| value.to_string()),
            ),
            (
                "ephemeral_data_loss_acceptable".to_owned(),
                requirements
                    .ephemeral_data_loss_acceptable
                    .map_or_else(String::new, |value| value.to_string()),
            ),
            (
                "high_frequency_requirement".to_owned(),
                vm_high_frequency_requirement_name(requirements.high_frequency_requirement)
                    .to_owned(),
            ),
            (
                "requested_target_arm_sku".to_owned(),
                requirements
                    .requested_target_arm_sku
                    .clone()
                    .unwrap_or_default(),
            ),
            (
                "target_topology".to_owned(),
                "one_vm_per_source_vm".to_owned(),
            ),
            (
                "disk_topology".to_owned(),
                "one_managed_disk_per_persistent_ebs_volume".to_owned(),
            ),
        ]),
    }
}

fn vm_burst_policy_name(policy: VmBurstPolicy) -> &'static str {
    match policy {
        VmBurstPolicy::ConfirmedBurstCompatible => "confirmed_burst_compatible",
        VmBurstPolicy::RequiresSustainedCpu => "requires_sustained_cpu",
        VmBurstPolicy::Unknown => "unknown",
        VmBurstPolicy::NotApplicable => "not_applicable",
    }
}

fn vm_instance_store_name(use_state: VmInstanceStoreUse) -> &'static str {
    match use_state {
        VmInstanceStoreUse::Unknown => "unknown",
        VmInstanceStoreUse::NotUsed => "not_used",
        VmInstanceStoreUse::Used => "used",
    }
}

fn vm_high_frequency_requirement_name(requirement: VmHighFrequencyRequirement) -> &'static str {
    match requirement {
        VmHighFrequencyRequirement::Required => "required",
        VmHighFrequencyRequirement::Unknown => "unknown",
        VmHighFrequencyRequirement::CapacityFitAccepted => "capacity_fit_accepted",
        VmHighFrequencyRequirement::NotApplicable => "not_applicable",
    }
}

fn vm_source_input_step(
    resource: &Ec2VmResource,
    source_vcpu: u32,
    source_class: SourceClass,
    volumes: &[VmVolumeRequirement],
) -> ExplanationStep {
    let capacity: Decimal = volumes.iter().map(|volume| volume.capacity_gb.0).sum();
    ExplanationStep {
        code: "vm_source_inputs".to_owned(),
        message: "The source VM shape and persistent volumes were preserved without consolidation."
            .to_owned(),
        values: BTreeMap::from([
            ("source_vcpu".to_owned(), source_vcpu.to_string()),
            (
                "source_ram_gb".to_owned(),
                resource.shared.source_ram_gb_per_instance.to_string(),
            ),
            (
                "source_class".to_owned(),
                format!("{source_class:?}").to_ascii_lowercase(),
            ),
            (
                "persistent_volume_count".to_owned(),
                volumes.len().to_string(),
            ),
            ("persistent_volume_gb".to_owned(), capacity.to_string()),
            ("quantity".to_owned(), resource.shared.quantity.to_string()),
            (
                "annual_hours_per_instance".to_owned(),
                resource.shared.annual_hours_per_instance.to_string(),
            ),
        ]),
    }
}

fn vm_source_cost_formula_step(
    resource: &Ec2VmResource,
    settings: &ProjectSettings,
    costs: &SourceCostBreakdown,
) -> ExplanationStep {
    let quantity = Decimal::from(resource.shared.quantity);
    let hours = resource.shared.annual_hours_per_instance.0;
    ExplanationStep {
        code: "source_cost_formula".to_owned(),
        message: "AWS Windows Shared On-Demand compute and persistent EBS costs were calculated from the resolved snapshot."
            .to_owned(),
        values: BTreeMap::from([
            (
                "compute_formula".to_owned(),
                "compute_gross = quantity * annual_hours_per_instance * windows_shared_ondemand_hourly"
                    .to_owned(),
            ),
            (
                "windows_shared_ondemand_hourly".to_owned(),
                divide_or_zero(costs.compute_gross.0, quantity * hours).to_string(),
            ),
            ("compute_gross".to_owned(), costs.compute_gross.to_string()),
            (
                "source_compute_discount".to_owned(),
                settings.source_compute_discount.to_string(),
            ),
            ("compute_net".to_owned(), costs.compute_net.to_string()),
            (
                "storage_formula".to_owned(),
                "storage_gross = quantity * 12 * sum(ebs_volume_monthly_cost)".to_owned(),
            ),
            ("storage_gross".to_owned(), costs.storage_gross.to_string()),
            (
                "source_storage_discount".to_owned(),
                settings.source_storage_discount.to_string(),
            ),
            ("storage_net".to_owned(), costs.storage_net.to_string()),
            ("source_total".to_owned(), costs.total.to_string()),
        ]),
    }
}

fn vm_target_selection_step(
    selected: &SelectedVmTarget,
    disk_source_urls: &[String],
) -> ExplanationStep {
    let disk_summary = selected
        .disks
        .iter()
        .map(|disk| {
            format!(
                "{}:{}:{}GiB:{}IOPS:{}MBps",
                disk.label,
                disk.tier_key.as_deref().unwrap_or(&disk.offer_key),
                disk.capacity_gb,
                disk.provisioned_iops,
                disk.provisioned_throughput_mbps
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    ExplanationStep {
        code: "vm_target_selection".to_owned(),
        message: "The smallest reviewed current-generation Azure VM and one capability-fit managed disk per source volume were selected."
            .to_owned(),
        values: BTreeMap::from([
            ("arm_sku_name".to_owned(), selected.arm_sku_name.clone()),
            ("lineage".to_owned(), format!("{:?}", selected.lineage).to_ascii_lowercase()),
            ("generation".to_owned(), selected.generation.clone()),
            ("vcpus".to_owned(), selected.vcpus.to_string()),
            ("memory_gb".to_owned(), selected.memory_gb.to_string()),
            ("managed_disks".to_owned(), disk_summary),
            ("disk_source_urls".to_owned(), disk_source_urls.join(" ")),
        ]),
    }
}

fn vm_azure_cost_formula_step(
    selected: &SelectedVmTarget,
    resource: &Ec2VmResource,
    vm_total_hourly_rate: DecimalValue,
    vm_license_hourly_rate: DecimalValue,
    managed_disk_monthly_per_instance: DecimalValue,
    settings: &ProjectSettings,
    costs: &AzureCostBreakdown,
) -> ExplanationStep {
    ExplanationStep {
        code: "azure_vm_cost_formula".to_owned(),
        message: "Azure VM base compute, Windows license, and selected managed-disk costs were calculated from one coherent snapshot for the selected purchase option."
            .to_owned(),
        values: BTreeMap::from([
            ("arm_sku_name".to_owned(), selected.arm_sku_name.clone()),
            (
                "compute_formula".to_owned(),
                "compute_gross = quantity * annual_hours_per_instance * (vm_total_hourly - windows_license_hourly)"
                    .to_owned(),
            ),
            (
                "vm_purchase_option".to_owned(),
                resource.vm_purchase_option.as_str().to_owned(),
            ),
            (
                "vm_total_hourly".to_owned(),
                vm_total_hourly_rate.to_string(),
            ),
            (
                "windows_license_hourly".to_owned(),
                vm_license_hourly_rate.to_string(),
            ),
            ("compute_gross".to_owned(), costs.compute_gross.to_string()),
            (
                "azure_compute_discount".to_owned(),
                settings.azure_compute_discount.to_string(),
            ),
            (
                "storage_formula".to_owned(),
                "storage_gross = quantity * 12 * sum(managed_disk_monthly_cost)".to_owned(),
            ),
            (
                "managed_disk_monthly_per_instance".to_owned(),
                managed_disk_monthly_per_instance.to_string(),
            ),
            ("storage_gross".to_owned(), costs.storage_gross.to_string()),
            (
                "azure_storage_discount".to_owned(),
                settings.azure_storage_discount.to_string(),
            ),
            ("storage_net".to_owned(), costs.storage_net.to_string()),
            (
                "license_formula".to_owned(),
                "license_gross = quantity * annual_hours_per_instance * windows_license_hourly"
                    .to_owned(),
            ),
            ("license_gross".to_owned(), costs.license_gross.to_string()),
            ("license_net".to_owned(), costs.license_net.to_string()),
            (
                "azure_license_discount".to_owned(),
                settings.azure_license_discount.to_string(),
            ),
            (
                "quantity".to_owned(),
                resource.shared.quantity.to_string(),
            ),
            (
                "annual_hours_per_instance".to_owned(),
                resource.shared.annual_hours_per_instance.to_string(),
            ),
            (
                "azure_total_before_parity".to_owned(),
                costs.total_before_parity.to_string(),
            ),
        ]),
    }
}

fn source_cost_formula_step(
    resource: &Resource,
    source_vcpu: u32,
    settings: &ProjectSettings,
    costs: &SourceCostBreakdown,
) -> Option<ExplanationStep> {
    let shared = resource.shared();
    let quantity = Decimal::from(shared.quantity);
    let hours = shared.annual_hours_per_instance.0;
    let mut values = BTreeMap::from([
        (
            "compute_formula".to_owned(),
            "compute_gross = quantity * annual_hours_per_instance * compute_hourly".to_owned(),
        ),
        ("quantity".to_owned(), shared.quantity.to_string()),
        (
            "annual_hours_per_instance".to_owned(),
            shared.annual_hours_per_instance.to_string(),
        ),
        (
            "compute_hourly".to_owned(),
            divide_or_zero(costs.compute_gross.0, quantity * hours).to_string(),
        ),
        ("compute_gross".to_owned(), costs.compute_gross.to_string()),
        (
            "compute_net_formula".to_owned(),
            "compute_net = compute_gross * (1 - source_compute_discount)".to_owned(),
        ),
        (
            "source_compute_discount".to_owned(),
            settings.source_compute_discount.to_string(),
        ),
        ("compute_net".to_owned(), costs.compute_net.to_string()),
        (
            "license_net_formula".to_owned(),
            "license_net = license_gross * (1 - source_license_discount)".to_owned(),
        ),
        (
            "source_license_discount".to_owned(),
            settings.source_license_discount.to_string(),
        ),
        ("license_gross".to_owned(), costs.license_gross.to_string()),
        ("license_net".to_owned(), costs.license_net.to_string()),
        (
            "storage_net_formula".to_owned(),
            "storage_net = storage_gross * (1 - source_storage_discount)".to_owned(),
        ),
        (
            "source_storage_discount".to_owned(),
            settings.source_storage_discount.to_string(),
        ),
        ("storage_gross".to_owned(), costs.storage_gross.to_string()),
        ("storage_net".to_owned(), costs.storage_net.to_string()),
        (
            "source_total_formula".to_owned(),
            "source_total = compute_net + license_net + storage_net".to_owned(),
        ),
        ("source_total".to_owned(), costs.total.to_string()),
    ]);

    match resource {
        Resource::Ec2(_) => {
            values.insert(
                "license_formula".to_owned(),
                "license_gross = quantity * annual_hours_per_instance * license_hourly".to_owned(),
            );
            values.insert(
                "license_hourly".to_owned(),
                divide_or_zero(costs.license_gross.0, quantity * hours).to_string(),
            );
            values.insert(
                "storage_formula".to_owned(),
                "storage_gross = quantity * 12 * monthly_storage_per_instance".to_owned(),
            );
            values.insert(
                "monthly_storage_per_instance".to_owned(),
                divide_or_zero(costs.storage_gross.0, quantity * Decimal::from(12)).to_string(),
            );
        }
        Resource::Rds(rds) => {
            values.insert(
                "license_formula".to_owned(),
                "license_gross = quantity * annual_hours_per_instance * source_vcpu * regional_edition_core_hourly"
                    .to_owned(),
            );
            values.insert("source_vcpu".to_owned(), source_vcpu.to_string());
            values.insert(
                "regional_edition_core_hourly".to_owned(),
                divide_or_zero(
                    costs.license_gross.0,
                    quantity * hours * Decimal::from(source_vcpu),
                )
                .to_string(),
            );
            values.insert(
                "storage_formula".to_owned(),
                "storage_gross = quantity * sql_data_gb_per_instance * 12 * storage_monthly_per_gb"
                    .to_owned(),
            );
            values.insert(
                "sql_data_gb_per_instance".to_owned(),
                rds.sql.sql_data_gb_per_instance.to_string(),
            );
            values.insert(
                "storage_monthly_per_gb".to_owned(),
                divide_or_zero(
                    costs.storage_gross.0,
                    quantity * rds.sql.sql_data_gb_per_instance.0 * Decimal::from(12),
                )
                .to_string(),
            );
        }
        Resource::OnPrem(_) | Resource::Ec2Vm(_) => return None,
    }

    Some(ExplanationStep {
        code: "source_cost_formula".to_owned(),
        message: "Source component costs were calculated from the resolved AWS rates and project discounts."
            .to_owned(),
        values,
    })
}

fn azure_cost_formula_step(
    selected: &super::target_selector::SelectedTarget,
    resource: &Resource,
    azure_storage_gb_per_instance: DecimalValue,
    rate: AzureRate,
    settings: &ProjectSettings,
    costs: &AzureCostBreakdown,
) -> ExplanationStep {
    let shared = resource.shared();
    let billable_storage_gb = azure_mi_billable_storage_gb(azure_storage_gb_per_instance);
    ExplanationStep {
        code: "azure_cost_formula".to_owned(),
        message: "Azure component costs were calculated from the selected MI shape and resolved purchase-option rates."
            .to_owned(),
        values: BTreeMap::from([
            (
                "compute_formula".to_owned(),
                "compute_gross = quantity * annual_hours_per_instance * mi_compute_hourly"
                    .to_owned(),
            ),
            ("quantity".to_owned(), shared.quantity.to_string()),
            (
                "annual_hours_per_instance".to_owned(),
                shared.annual_hours_per_instance.to_string(),
            ),
            ("mi_compute_hourly".to_owned(), rate.compute_hourly.to_string()),
            ("compute_gross".to_owned(), costs.compute_gross.to_string()),
            (
                "additional_ram_gb_formula".to_owned(),
                "additional_ram_gb = max(0, selected_mi_ram_gb - included_mi_ram_gb)"
                    .to_owned(),
            ),
            (
                "included_mi_ram_gb".to_owned(),
                selected.included_memory_gb.to_string(),
            ),
            (
                "selected_mi_ram_gb".to_owned(),
                selected.selected_memory_gb.to_string(),
            ),
            (
                "additional_ram_gb".to_owned(),
                costs.additional_ram_gb.to_string(),
            ),
            (
                "additional_ram_formula".to_owned(),
                "additional_ram_gross = quantity * annual_hours_per_instance * additional_ram_gb * additional_memory_per_gb_hourly"
                    .to_owned(),
            ),
            (
                "additional_memory_per_gb_hourly".to_owned(),
                rate.additional_memory_per_gb_hourly.to_string(),
            ),
            (
                "additional_ram_gross".to_owned(),
                costs.additional_ram_gross.to_string(),
            ),
            (
                "compute_plus_ram_net_formula".to_owned(),
                "compute_plus_ram_net = (compute_gross + additional_ram_gross) * (1 - azure_compute_discount)"
                    .to_owned(),
            ),
            (
                "azure_compute_discount".to_owned(),
                settings.azure_compute_discount.to_string(),
            ),
            (
                "compute_plus_ram_net".to_owned(),
                costs.compute_plus_ram_net.to_string(),
            ),
            (
                "license_formula".to_owned(),
                "license_gross = quantity * annual_hours_per_instance * mi_license_hourly"
                    .to_owned(),
            ),
            ("mi_license_hourly".to_owned(), rate.license_hourly.to_string()),
            ("license_gross".to_owned(), costs.license_gross.to_string()),
            (
                "license_net_formula".to_owned(),
                "license_net = license_gross * (1 - azure_license_discount)".to_owned(),
            ),
            (
                "azure_license_discount".to_owned(),
                settings.azure_license_discount.to_string(),
            ),
            ("license_net".to_owned(), costs.license_net.to_string()),
            (
                "storage_formula".to_owned(),
                "azure_billable_storage_gb_per_instance = max(azure_configured_storage_gb_per_instance - 32, 0); storage_gross = quantity * azure_billable_storage_gb_per_instance * 12 * mi_storage_monthly_per_gb"
                    .to_owned(),
            ),
            (
                "azure_configured_storage_gb_per_instance".to_owned(),
                azure_storage_gb_per_instance.to_string(),
            ),
            ("azure_included_storage_gb_per_instance".to_owned(), "32".to_owned()),
            (
                "azure_billable_storage_gb_per_instance".to_owned(),
                billable_storage_gb.to_string(),
            ),
            (
                "mi_storage_monthly_per_gb".to_owned(),
                rate.storage_monthly_per_gb.to_string(),
            ),
            ("storage_gross".to_owned(), costs.storage_gross.to_string()),
            (
                "storage_net_formula".to_owned(),
                "storage_net = storage_gross * (1 - azure_storage_discount)".to_owned(),
            ),
            (
                "azure_storage_discount".to_owned(),
                settings.azure_storage_discount.to_string(),
            ),
            ("storage_net".to_owned(), costs.storage_net.to_string()),
            (
                "azure_total_before_parity_formula".to_owned(),
                "azure_total_before_parity = compute_plus_ram_net + license_net + storage_net"
                    .to_owned(),
            ),
            (
                "azure_total_before_parity".to_owned(),
                costs.total_before_parity.to_string(),
            ),
        ]),
    }
}

fn savings_formula_step(
    source: &SourceCostBreakdown,
    azure: &AzureCostBreakdown,
    savings: &SavingsBreakdown,
) -> ExplanationStep {
    ExplanationStep {
        code: "savings_formula".to_owned(),
        message: "Savings before parity are source component costs minus Azure component costs."
            .to_owned(),
        values: BTreeMap::from([
            (
                "compute_savings_formula".to_owned(),
                "compute_savings = source_compute_net - azure_compute_plus_ram_net".to_owned(),
            ),
            (
                "source_compute_net".to_owned(),
                source.compute_net.to_string(),
            ),
            (
                "azure_compute_plus_ram_net".to_owned(),
                azure.compute_plus_ram_net.to_string(),
            ),
            (
                "compute_savings".to_owned(),
                savings.compute_savings.to_string(),
            ),
            (
                "license_savings_formula".to_owned(),
                "license_savings = source_license_net - azure_license_net".to_owned(),
            ),
            (
                "source_license_net".to_owned(),
                source.license_net.to_string(),
            ),
            (
                "azure_license_net".to_owned(),
                azure.license_net.to_string(),
            ),
            (
                "license_savings".to_owned(),
                savings.license_savings.to_string(),
            ),
            (
                "storage_savings_formula".to_owned(),
                "storage_savings = source_storage_net - azure_storage_net".to_owned(),
            ),
            (
                "source_storage_net".to_owned(),
                source.storage_net.to_string(),
            ),
            (
                "azure_storage_net".to_owned(),
                azure.storage_net.to_string(),
            ),
            (
                "storage_savings".to_owned(),
                savings.storage_savings.to_string(),
            ),
            (
                "total_savings_formula".to_owned(),
                "total_savings = source_total - azure_total_before_parity".to_owned(),
            ),
            ("source_total".to_owned(), source.total.to_string()),
            (
                "azure_total_before_parity".to_owned(),
                azure.total_before_parity.to_string(),
            ),
            (
                "total_savings".to_owned(),
                savings.total_savings.to_string(),
            ),
        ]),
    }
}

fn parity_formula_step(
    source: &SourceCostBreakdown,
    azure: &AzureCostBreakdown,
    savings: &SavingsBreakdown,
) -> ExplanationStep {
    ExplanationStep {
        code: "parity_formula".to_owned(),
        message: "Parity applies the selected adjustment to the Azure total and compares it with the source total."
            .to_owned(),
        values: BTreeMap::from([
            (
                "required_adjustment_formula".to_owned(),
                "required_adjustment = if azure_total_before_parity == 0 then 0 else 1 - source_total / azure_total_before_parity"
                    .to_owned(),
            ),
            ("source_total".to_owned(), source.total.to_string()),
            (
                "azure_total_before_parity".to_owned(),
                azure.total_before_parity.to_string(),
            ),
            (
                "required_adjustment".to_owned(),
                savings.required_adjustment.to_string(),
            ),
            (
                "azure_after_selected_parity_formula".to_owned(),
                "azure_after_selected_parity = azure_total_before_parity * (1 - selected_adjustment)"
                    .to_owned(),
            ),
            (
                "selected_adjustment".to_owned(),
                savings.selected_adjustment.to_string(),
            ),
            (
                "azure_after_selected_parity".to_owned(),
                savings.azure_after_selected_parity.to_string(),
            ),
            (
                "difference_formula".to_owned(),
                "difference = azure_after_selected_parity - source_total".to_owned(),
            ),
            ("difference".to_owned(), savings.difference.to_string()),
        ]),
    }
}

fn divide_or_zero(numerator: Decimal, denominator: Decimal) -> Decimal {
    if denominator == Decimal::ZERO {
        Decimal::ZERO
    } else {
        numerator / denominator
    }
}

fn source_provenance_step(source_url: &str, retrieved_at: &str) -> ExplanationStep {
    ExplanationStep {
        code: "source_price_provenance".to_owned(),
        message: "Source rates were resolved from the immutable AWS snapshot.".to_owned(),
        values: BTreeMap::from([
            ("source_url".to_owned(), source_url.to_owned()),
            ("retrieved_at".to_owned(), retrieved_at.to_owned()),
        ]),
    }
}

fn target_provenance_step(source_url: &str, retrieved_at: &str) -> ExplanationStep {
    ExplanationStep {
        code: "target_price_provenance".to_owned(),
        message: "Target rates were resolved from the immutable Azure snapshot.".to_owned(),
        values: BTreeMap::from([
            ("source_url".to_owned(), source_url.to_owned()),
            ("retrieved_at".to_owned(), retrieved_at.to_owned()),
        ]),
    }
}

fn on_prem_power_step(
    explanation: &OnPremExplanation,
    resource: &OnPremResource,
) -> ExplanationStep {
    ExplanationStep {
        code: "on_prem_power_estimate".to_owned(),
        message: "Indicative server power was estimated from fixed coefficients; an explicit override takes precedence."
            .to_owned(),
        values: BTreeMap::from([
            ("fixed_kw".to_owned(), "0.100".to_owned()),
            ("kw_per_vcpu".to_owned(), "0.0125".to_owned()),
            ("kw_per_ram_gb".to_owned(), "0.000375".to_owned()),
            ("kw_per_data_tb".to_owned(), "0.010".to_owned()),
            ("source_vcpu".to_owned(), resource.source_vcpu.to_string()),
            (
                "source_ram_gb".to_owned(),
                resource.shared.source_ram_gb_per_instance.to_string(),
            ),
            (
                "sql_data_gb".to_owned(),
                resource.sql.sql_data_gb_per_instance.to_string(),
            ),
            (
                "estimated_power_kw".to_owned(),
                explanation.estimated_power_kw.to_string(),
            ),
            (
                "effective_power_kw".to_owned(),
                explanation.effective_power_kw.to_string(),
            ),
            (
                "override_applied".to_owned(),
                explanation.power_override_applied.to_string(),
            ),
            ("annual_kwh".to_owned(), explanation.annual_kwh.to_string()),
        ]),
    }
}

fn on_prem_cost_formula_step(
    resource: &OnPremResource,
    settings: &ProjectSettings,
    costs: &SourceCostBreakdown,
    explanation: &OnPremExplanation,
) -> ExplanationStep {
    let license_price = match resource.sql.sql_edition {
        crate::domain::resource::SqlEdition::Standard => {
            settings.standard_license_sa_usd_per_two_core_pack
        }
        crate::domain::resource::SqlEdition::Enterprise => {
            settings.enterprise_license_sa_usd_per_two_core_pack
        }
    };
    ExplanationStep {
        code: "source_cost_formula".to_owned(),
        message: "On-premises component costs were calculated from hardware depreciation, License + SA, and electricity inputs."
            .to_owned(),
        values: BTreeMap::from([
            ("quantity".to_owned(), resource.shared.quantity.to_string()),
            (
                "annual_hours_per_instance".to_owned(),
                resource.shared.annual_hours_per_instance.to_string(),
            ),
            (
                "hardware_formula".to_owned(),
                "hardware_annual = quantity * hardware_capex_usd / depreciation_years"
                    .to_owned(),
            ),
            (
                "hardware_capex_usd".to_owned(),
                resource.hardware_capex_usd.to_string(),
            ),
            (
                "depreciation_years".to_owned(),
                resource.depreciation_years.to_string(),
            ),
            ("hardware_annual".to_owned(), costs.hardware_annual.to_string()),
            ("compute_gross".to_owned(), costs.compute_gross.to_string()),
            ("compute_net".to_owned(), costs.compute_net.to_string()),
            (
                "license_formula".to_owned(),
                "license_gross = quantity * license_pack_count * license_sa_usd_per_two_core_pack * 12 / remaining_coverage_months"
                    .to_owned(),
            ),
            (
                "licensable_cores".to_owned(),
                resource.licensable_cores.to_string(),
            ),
            (
                "license_pack_count".to_owned(),
                explanation.license_pack_count.to_string(),
            ),
            (
                "license_sa_usd_per_two_core_pack".to_owned(),
                license_price.map_or_else(String::new, |value| value.to_string()),
            ),
            (
                "remaining_coverage_months".to_owned(),
                settings
                    .remaining_coverage_months
                    .map_or_else(String::new, |value| value.to_string()),
            ),
            ("license_gross".to_owned(), costs.license_gross.to_string()),
            (
                "license_net_formula".to_owned(),
                "license_net = license_gross * (1 - source_license_discount)".to_owned(),
            ),
            (
                "source_license_discount".to_owned(),
                settings.source_license_discount.to_string(),
            ),
            ("license_net".to_owned(), costs.license_net.to_string()),
            (
                "electricity_formula".to_owned(),
                "electricity_annual = annual_kwh * electricity_rate_usd_per_kwh".to_owned(),
            ),
            ("annual_kwh".to_owned(), explanation.annual_kwh.to_string()),
            (
                "electricity_rate_usd_per_kwh".to_owned(),
                settings
                    .electricity_rate_usd_per_kwh
                    .map_or_else(String::new, |value| value.to_string()),
            ),
            (
                "electricity_annual".to_owned(),
                costs.electricity_annual.to_string(),
            ),
            (
                "source_total_formula".to_owned(),
                "source_total = hardware_annual + license_net + electricity_annual".to_owned(),
            ),
            ("source_total".to_owned(), costs.total.to_string()),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rust_decimal::Decimal;

    use super::*;
    use crate::{
        calculation::{
            cost::{AzureRate, EbsRate, Ec2Rate},
            target_selector::{SelectionReasonCode, ServiceTier, TargetCandidate},
            vm_target_selector::{
                ManagedDiskOffer, ManagedDiskTier, VmLifecycle, VmLineage, VmSizeCandidate,
            },
        },
        domain::{
            project::SqlPaygSettings,
            resource::{
                EbsVolume, EbsVolumeType, Ec2VmRequirements, Ec2VmResource, LicenseBasis,
                ProjectType, PurchaseOption, SharedResource, SqlEdition, SqlWorkload,
                VmBurstPolicy, VmDiskRole, VmHighFrequencyRequirement, VmInstanceStoreUse,
                VmVolume,
            },
        },
        pricing::snapshot::{
            AwsEbsRateRecord, AwsEc2RateRecord, AzureManagedDiskRateRecord, AzureMiRateRecord,
            AzureVmRateRecord, RateProvenance, SnapshotCreationMetadata,
        },
    };

    #[test]
    fn sql_payg_workflow_uses_net_payg_for_portfolio_comparison() {
        let engine = engine(Some("512"));
        let mut settings = settings();
        settings.project_type = ProjectType::SqlPayg;
        settings.aws_region = None;
        settings.azure_region = "global".to_owned();
        settings.default_annual_hours = decimal("1920");
        settings.selected_parity_adjustment = decimal("0.25");
        settings.sql_payg = Some(SqlPaygSettings {
            enterprise_licensed_cores: 8,
            standard_licensed_cores: 16,
            software_assurance_annual_usd: decimal("20000"),
        });

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[],
                aws_snapshot: None,
                azure_snapshot: None,
                expected_formula_version: Some("1.0.0"),
            })
            .expect("calculation");
        let analysis = revision
            .sql_payg_analysis
            .expect("SQL PAYGO analysis should be present");

        assert_eq!(analysis.payg_gross_annual_usd, decimal("8832.000"));
        assert_eq!(analysis.payg_net_annual_usd, decimal("6624.00000"));
        assert_eq!(analysis.annual_savings_usd, decimal("13376.00000"));
        assert_eq!(
            revision.portfolio_totals.azure_mapped_rows_total,
            decimal("8832.000")
        );
        assert_eq!(
            revision.portfolio_totals.portfolio_after_selected_parity,
            decimal("6624.00000")
        );
        assert_eq!(
            revision.portfolio_totals.portfolio_difference,
            decimal("-13376.00000")
        );
    }

    #[test]
    fn no_mapping_retains_source_cost_and_excludes_row_from_parity() {
        let engine = engine(Some("512"));
        let settings = settings();
        let resource = Resource::Ec2(ec2_resource("800"));
        let aws = aws_snapshot();
        let azure = azure_snapshot();

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("calculation");

        let row = &revision.resource_results[0];
        assert_eq!(row.mapping_status, Some(MappingStatus::NoMapping));
        assert!(row.source_costs.is_some());
        assert!(row.azure_costs.is_none());
        assert_eq!(revision.portfolio_totals.no_mapping_resource_count, 1);
        assert_eq!(
            revision.portfolio_totals.aws_mapped_rows_total,
            DecimalValue::ZERO
        );
        assert!(revision.portfolio_totals.aws_all_rows_total.is_some());
    }

    #[test]
    fn ec2_vm_workflow_selects_vm_and_disk_and_calculates_exact_annual_costs() {
        let base = engine(None);
        let engine = CalculationEngine::with_vm_catalogs(
            Arc::clone(&base.capabilities),
            Arc::new(VmCapabilityCatalog {
                schema_version: "test".to_owned(),
                candidates: vec![VmSizeCandidate {
                    arm_sku_name: "Standard_D8s_v5".to_owned(),
                    display_family: "Dsv5".to_owned(),
                    lineage: VmLineage::GeneralPurpose,
                    generation: "v5".to_owned(),
                    generation_rank: 5,
                    lifecycle: VmLifecycle::Current,
                    azure_region: "swedencentral".to_owned(),
                    cpu_architecture: "x64".to_owned(),
                    windows_eligible: true,
                    vcpus: 8,
                    memory_gb: decimal("32"),
                    max_data_disk_count: 16,
                    premium_io: true,
                    uncached_disk_iops: 12_800,
                    uncached_disk_throughput_mbps: 200,
                    local_temp_disk_gb: None,
                    source_url: "https://example.test/vm-capability".to_owned(),
                    documentation_url: "https://example.test/vm-docs".to_owned(),
                    reviewed_date: "2026-08-24".to_owned(),
                }],
            }),
            Arc::new(ManagedDiskCatalog {
                schema_version: "test".to_owned(),
                offers: vec![ManagedDiskOffer {
                    offer_key: "premium-ssd-lrs".to_owned(),
                    display_name: "Premium SSD LRS".to_owned(),
                    redundancy: "LRS".to_owned(),
                    allowed_roles: vec![VmDiskRole::Os, VmDiskRole::Data],
                    performance_is_provisioned_independently: false,
                    azure_regions: vec!["swedencentral".to_owned()],
                    included_iops: None,
                    included_throughput_mbps: None,
                    maximum_iops: 20_000,
                    maximum_throughput_mbps: 900,
                    capacity_increment_gb: None,
                    minimum_capacity_gb: None,
                    maximum_capacity_gb: decimal("32768"),
                    tiers: vec![ManagedDiskTier {
                        tier_key: "P30".to_owned(),
                        capacity_gb: decimal("1024"),
                        provisioned_iops: 5_000,
                        provisioned_throughput_mbps: decimal("200"),
                    }],
                    source_url: "https://example.test/disk-capability".to_owned(),
                    reviewed_date: "2026-08-24".to_owned(),
                }],
            }),
            "1.0.0",
        )
        .expect("VM calculation engine");
        let resource_id = Uuid::new_v4();
        let resource = Resource::Ec2Vm(Ec2VmResource {
            shared: SharedResource {
                id: resource_id,
                workload_name: "Synthetic Windows VM".to_owned(),
                server_name: None,
                quantity: 1,
                source_ram_gb_per_instance: decimal("32"),
                annual_hours_per_instance: decimal("8760"),
            },
            instance_type: "m5.2xlarge".to_owned(),
            vm_purchase_option: VmPurchaseOption::Payg,
            requirements: Default::default(),
            volumes: vec![VmVolume {
                id: Uuid::new_v4(),
                label: "OS".to_owned(),
                aws_volume_id: None,
                volume_type: EbsVolumeType::Gp3,
                role: VmDiskRole::Os,
                capacity_gb: decimal("1024"),
                provisioned_iops: Some(3_000),
                throughput_mibps: Some(decimal("125")),
            }],
        });
        let mut settings = settings();
        settings.project_type = ProjectType::Ec2Vm;
        let aws = vm_aws_snapshot();
        let azure = vm_azure_snapshot();

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("EC2 VM calculation");

        let row = &revision.resource_results[0];
        let selected = row
            .vm_target_selection
            .as_ref()
            .and_then(|selection| selection.selected.as_ref())
            .expect("selected VM target");
        assert_eq!(row.resource_id, resource_id);
        assert_eq!(row.mapping_status, Some(MappingStatus::Mapped));
        assert!(row.target_selection.is_none());
        assert_eq!(selected.arm_sku_name, "Standard_D8s_v5");
        assert_eq!(selected.disks[0].tier_key.as_deref(), Some("P30"));
        assert_eq!(
            row.source_costs.as_ref().expect("source costs").total,
            decimal("9743.04")
        );
        let azure_costs = row.azure_costs.as_ref().expect("Azure costs");
        assert_eq!(azure_costs.total_before_parity, decimal("8482.56"));
        assert_eq!(azure_costs.license_gross, DecimalValue::ZERO);
        assert_eq!(azure_costs.additional_ram_gross, DecimalValue::ZERO);
        assert_eq!(
            row.storage_inputs.azure_storage_gb_per_instance,
            decimal("1024")
        );
        assert_eq!(revision.portfolio_totals.comparable_resource_count, 1);
        assert!(row.explanation_steps.iter().any(|step| {
            step.code == "vm_assumptions"
                && step.values.get("source_instance_store").map(String::as_str) == Some("not_used")
        }));
    }

    #[test]
    fn ec2_vm_workflow_applies_three_year_savings_plan_with_ahb() {
        let engine = reviewed_vm_engine();
        let mut settings = settings();
        settings.project_type = ProjectType::Ec2Vm;
        let mut resource = reviewed_vm_resource(
            "m5.2xlarge",
            "32",
            Ec2VmRequirements::defaults_for("m5.2xlarge"),
        );
        let Resource::Ec2Vm(vm) = &mut resource else {
            unreachable!("VM resource")
        };
        vm.vm_purchase_option = VmPurchaseOption::AhbSavingsThreeYear;
        let aws = vm_aws_snapshot_for("m5.2xlarge", 8, "32");
        let azure = reviewed_vm_azure_snapshot("Standard_D8s_v7");

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("Savings Plan with AHB calculation");

        let row = &revision.resource_results[0];
        let selected = row
            .vm_target_selection
            .as_ref()
            .and_then(|selection| selection.selected.as_ref())
            .expect("selected VM");
        assert_eq!(selected.arm_sku_name, "Standard_D8s_v7");
        assert_eq!(row.azure_pricing_status, PricingStatus::Fresh);
        let costs = row.azure_costs.as_ref().expect("Azure costs");
        assert_eq!(costs.compute_gross, decimal("3504.0"));
        assert_eq!(costs.license_gross, DecimalValue::ZERO);
        assert_eq!(costs.storage_gross, decimal("60"));
        assert_eq!(costs.total_before_parity, decimal("3564.0"));
        let pricing = row
            .vm_purchase_option_pricing
            .as_ref()
            .expect("VM option pricing");
        assert_eq!(pricing.len(), VmPurchaseOption::ALL.len());
        assert!(pricing.iter().all(|option| option.available));
        let selected_pricing = pricing
            .iter()
            .find(|option| option.purchase_option == VmPurchaseOption::AhbSavingsThreeYear)
            .expect("selected option pricing");
        assert!(
            selected_pricing
                .compute_discount
                .is_some_and(|value| value.0 > Decimal::ZERO)
        );
        assert_eq!(selected_pricing.license_discount, Some(decimal("1")));
    }

    #[test]
    fn ec2_vm_workflow_reports_unavailable_exact_reservation_term() {
        let engine = reviewed_vm_engine();
        let mut settings = settings();
        settings.project_type = ProjectType::Ec2Vm;
        let mut resource = reviewed_vm_resource(
            "m5.2xlarge",
            "32",
            Ec2VmRequirements::defaults_for("m5.2xlarge"),
        );
        let Resource::Ec2Vm(vm) = &mut resource else {
            unreachable!("VM resource")
        };
        vm.vm_purchase_option = VmPurchaseOption::ThreeYear;
        let aws = vm_aws_snapshot_for("m5.2xlarge", 8, "32");
        let azure = reviewed_vm_azure_snapshot_without_reservations("Standard_D8s_v7");

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("unavailable Reservation calculation");

        let row = &revision.resource_results[0];
        assert_eq!(row.mapping_status, Some(MappingStatus::Mapped));
        assert_eq!(row.azure_pricing_status, PricingStatus::Unavailable);
        assert!(row.azure_costs.is_none());
        assert_eq!(
            row.vm_target_selection
                .as_ref()
                .map(|selection| selection.recommendation_status),
            Some(VmRecommendationStatus::Incomplete)
        );
        assert!(row.unresolved_components.iter().any(|component| {
            component.code == "azure_vm_purchase_option_unavailable"
                && component.message.contains("three-year")
        }));
        let pricing = row
            .vm_purchase_option_pricing
            .as_ref()
            .expect("VM option pricing");
        assert!(pricing.iter().any(|option| {
            option.purchase_option == VmPurchaseOption::ThreeYear && !option.available
        }));
        assert!(pricing.iter().any(|option| {
            option.purchase_option == VmPurchaseOption::SavingsThreeYear && option.available
        }));
    }

    #[test]
    fn unknown_t3_burst_policy_uses_conservative_d_series_and_requires_review() {
        let engine = reviewed_vm_engine();
        let mut settings = settings();
        settings.project_type = ProjectType::Ec2Vm;
        let mut requirements = Ec2VmRequirements::defaults_for("t3.large");
        requirements.burst_policy = VmBurstPolicy::Unknown;
        let resource = reviewed_vm_resource("t3.large", "8", requirements);
        let aws = vm_aws_snapshot_for("t3.large", 2, "8");
        let azure = reviewed_vm_azure_snapshot("Standard_D2ds_v7");

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("T3 calculation");

        let selection = revision.resource_results[0]
            .vm_target_selection
            .as_ref()
            .expect("VM selection");
        assert_eq!(
            selection.recommendation_status,
            VmRecommendationStatus::CapacityFitReviewRequired
        );
        assert_eq!(
            selection
                .selected
                .as_ref()
                .map(|target| target.arm_sku_name.as_str()),
            Some("Standard_D2ds_v7")
        );
        assert!(
            selection
                .outcome_reasons
                .iter()
                .any(|reason| { reason.code == VmSelectionReasonCode::BurstPolicyReviewRequired })
        );
    }

    #[test]
    fn high_frequency_source_is_never_recommended_from_capacity_alone() {
        let engine = reviewed_vm_engine();
        let mut settings = settings();
        settings.project_type = ProjectType::Ec2Vm;
        let mut requirements = Ec2VmRequirements::defaults_for("z1d.2xlarge");
        requirements.high_frequency_requirement = VmHighFrequencyRequirement::CapacityFitAccepted;
        let resource = reviewed_vm_resource("z1d.2xlarge", "64", requirements);
        let aws = vm_aws_snapshot_for("z1d.2xlarge", 8, "64");
        let azure = reviewed_vm_azure_snapshot("Standard_E8ds_v7");

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("high-frequency calculation");

        let selection = revision.resource_results[0]
            .vm_target_selection
            .as_ref()
            .expect("VM selection");
        assert_eq!(
            selection.recommendation_status,
            VmRecommendationStatus::CapacityFitReviewRequired
        );
        assert!(
            selection.outcome_reasons.iter().any(|reason| {
                reason.code == VmSelectionReasonCode::HighFrequencyReviewRequired
            })
        );
    }

    #[test]
    fn unknown_instance_store_use_is_incomplete_without_hiding_source_cost() {
        let engine = reviewed_vm_engine();
        let mut settings = settings();
        settings.project_type = ProjectType::Ec2Vm;
        let mut requirements = Ec2VmRequirements::defaults_for("r6id.8xlarge");
        requirements.instance_store_use = VmInstanceStoreUse::Unknown;
        let resource = reviewed_vm_resource("r6id.8xlarge", "256", requirements);
        let aws = vm_aws_snapshot_for("r6id.8xlarge", 32, "256");

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: None,
                expected_formula_version: Some("1.0.0"),
            })
            .expect("incomplete instance-store calculation");

        let row = &revision.resource_results[0];
        let selection = row.vm_target_selection.as_ref().expect("VM selection");
        assert_eq!(
            selection.recommendation_status,
            VmRecommendationStatus::Incomplete
        );
        assert!(selection.selected.is_none());
        assert!(row.source_costs.is_some());
        assert!(row.azure_costs.is_none());
        assert!(
            selection.outcome_reasons.iter().any(|reason| {
                reason.code == VmSelectionReasonCode::InstanceStoreReviewRequired
            })
        );
    }

    #[test]
    fn ec2_sql_data_and_persistent_ebs_are_combined_for_target_storage() {
        let engine = engine(Some("512"));
        let settings = settings();
        let mut resource = ec2_resource("1");
        resource.volumes = vec![EbsVolume {
            id: Uuid::new_v4(),
            label: "SQL data".to_owned(),
            aws_volume_id: None,
            volume_type: EbsVolumeType::Gp3,
            capacity_gb: decimal("600"),
            provisioned_iops: Some(3000),
            throughput_mibps: Some(decimal("125")),
        }];
        let aws = aws_snapshot();

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[Resource::Ec2(resource)],
                aws_snapshot: Some(&aws),
                azure_snapshot: None,
                expected_formula_version: Some("1.0.0"),
            })
            .expect("calculation");

        assert_eq!(
            revision.resource_results[0].mapping_status,
            Some(MappingStatus::NoMapping)
        );
        assert_eq!(
            revision.resource_results[0]
                .storage_inputs
                .sql_data_gb_per_instance,
            decimal("1")
        );
        assert_eq!(
            revision.resource_results[0]
                .storage_inputs
                .persistent_ebs_gb_per_instance,
            decimal("600")
        );
        assert_eq!(
            revision.resource_results[0]
                .storage_inputs
                .azure_storage_gb_per_instance,
            decimal("608")
        );
    }

    #[test]
    fn ec2_combined_storage_is_used_for_azure_storage_cost() {
        let engine = engine(Some("1024"));
        let settings = settings();
        let mut resource = ec2_resource("1");
        resource.volumes = vec![EbsVolume {
            id: Uuid::new_v4(),
            label: "SQL data".to_owned(),
            aws_volume_id: None,
            volume_type: EbsVolumeType::Gp3,
            capacity_gb: decimal("600"),
            provisioned_iops: Some(3000),
            throughput_mibps: Some(decimal("125")),
        }];
        let aws = aws_snapshot();
        let azure = azure_snapshot();

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[Resource::Ec2(resource)],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("calculation");

        let row = &revision.resource_results[0];
        assert_eq!(row.mapping_status, Some(MappingStatus::Mapped));
        assert_eq!(
            row.azure_costs.as_ref().expect("Azure costs").storage_gross,
            decimal("691.20")
        );
    }

    #[test]
    fn very_large_ec2_shape_uses_closest_sql_mi_capacity_with_explicit_notes() {
        let engine = capacity_engine();
        let settings = settings();
        let resource = Resource::Ec2(Ec2Resource {
            shared: SharedResource {
                id: Uuid::new_v4(),
                workload_name: "Synthetic large-memory EC2 workload".to_owned(),
                server_name: None,
                quantity: 1,
                source_ram_gb_per_instance: decimal("1536"),
                annual_hours_per_instance: decimal("8760"),
            },
            sql: SqlWorkload {
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::Byol,
                sql_data_gb_per_instance: decimal("4096"),
                mi_purchase_option: PurchaseOption::Ahb,
            },
            instance_type: "r7i.48xlarge".to_owned(),
            volumes: vec![EbsVolume {
                id: Uuid::new_v4(),
                label: "Instance storage".to_owned(),
                aws_volume_id: None,
                volume_type: EbsVolumeType::Ephemeral,
                capacity_gb: DecimalValue::ZERO,
                provisioned_iops: None,
                throughput_mibps: None,
            }],
        });
        let aws = aws_snapshot_for("r7i.48xlarge", 192, "1536");
        let azure = azure_snapshot_for(
            "managed-vcore-next-gen-general-purpose-premium-series-memory-optimized-128",
        );

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("calculation");
        let row = &revision.resource_results[0];
        let selection = row.target_selection.as_ref().expect("target selection");
        let selected = selection.selected.as_ref().expect("capacity fallback");

        assert_eq!(row.mapping_status, Some(MappingStatus::Mapped));
        assert_eq!(selected.vcores, 128);
        assert_eq!(selected.selected_memory_gb, decimal("870.4"));
        assert_eq!(selection.outcome_reasons.len(), 2);
        assert!(selection.outcome_reasons.iter().any(|reason| {
            reason.code == SelectionReasonCode::InsufficientVcores
                && reason.detail.contains("source requirement of 192 vCores")
                && reason
                    .detail
                    .contains("closest available configuration provides 128 vCores")
        }));
        assert!(selection.outcome_reasons.iter().any(|reason| {
            reason.code == SelectionReasonCode::InsufficientMemory
                && reason
                    .detail
                    .contains("source requirement of 1536 GB memory")
                && reason
                    .detail
                    .contains("closest available configuration provides 870.4 GB")
        }));
    }

    #[test]
    fn one_tib_on_prem_source_uses_closest_sql_mi_memory_with_explicit_note() {
        let engine = capacity_engine();
        let mut settings = settings();
        settings.project_type = ProjectType::OnPrem;
        settings.aws_region = None;
        settings.standard_license_sa_usd_per_two_core_pack = Some(decimal("1200"));
        settings.enterprise_license_sa_usd_per_two_core_pack = Some(decimal("4800"));
        settings.remaining_coverage_months = Some(36);
        settings.electricity_rate_usd_per_kwh = Some(decimal("0.20"));
        let resource = Resource::OnPrem(OnPremResource {
            shared: SharedResource {
                id: Uuid::new_v4(),
                workload_name: "Synthetic 1 TiB on-prem workload".to_owned(),
                server_name: None,
                quantity: 1,
                source_ram_gb_per_instance: decimal("1024"),
                annual_hours_per_instance: decimal("8760"),
            },
            sql: SqlWorkload {
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::LicenseIncluded,
                sql_data_gb_per_instance: decimal("4096"),
                mi_purchase_option: PurchaseOption::Ahb,
            },
            source_vcpu: 128,
            licensable_cores: 128,
            source_max_iops: 0,
            hardware_capex_usd: decimal("100000"),
            depreciation_years: decimal("4"),
            average_power_kw_override: Some(decimal("2")),
        });
        let azure = azure_snapshot_for(
            "managed-vcore-next-gen-general-purpose-premium-series-memory-optimized-128",
        );

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: None,
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("calculation");
        let row = &revision.resource_results[0];
        let selection = row.target_selection.as_ref().expect("target selection");
        let selected = selection.selected.as_ref().expect("capacity fallback");

        assert_eq!(row.mapping_status, Some(MappingStatus::Mapped));
        assert_eq!(selected.vcores, 128);
        assert_eq!(selected.selected_memory_gb, decimal("870.4"));
        assert_eq!(selection.outcome_reasons.len(), 1);
        assert_eq!(
            selection.outcome_reasons[0].code,
            SelectionReasonCode::InsufficientMemory
        );
        assert!(
            selection.outcome_reasons[0]
                .detail
                .contains("source requirement of 1024 GB memory")
        );
        assert!(
            selection.outcome_reasons[0]
                .detail
                .contains("capacity-limited match has been applied")
        );
    }

    #[test]
    fn calculation_excludes_snapshot_warnings_for_unsubmitted_resources() {
        let engine = engine(None);
        let settings = settings();
        let resource = Resource::Ec2(ec2_resource("100"));
        let mut aws = aws_snapshot();
        aws.metadata.warnings = vec![
            "EC2 Standard SQL rate for m-test uses the regional four-core-minimum fallback."
                .to_owned(),
            "EC2 Enterprise SQL rate for c4.large uses the regional four-core-minimum fallback."
                .to_owned(),
            "Catalog data was normalized with a documented fallback.".to_owned(),
        ];
        let azure = azure_snapshot();

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: None,
            })
            .expect("calculation");

        assert_eq!(
            revision.warnings,
            vec![
                "Catalog data was normalized with a documented fallback.",
                "EC2 Standard SQL rate for m-test uses the regional four-core-minimum fallback.",
            ]
        );
    }

    #[test]
    fn missing_azure_snapshot_keeps_mapping_and_source_cost() {
        let engine = engine(None);
        let settings = settings();
        let resource = Resource::Ec2(ec2_resource("100"));
        let aws = aws_snapshot();

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: None,
                expected_formula_version: None,
            })
            .expect("calculation");

        let row = &revision.resource_results[0];
        assert_eq!(row.mapping_status, Some(MappingStatus::Mapped));
        assert!(row.source_costs.is_some());
        assert_eq!(row.azure_pricing_status, PricingStatus::Unavailable);
        assert!(row.savings.is_none());
    }

    #[test]
    fn mapped_resource_explains_cost_savings_and_parity_formulas() {
        let engine = engine(None);
        let settings = settings();
        let resource = Resource::Ec2(ec2_resource("100"));
        let aws = aws_snapshot();
        let azure = azure_snapshot();

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("calculation");

        let row = &revision.resource_results[0];
        let source_costs = row.source_costs.as_ref().expect("source costs");
        let azure_costs = row.azure_costs.as_ref().expect("Azure costs");
        let discounts = row
            .purchase_option_discounts
            .as_ref()
            .expect("purchase option discounts");
        let savings = row.savings.as_ref().expect("savings");
        let step = |code: &str| {
            row.explanation_steps
                .iter()
                .find(|step| step.code == code)
                .unwrap_or_else(|| panic!("missing {code} explanation step"))
        };

        let source = step("source_cost_formula");
        assert_eq!(
            source.values.get("compute_formula").map(String::as_str),
            Some("compute_gross = quantity * annual_hours_per_instance * compute_hourly")
        );
        assert_eq!(
            source.values.get("compute_net"),
            Some(&source_costs.compute_net.to_string())
        );
        assert_eq!(
            source.values.get("source_total"),
            Some(&source_costs.total.to_string())
        );

        let azure = step("azure_cost_formula");
        assert_eq!(
            azure
                .values
                .get("additional_ram_formula")
                .map(String::as_str),
            Some(
                "additional_ram_gross = quantity * annual_hours_per_instance * additional_ram_gb * additional_memory_per_gb_hourly"
            )
        );
        assert_eq!(
            azure.values.get("compute_plus_ram_net"),
            Some(&azure_costs.compute_plus_ram_net.to_string())
        );
        assert_eq!(
            azure.values.get("azure_total_before_parity"),
            Some(&azure_costs.total_before_parity.to_string())
        );
        assert_eq!(discounts.payg, DecimalValue::ZERO);
        assert_eq!(discounts.one_year_reserved, decimal("0.25"));
        assert_eq!(discounts.three_year_reserved, decimal("0.375"));
        assert_eq!(discounts.one_year_savings_plan, decimal("0.125"));
        assert_eq!(discounts.azure_hybrid_benefit, decimal("1"));

        let savings_step = step("savings_formula");
        assert_eq!(
            savings_step
                .values
                .get("total_savings_formula")
                .map(String::as_str),
            Some("total_savings = source_total - azure_total_before_parity")
        );
        assert_eq!(
            savings_step.values.get("total_savings"),
            Some(&savings.total_savings.to_string())
        );

        let parity = step("parity_formula");
        assert_eq!(
            parity
                .values
                .get("required_adjustment_formula")
                .map(String::as_str),
            Some(
                "required_adjustment = if azure_total_before_parity == 0 then 0 else 1 - source_total / azure_total_before_parity"
            )
        );
        assert_eq!(
            parity.values.get("required_adjustment"),
            Some(&savings.required_adjustment.to_string())
        );
        assert_eq!(
            parity.values.get("difference"),
            Some(&savings.difference.to_string())
        );
    }

    #[test]
    fn on_prem_resource_explains_hardware_license_and_electricity_formulas() {
        let engine = engine(None);
        let mut settings = settings();
        settings.project_type = ProjectType::OnPrem;
        settings.aws_region = None;
        settings.source_license_discount = decimal("0.05");
        settings.standard_license_sa_usd_per_two_core_pack = Some(decimal("1200"));
        settings.enterprise_license_sa_usd_per_two_core_pack = Some(decimal("4800"));
        settings.remaining_coverage_months = Some(36);
        settings.electricity_rate_usd_per_kwh = Some(decimal("0.20"));
        let resource = Resource::OnPrem(OnPremResource {
            shared: SharedResource {
                id: Uuid::new_v4(),
                workload_name: "Synthetic on-prem workload".to_owned(),
                server_name: None,
                quantity: 2,
                source_ram_gb_per_instance: decimal("256"),
                annual_hours_per_instance: decimal("8760"),
            },
            sql: SqlWorkload {
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::LicenseIncluded,
                sql_data_gb_per_instance: decimal("1024"),
                mi_purchase_option: PurchaseOption::Ahb,
            },
            source_vcpu: 16,
            licensable_cores: 16,
            source_max_iops: 5000,
            hardware_capex_usd: decimal("24000"),
            depreciation_years: decimal("4"),
            average_power_kw_override: Some(decimal("0.75")),
        });
        let azure = azure_snapshot();

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: None,
                azure_snapshot: Some(&azure),
                expected_formula_version: Some("1.0.0"),
            })
            .expect("calculation");

        let row = &revision.resource_results[0];
        let costs = row.source_costs.as_ref().expect("source costs");
        let source = row
            .explanation_steps
            .iter()
            .find(|step| step.code == "source_cost_formula")
            .expect("source formula explanation");
        assert_eq!(
            source.values.get("hardware_formula").map(String::as_str),
            Some("hardware_annual = quantity * hardware_capex_usd / depreciation_years")
        );
        assert_eq!(
            source.values.get("license_pack_count").map(String::as_str),
            Some("8")
        );
        assert_eq!(
            source
                .values
                .get("license_sa_usd_per_two_core_pack")
                .map(String::as_str),
            Some("4800")
        );
        assert_eq!(
            source.values.get("electricity_annual"),
            Some(&costs.electricity_annual.to_string())
        );
        assert_eq!(
            source.values.get("source_total"),
            Some(&costs.total.to_string())
        );
    }

    fn engine(maximum_storage_gb: Option<&str>) -> CalculationEngine {
        CalculationEngine::new(
            Arc::new(CapabilityCatalog {
                schema_version: "test".to_owned(),
                candidates: vec![TargetCandidate {
                    configuration_key: "nggp-8".to_owned(),
                    azure_region: "swedencentral".to_owned(),
                    service_tier: ServiceTier::NextGenerationGeneralPurpose,
                    hardware_family: "Premium Series".to_owned(),
                    vcores: 8,
                    zone_redundant: false,
                    included_memory_gb: decimal("56"),
                    supported_memory_gb: vec![decimal("56"), decimal("64")],
                    storage_architecture: "Remote LRS".to_owned(),
                    maximum_storage_gb: maximum_storage_gb.map(decimal),
                    source_url: "https://learn.microsoft.com/azure/azure-sql/managed-instance/resource-limits"
                        .to_owned(),
                    reviewed_date: "2026-07-31".to_owned(),
                }],
            }),
            "1.0.0",
        )
        .expect("engine")
    }

    fn capacity_engine() -> CalculationEngine {
        CalculationEngine::new(
            Arc::new(CapabilityCatalog {
                schema_version: "test".to_owned(),
                candidates: vec![
                    TargetCandidate {
                        configuration_key:
                            "managed-vcore-next-gen-general-purpose-premium-series-128"
                                .to_owned(),
                        azure_region: "swedencentral".to_owned(),
                        service_tier: ServiceTier::NextGenerationGeneralPurpose,
                        hardware_family: "Premium Series".to_owned(),
                        vcores: 128,
                        zone_redundant: false,
                        included_memory_gb: decimal("560"),
                        supported_memory_gb: vec![decimal("560")],
                        storage_architecture: "Remote LRS".to_owned(),
                        maximum_storage_gb: Some(decimal("32768")),
                        source_url: "https://learn.microsoft.com/azure/azure-sql/managed-instance/resource-limits"
                            .to_owned(),
                        reviewed_date: "2026-08-14".to_owned(),
                    },
                    TargetCandidate {
                        configuration_key: "managed-vcore-next-gen-general-purpose-premium-series-memory-optimized-128"
                            .to_owned(),
                        azure_region: "swedencentral".to_owned(),
                        service_tier: ServiceTier::NextGenerationGeneralPurpose,
                        hardware_family: "Premium Series Memory Optimized".to_owned(),
                        vcores: 128,
                        zone_redundant: false,
                        included_memory_gb: decimal("870.4"),
                        supported_memory_gb: vec![decimal("870.4")],
                        storage_architecture: "Remote LRS".to_owned(),
                        maximum_storage_gb: Some(decimal("32768")),
                        source_url: "https://learn.microsoft.com/azure/azure-sql/managed-instance/resource-limits"
                            .to_owned(),
                        reviewed_date: "2026-08-14".to_owned(),
                    },
                ],
            }),
            "1.0.0",
        )
        .expect("capacity engine")
    }

    fn settings() -> ProjectSettings {
        ProjectSettings {
            project_type: ProjectType::Ec2,
            aws_region: Some("eu-west-1".to_owned()),
            azure_region: "swedencentral".to_owned(),
            currency: "USD".to_owned(),
            source_compute_discount: DecimalValue::ZERO,
            source_license_discount: DecimalValue::ZERO,
            source_storage_discount: DecimalValue::ZERO,
            azure_compute_discount: DecimalValue::ZERO,
            azure_license_discount: DecimalValue::ZERO,
            azure_storage_discount: DecimalValue::ZERO,
            selected_parity_adjustment: DecimalValue::ZERO,
            default_annual_hours: decimal("8760"),
            default_mi_purchase_option: PurchaseOption::Ahb,
            enterprise_license_sa_usd_per_two_core_pack: None,
            standard_license_sa_usd_per_two_core_pack: None,
            remaining_coverage_months: None,
            electricity_rate_usd_per_kwh: None,
            sql_payg: None,
        }
    }

    fn ec2_resource(sql_data_gb: &str) -> Ec2Resource {
        Ec2Resource {
            shared: SharedResource {
                id: Uuid::new_v4(),
                workload_name: "Synthetic workload".to_owned(),
                server_name: None,
                quantity: 1,
                source_ram_gb_per_instance: decimal("64"),
                annual_hours_per_instance: decimal("8760"),
            },
            sql: SqlWorkload {
                sql_edition: SqlEdition::Standard,
                license_basis: LicenseBasis::Byol,
                sql_data_gb_per_instance: decimal(sql_data_gb),
                mi_purchase_option: PurchaseOption::Ahb,
            },
            instance_type: "m-test".to_owned(),
            volumes: vec![EbsVolume {
                id: Uuid::new_v4(),
                label: "D".to_owned(),
                aws_volume_id: None,
                volume_type: EbsVolumeType::Ephemeral,
                capacity_gb: DecimalValue::ZERO,
                provisioned_iops: None,
                throughput_mibps: None,
            }],
        }
    }

    fn aws_snapshot() -> AwsPriceSnapshot {
        aws_snapshot_for("m-test", 8, "64")
    }

    fn vm_aws_snapshot() -> AwsPriceSnapshot {
        vm_aws_snapshot_for("m5.2xlarge", 8, "32")
    }

    fn vm_aws_snapshot_for(
        instance_type: &str,
        source_vcpu: u32,
        catalog_memory_gb: &str,
    ) -> AwsPriceSnapshot {
        AwsPriceSnapshot::create(
            snapshot_metadata("aws-vm"),
            "eu-west-1",
            vec![AwsEc2RateRecord {
                stable_key: instance_type.to_owned(),
                instance_type: instance_type.to_owned(),
                rate: Ec2Rate {
                    source_vcpu,
                    catalog_memory_gb: decimal(catalog_memory_gb),
                    compute_hourly: decimal("1"),
                    standard_license_hourly: None,
                    enterprise_license_hourly: None,
                },
                provenance: provenance("https://example.test/aws-vm"),
            }],
            Vec::new(),
            vec![AwsEbsRateRecord {
                stable_key: "gp3".to_owned(),
                rate: EbsRate {
                    volume_type: EbsVolumeType::Gp3,
                    capacity_monthly_per_gb: decimal("0.08"),
                    included_iops: 3_000,
                    iops_monthly_per_unit: Some(decimal("0.005")),
                    iops_tiers: Vec::new(),
                    included_throughput_mibps: decimal("125"),
                    throughput_monthly_per_mibps: Some(decimal("0.04")),
                },
                provenance: provenance("https://example.test/aws-ebs"),
            }],
        )
        .expect("AWS VM snapshot")
    }

    fn reviewed_vm_engine() -> CalculationEngine {
        let base = engine(None);
        CalculationEngine::with_vm_catalogs(
            Arc::clone(&base.capabilities),
            Arc::new(
                serde_json::from_str(include_str!(
                    "../../../app/catalogs/azure-vm-capabilities.json"
                ))
                .expect("reviewed VM catalog"),
            ),
            Arc::new(
                serde_json::from_str(include_str!(
                    "../../../app/catalogs/azure-managed-disk-capabilities.json"
                ))
                .expect("reviewed disk catalog"),
            ),
            "1.0.0",
        )
        .expect("reviewed VM engine")
    }

    fn reviewed_vm_resource(
        instance_type: &str,
        memory_gb: &str,
        requirements: Ec2VmRequirements,
    ) -> Resource {
        Resource::Ec2Vm(Ec2VmResource {
            shared: SharedResource {
                id: Uuid::new_v4(),
                workload_name: "Synthetic Windows VM".to_owned(),
                server_name: None,
                quantity: 1,
                source_ram_gb_per_instance: decimal(memory_gb),
                annual_hours_per_instance: decimal("8760"),
            },
            instance_type: instance_type.to_owned(),
            vm_purchase_option: VmPurchaseOption::Payg,
            requirements,
            volumes: vec![VmVolume {
                id: Uuid::new_v4(),
                label: "OS".to_owned(),
                aws_volume_id: None,
                volume_type: EbsVolumeType::Gp3,
                role: VmDiskRole::Os,
                capacity_gb: decimal("32"),
                provisioned_iops: Some(120),
                throughput_mibps: Some(decimal("25")),
            }],
        })
    }

    fn aws_snapshot_for(
        instance_type: &str,
        source_vcpu: u32,
        catalog_memory_gb: &str,
    ) -> AwsPriceSnapshot {
        AwsPriceSnapshot::create(
            snapshot_metadata("aws"),
            "eu-west-1",
            vec![AwsEc2RateRecord {
                stable_key: instance_type.to_owned(),
                instance_type: instance_type.to_owned(),
                rate: Ec2Rate {
                    source_vcpu,
                    catalog_memory_gb: decimal(catalog_memory_gb),
                    compute_hourly: decimal("1"),
                    standard_license_hourly: Some(decimal("0.48")),
                    enterprise_license_hourly: Some(decimal("1.5")),
                },
                provenance: provenance("https://example.test/aws"),
            }],
            Vec::new(),
            Vec::new(),
        )
        .expect("AWS snapshot")
    }

    fn azure_snapshot() -> AzurePriceSnapshot {
        azure_snapshot_for("nggp-8")
    }

    fn azure_snapshot_for(configuration_key: &str) -> AzurePriceSnapshot {
        AzurePriceSnapshot::create(
            snapshot_metadata("azure"),
            "swedencentral",
            PurchaseOption::ALL
                .iter()
                .enumerate()
                .map(|(index, purchase_option)| AzureMiRateRecord {
                    stable_key: format!("{configuration_key}|{index}"),
                    configuration_key: configuration_key.to_owned(),
                    purchase_option: *purchase_option,
                    rate: AzureRate {
                        compute_hourly: match purchase_option {
                            PurchaseOption::Payg | PurchaseOption::Ahb => decimal("0.8"),
                            PurchaseOption::OneYear | PurchaseOption::AhbOneYear => decimal("0.6"),
                            PurchaseOption::ThreeYear | PurchaseOption::AhbThreeYear => {
                                decimal("0.5")
                            }
                            PurchaseOption::SavingsOneYear | PurchaseOption::AhbSavingsOneYear => {
                                decimal("0.7")
                            }
                        },
                        license_hourly: if matches!(
                            purchase_option,
                            PurchaseOption::Ahb
                                | PurchaseOption::AhbOneYear
                                | PurchaseOption::AhbThreeYear
                                | PurchaseOption::AhbSavingsOneYear
                        ) {
                            DecimalValue::ZERO
                        } else {
                            decimal("0.4")
                        },
                        storage_monthly_per_gb: decimal("0.10"),
                        additional_memory_per_gb_hourly: decimal("0.01"),
                    },
                    provenance: provenance("https://example.test/azure"),
                })
                .collect(),
        )
        .expect("Azure snapshot")
    }

    fn vm_azure_snapshot() -> AzurePriceSnapshot {
        AzurePriceSnapshot::create_with_vm_rates(
            snapshot_metadata("azure-vm"),
            "swedencentral",
            Vec::new(),
            vec![AzureVmRateRecord {
                stable_key: "Standard_D8s_v5".to_owned(),
                arm_sku_name: "Standard_D8s_v5".to_owned(),
                purchase_option: VmPurchaseOption::Payg,
                hourly_rate: decimal("0.8"),
                license_hourly: DecimalValue::ZERO,
                unit_of_measure: "1 Hour".to_owned(),
                raw_price_lexeme: "0.8".to_owned(),
                provenance: provenance("https://example.test/azure-vm"),
            }],
            vec![AzureManagedDiskRateRecord {
                stable_key: "premium-ssd-lrs|P30".to_owned(),
                offer_key: "premium-ssd-lrs".to_owned(),
                tier_key: Some("P30".to_owned()),
                dimension: AzureManagedDiskPriceDimension::CapacityTier,
                normalized_monthly_rate: decimal("122.88"),
                unit_of_measure: "1/Month".to_owned(),
                raw_price_lexeme: "122.88".to_owned(),
                provenance: provenance("https://example.test/azure-disk"),
            }],
        )
        .expect("Azure VM snapshot")
    }

    fn reviewed_vm_azure_snapshot(arm_sku_name: &str) -> AzurePriceSnapshot {
        reviewed_vm_azure_snapshot_with_options(arm_sku_name, &VmPurchaseOption::ALL)
    }

    fn reviewed_vm_azure_snapshot_without_reservations(
        arm_sku_name: &str,
    ) -> AzurePriceSnapshot {
        reviewed_vm_azure_snapshot_with_options(
            arm_sku_name,
            &[
                VmPurchaseOption::Payg,
                VmPurchaseOption::Ahb,
                VmPurchaseOption::SavingsOneYear,
                VmPurchaseOption::AhbSavingsOneYear,
                VmPurchaseOption::SavingsThreeYear,
                VmPurchaseOption::AhbSavingsThreeYear,
            ],
        )
    }

    fn reviewed_vm_azure_snapshot_with_options(
        arm_sku_name: &str,
        options: &[VmPurchaseOption],
    ) -> AzurePriceSnapshot {
        AzurePriceSnapshot::create_with_vm_rates(
            snapshot_metadata("azure-reviewed-vm"),
            "swedencentral",
            Vec::new(),
            options
                .iter()
                .map(|purchase_option| {
                    let compute = match purchase_option {
                        VmPurchaseOption::Payg | VmPurchaseOption::Ahb => decimal("0.6"),
                        VmPurchaseOption::OneYear | VmPurchaseOption::AhbOneYear => {
                            decimal("0.45")
                        }
                        VmPurchaseOption::ThreeYear | VmPurchaseOption::AhbThreeYear => {
                            decimal("0.35")
                        }
                        VmPurchaseOption::SavingsOneYear
                        | VmPurchaseOption::AhbSavingsOneYear => decimal("0.5"),
                        VmPurchaseOption::SavingsThreeYear
                        | VmPurchaseOption::AhbSavingsThreeYear => decimal("0.4"),
                    };
                    let license = if purchase_option.uses_ahb() {
                        DecimalValue::ZERO
                    } else {
                        decimal("0.2")
                    };
                    let hourly_rate = DecimalValue(compute.0 + license.0);
                    AzureVmRateRecord {
                        stable_key: format!(
                            "{}|{}",
                            arm_sku_name.to_ascii_lowercase(),
                            purchase_option.as_str()
                        ),
                        arm_sku_name: arm_sku_name.to_owned(),
                        purchase_option: *purchase_option,
                        hourly_rate,
                        license_hourly: license,
                        unit_of_measure: "1 Hour".to_owned(),
                        raw_price_lexeme: hourly_rate.to_string(),
                        provenance: provenance("https://example.test/azure-reviewed-vm"),
                    }
                })
                .collect(),
            vec![AzureManagedDiskRateRecord {
                stable_key: "premium_ssd_lrs|P4".to_owned(),
                offer_key: "premium_ssd_lrs".to_owned(),
                tier_key: Some("P4".to_owned()),
                dimension: AzureManagedDiskPriceDimension::CapacityTier,
                normalized_monthly_rate: decimal("5"),
                unit_of_measure: "1/Month".to_owned(),
                raw_price_lexeme: "5".to_owned(),
                provenance: provenance("https://example.test/azure-reviewed-disk"),
            }],
        )
        .expect("reviewed Azure VM snapshot")
    }

    fn snapshot_metadata(provider: &str) -> SnapshotCreationMetadata {
        SnapshotCreationMetadata {
            status: ResolutionStatus::Fresh,
            retrieved_at: "2026-01-01T00:00:00Z".to_owned(),
            source_published_at: None,
            currency: "USD".to_owned(),
            source_urls: vec![format!("https://example.test/{provider}")],
            parser_schema_version: "test".to_owned(),
            warnings: Vec::new(),
        }
    }

    fn provenance(source_url: &str) -> RateProvenance {
        RateProvenance {
            source_url: source_url.to_owned(),
            effective_at: None,
            source_version: None,
            meter_ids: Vec::new(),
        }
    }

    fn decimal(value: &str) -> DecimalValue {
        DecimalValue(Decimal::from_str(value).expect("valid decimal"))
    }
}
