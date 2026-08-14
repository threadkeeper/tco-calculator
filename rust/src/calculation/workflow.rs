use std::{collections::BTreeMap, sync::Arc};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        decimal::DecimalValue,
        project::{EditableProject, ProjectSettings, ValidationIssue},
        resource::{Ec2Resource, OnPremResource, RdsResource, Resource},
    },
    pricing::{
        provider::{Provider, ResolutionStatus},
        snapshot::{AwsPriceSnapshot, AzurePriceSnapshot},
        warnings::relevant_for_resources,
    },
};

use super::{
    cost::{
        AzureCostBreakdown, AzureRate, CostError, OnPremExplanation, SavingsBreakdown,
        SourceCostBreakdown, calculate_azure, calculate_ec2_source, calculate_on_prem_source,
        calculate_rds_source, calculate_savings, source_max_iops,
    },
    target_selector::{
        CapabilityCatalog, MappingStatus, TargetSelection, TargetSelectionError,
        TargetSelectionRequest, select_target,
    },
};

#[derive(Clone)]
pub struct CalculationEngine {
    capabilities: Arc<CapabilityCatalog>,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResourceCalculation {
    pub resource_id: Uuid,
    pub mapping_status: Option<MappingStatus>,
    pub aws_pricing_status: PricingStatus,
    pub azure_pricing_status: PricingStatus,
    pub target_selection: Option<TargetSelection>,
    pub source_costs: Option<SourceCostBreakdown>,
    pub azure_costs: Option<AzureCostBreakdown>,
    pub savings: Option<SavingsBreakdown>,
    pub explanation_steps: Vec<ExplanationStep>,
    pub unresolved_components: Vec<UnresolvedComponent>,
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
    Cost(#[from] CostError),
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
            formula_version: formula_version.into(),
        })
    }

    pub fn calculate(
        &self,
        input: CalculationInput<'_>,
    ) -> Result<CalculationRevision, CalculationError> {
        self.validate_input(&input)?;
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
        let mut source = match resource {
            Resource::Ec2(resource) => resolve_ec2_source(resource, input),
            Resource::Rds(resource) => resolve_rds_source(resource, input),
            Resource::OnPrem(resource) => resolve_on_prem_source(resource, input.settings)?,
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
                mapping_status: None,
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: PricingStatus::Unavailable,
                target_selection: None,
                source_costs: source.costs,
                azure_costs: None,
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
                sql_data_gb: shared.sql_data_gb_per_instance,
                source_max_iops,
                workbook_parity_mode: true,
            },
        )?;
        source
            .explanation_steps
            .push(source_input_step(source_vcpu, source_max_iops, resource));
        if let Some(source_costs) = source.costs.as_ref()
            && let Some(step) =
                source_cost_formula_step(resource, source_vcpu, input.settings, source_costs)
        {
            source.explanation_steps.push(step);
        }

        if target_selection.mapping_status == MappingStatus::NoMapping {
            return Ok(ResourceCalculation {
                resource_id: shared.id,
                mapping_status: Some(MappingStatus::NoMapping),
                aws_pricing_status: source.pricing_status,
                azure_pricing_status: PricingStatus::NotRequired,
                target_selection: Some(target_selection),
                source_costs: source.costs,
                azure_costs: None,
                savings: None,
                explanation_steps: source.explanation_steps,
                unresolved_components: source.unresolved_components,
            });
        }

        let selected = target_selection
            .selected
            .as_ref()
            .ok_or(CalculationError::InvalidTargetSelection)?;
        let (azure_costs, azure_pricing_status, azure_unresolved, azure_steps) =
            resolve_azure_costs(selected, resource, input);
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
            mapping_status: Some(MappingStatus::Mapped),
            aws_pricing_status: source.pricing_status,
            azure_pricing_status,
            target_selection: Some(target_selection),
            source_costs: source.costs,
            azure_costs,
            savings,
            explanation_steps: source.explanation_steps,
            unresolved_components: source.unresolved_components,
        })
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
                message: "A usable Azure price snapshot is required for target cost.".to_owned(),
            }],
            Vec::new(),
        );
    };
    if !snapshot.has_complete_mi_rate_set(&selected.configuration_key) {
        return (
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
    let Some(record) = snapshot.mi_rate(
        &selected.configuration_key,
        resource.shared().mi_purchase_option,
    ) else {
        return (
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
        resource.shared().sql_data_gb_per_instance,
        selected.included_memory_gb,
        selected.selected_memory_gb,
        record.rate,
        input.settings,
    ) {
        Ok(costs) => {
            let explanation_steps = vec![
                target_provenance_step(
                    &record.provenance.source_url,
                    &snapshot.metadata.retrieved_at,
                ),
                azure_cost_formula_step(selected, resource, record.rate, input.settings, &costs),
            ];
            (
                Some(costs),
                pricing_status(snapshot.metadata.status),
                Vec::new(),
                explanation_steps,
            )
        }
        Err(error) => (
            None,
            PricingStatus::Unavailable,
            vec![cost_unresolved(Provider::Azure, error)],
            Vec::new(),
        ),
    }
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
                shared.sql_data_gb_per_instance.to_string(),
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
        Resource::Rds(_) => {
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
                shared.sql_data_gb_per_instance.to_string(),
            );
            values.insert(
                "storage_monthly_per_gb".to_owned(),
                divide_or_zero(
                    costs.storage_gross.0,
                    quantity * shared.sql_data_gb_per_instance.0 * Decimal::from(12),
                )
                .to_string(),
            );
        }
        Resource::OnPrem(_) => return None,
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
    rate: AzureRate,
    settings: &ProjectSettings,
    costs: &AzureCostBreakdown,
) -> ExplanationStep {
    let shared = resource.shared();
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
                "storage_gross = quantity * sql_data_gb_per_instance * 12 * mi_storage_monthly_per_gb"
                    .to_owned(),
            ),
            (
                "sql_data_gb_per_instance".to_owned(),
                shared.sql_data_gb_per_instance.to_string(),
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
                resource.shared.sql_data_gb_per_instance.to_string(),
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
    let license_price = match resource.shared.sql_edition {
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
            cost::{AzureRate, Ec2Rate},
            target_selector::{SelectionReasonCode, ServiceTier, TargetCandidate},
        },
        domain::resource::{
            EbsVolume, EbsVolumeType, LicenseBasis, ProjectType, PurchaseOption, SharedResource,
            SqlEdition,
        },
        pricing::snapshot::{
            AwsEc2RateRecord, AzureMiRateRecord, RateProvenance, SnapshotCreationMetadata,
        },
    };

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
    fn very_large_ec2_shape_uses_closest_sql_mi_capacity_with_explicit_notes() {
        let engine = capacity_engine();
        let settings = settings();
        let resource = Resource::Ec2(Ec2Resource {
            shared: SharedResource {
                id: Uuid::new_v4(),
                workload_name: "Synthetic large-memory EC2 workload".to_owned(),
                quantity: 1,
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::Byol,
                sql_data_gb_per_instance: decimal("4096"),
                source_ram_gb_per_instance: decimal("1536"),
                annual_hours_per_instance: decimal("8760"),
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
                quantity: 1,
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::LicenseIncluded,
                sql_data_gb_per_instance: decimal("4096"),
                source_ram_gb_per_instance: decimal("1024"),
                annual_hours_per_instance: decimal("8760"),
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
                quantity: 2,
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::LicenseIncluded,
                sql_data_gb_per_instance: decimal("1024"),
                source_ram_gb_per_instance: decimal("256"),
                annual_hours_per_instance: decimal("8760"),
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
        }
    }

    fn ec2_resource(sql_data_gb: &str) -> Ec2Resource {
        Ec2Resource {
            shared: SharedResource {
                id: Uuid::new_v4(),
                workload_name: "Synthetic workload".to_owned(),
                quantity: 1,
                sql_edition: SqlEdition::Standard,
                license_basis: LicenseBasis::Byol,
                sql_data_gb_per_instance: decimal(sql_data_gb),
                source_ram_gb_per_instance: decimal("64"),
                annual_hours_per_instance: decimal("8760"),
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
                        compute_hourly: decimal("0.8"),
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
