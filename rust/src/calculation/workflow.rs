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
    },
};

use super::{
    cost::{
        AzureCostBreakdown, CostError, OnPremExplanation, SavingsBreakdown, SourceCostBreakdown,
        calculate_azure, calculate_ec2_source, calculate_on_prem_source, calculate_rds_source,
        calculate_savings, source_max_iops,
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
            .into_iter()
            .flat_map(|snapshot| snapshot.metadata.warnings.iter().cloned())
            .chain(
                input
                    .azure_snapshot
                    .into_iter()
                    .flat_map(|snapshot| snapshot.metadata.warnings.iter().cloned()),
            )
            .collect::<Vec<_>>();
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
        let (azure_costs, azure_pricing_status, azure_unresolved, azure_step) =
            resolve_azure_costs(selected, resource, input);
        source.unresolved_components.extend(azure_unresolved);
        if let Some(step) = azure_step {
            source.explanation_steps.push(step);
        }
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
    Ok(ResolvedSource {
        source_vcpu: Some(resource.source_vcpu),
        source_max_iops: Some(resource.source_max_iops),
        costs: Some(result.costs),
        pricing_status: PricingStatus::NotRequired,
        explanation_steps: vec![on_prem_power_step(&result.explanation, resource)],
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
    Option<ExplanationStep>,
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
            None,
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
            None,
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
            None,
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
        Ok(costs) => (
            Some(costs),
            pricing_status(snapshot.metadata.status),
            Vec::new(),
            Some(target_provenance_step(
                &record.provenance.source_url,
                &snapshot.metadata.retrieved_at,
            )),
        ),
        Err(error) => (
            None,
            PricingStatus::Unavailable,
            vec![cost_unresolved(Provider::Azure, error)],
            None,
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rust_decimal::Decimal;

    use super::*;
    use crate::{
        calculation::{
            cost::{AzureRate, Ec2Rate},
            target_selector::{ServiceTier, TargetCandidate},
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
        AwsPriceSnapshot::create(
            snapshot_metadata("aws"),
            "eu-west-1",
            vec![AwsEc2RateRecord {
                stable_key: "m-test".to_owned(),
                instance_type: "m-test".to_owned(),
                rate: Ec2Rate {
                    source_vcpu: 8,
                    catalog_memory_gb: decimal("64"),
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
        AzurePriceSnapshot::create(
            snapshot_metadata("azure"),
            "swedencentral",
            PurchaseOption::ALL
                .iter()
                .enumerate()
                .map(|(index, purchase_option)| AzureMiRateRecord {
                    stable_key: format!("nggp-8|{index}"),
                    configuration_key: "nggp-8".to_owned(),
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
