use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection, rejection::PathRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    calculation::{
        engine::{CalculationRevision, PricingStatus, ResourceCalculation},
        target_selector::{MappingStatus, ServiceTier},
    },
    config::{AppEnvironment, FORMULA_VERSION},
    domain::{
        decimal::DecimalValue,
        project::ProjectDocument,
        resource::{ProjectType, PurchaseOption, Resource},
    },
    persistence::calculator_launch::{
        CALCULATOR_CONTRACT_VERSION, CALCULATOR_MANIFEST_VERSION, CALCULATOR_PROTOCOL_VERSION,
        CalculatorDeploymentModel, CalculatorExpectedPublicAnnual, CalculatorHardwareFamily,
        CalculatorLaunchError, CalculatorManifest, CalculatorManifestItem, CalculatorProduct,
        CalculatorPurchaseOption, CalculatorServiceTier, MAX_CALCULATOR_MANIFEST_ITEMS,
        MINIMUM_COMPANION_VERSION, NewCalculatorLaunch,
    },
    persistence::repository::{RepositoryError, current_timestamp},
    problem::Problem,
    state::AppState,
};

use super::require_principal;

const CALCULATOR_URL: &str = "https://azure.microsoft.com/en-us/pricing/calculator/";
const CREATE_INSTANCE: &str = "/api/v1/projects/{project_id}/calculator-launches";
const CLAIM_INSTANCE: &str = "/api/v1/calculator-launches/{launch_id}/claim";
const ACKNOWLEDGE_INSTANCE: &str = "/api/v1/calculator-launches/{launch_id}/acknowledge";
const FETCH_SITE_HEADER: &str = "sec-fetch-site";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCalculatorLaunchRequest {
    launch_id: Uuid,
    protocol_version: u16,
}

#[derive(Debug, Serialize)]
struct CalculatorLaunchResponse {
    launch_id: Uuid,
    status: &'static str,
    claim_expires_at: String,
    minimum_companion_version: String,
    protocol_version: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimCalculatorLaunchRequest {
    companion_instance_id: Uuid,
    companion_version: String,
    supported_protocol_versions: Vec<u16>,
    supported_manifest_versions: Vec<u16>,
    supported_calculator_contracts: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ClaimedCalculatorLaunchResponse {
    manifest_sha256: String,
    manifest: CalculatorManifest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcknowledgeCalculatorLaunchRequest {
    companion_instance_id: Uuid,
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<CreateCalculatorLaunchRequest>, JsonRejection>,
) -> Result<Response, Problem> {
    let settings = state
        .config
        .calculator_companion
        .as_ref()
        .ok_or_else(|| Problem::calculator_unavailable(CREATE_INSTANCE))?;
    validate_same_origin(&headers, &settings.application_origin)?;
    let principal = require_principal(&headers, &state, CREATE_INSTANCE)?;
    let Path(project_id) = path.map_err(|_| Problem::malformed_request(CREATE_INSTANCE))?;
    let request = request
        .map_err(|error| super::json_rejection(error, CREATE_INSTANCE))?
        .0;
    if request.launch_id.is_nil() || request.protocol_version != CALCULATOR_PROTOCOL_VERSION {
        return Err(Problem::malformed_request(CREATE_INSTANCE));
    }
    let if_match = required_if_match(&headers, CREATE_INSTANCE)?;
    let owner_id = principal.owner_id();
    let project = state
        .projects
        .get(&owner_id, project_id)
        .await
        .map_err(|error| map_project_error(error, CREATE_INSTANCE))?;
    if project.etag != if_match {
        return Err(Problem::precondition_failed(
            CREATE_INSTANCE,
            Some(&project.etag),
        ));
    }
    let manifest = build_calculator_manifest(
        &project,
        current_timestamp().map_err(|_| Problem::internal(CREATE_INSTANCE))?,
    )
    .map_err(|_| Problem::calculator_ineligible(CREATE_INSTANCE))?;
    let source_azure_snapshot_id = project
        .azure_price_snapshot_id
        .clone()
        .ok_or_else(|| Problem::calculator_ineligible(CREATE_INSTANCE))?;
    let created = state
        .calculator_launches
        .create(
            &owner_id,
            NewCalculatorLaunch {
                id: request.launch_id,
                source_project_id: project.id,
                source_project_etag: project.etag,
                source_formula_version: project.formula_version,
                source_azure_snapshot_id,
                manifest,
            },
        )
        .await
        .map_err(|error| map_launch_error(error, CREATE_INSTANCE))?;
    let status = if created.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    response_with_no_store(
        status,
        Json(CalculatorLaunchResponse {
            launch_id: created.document.id,
            status: "ready",
            claim_expires_at: created.document.claim_expires_at,
            minimum_companion_version: created.document.minimum_companion_version,
            protocol_version: created.document.protocol_version,
        })
        .into_response(),
    )
}

pub async fn claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<ClaimCalculatorLaunchRequest>, JsonRejection>,
) -> Result<Response, Problem> {
    let settings = state
        .config
        .calculator_companion
        .as_ref()
        .ok_or_else(|| Problem::calculator_unavailable(CLAIM_INSTANCE))?;
    let principal = require_principal(&headers, &state, CLAIM_INSTANCE)?;
    if state.config.environment != AppEnvironment::Local {
        principal
            .authorize_companion(settings.client_application_id, &settings.delegated_scope)
            .map_err(|_| {
                Problem::forbidden(
                    CLAIM_INSTANCE,
                    "The authenticated client is not authorized to claim Calculator launches.",
                )
            })?;
    }
    let Path(launch_id) = path.map_err(|_| Problem::malformed_request(CLAIM_INSTANCE))?;
    let request = request
        .map_err(|error| super::json_rejection(error, CLAIM_INSTANCE))?
        .0;
    validate_claim_request(&request)?;
    let document = state
        .calculator_launches
        .claim(
            &principal.owner_id(),
            launch_id,
            request.companion_instance_id,
            &request.companion_version,
        )
        .await
        .map_err(|error| map_launch_error(error, CLAIM_INSTANCE))?;
    let etag =
        HeaderValue::from_str(&document.etag).map_err(|_| Problem::internal(CLAIM_INSTANCE))?;
    let payload = ClaimedCalculatorLaunchResponse {
        manifest_sha256: document
            .manifest_sha256
            .ok_or_else(|| Problem::internal(CLAIM_INSTANCE))?,
        manifest: document
            .manifest
            .ok_or_else(|| Problem::internal(CLAIM_INSTANCE))?,
    };
    let mut response = Json(payload).into_response();
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(header::ETAG, etag);
    response_with_no_store(StatusCode::OK, response)
}

pub async fn acknowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<AcknowledgeCalculatorLaunchRequest>, JsonRejection>,
) -> Result<Response, Problem> {
    let settings = state
        .config
        .calculator_companion
        .as_ref()
        .ok_or_else(|| Problem::calculator_unavailable(ACKNOWLEDGE_INSTANCE))?;
    let principal = require_principal(&headers, &state, ACKNOWLEDGE_INSTANCE)?;
    if state.config.environment != AppEnvironment::Local {
        principal
            .authorize_companion(settings.client_application_id, &settings.delegated_scope)
            .map_err(|_| {
                Problem::forbidden(
                    ACKNOWLEDGE_INSTANCE,
                    "The authenticated client is not authorized to acknowledge Calculator launches.",
                )
            })?;
    }
    let Path(launch_id) = path.map_err(|_| Problem::malformed_request(ACKNOWLEDGE_INSTANCE))?;
    let request = request
        .map_err(|error| super::json_rejection(error, ACKNOWLEDGE_INSTANCE))?
        .0;
    if request.companion_instance_id.is_nil() {
        return Err(Problem::malformed_request(ACKNOWLEDGE_INSTANCE));
    }
    let if_match = required_if_match(&headers, ACKNOWLEDGE_INSTANCE)?;
    state
        .calculator_launches
        .acknowledge(
            &principal.owner_id(),
            launch_id,
            request.companion_instance_id,
            &if_match,
        )
        .await
        .map_err(|error| map_launch_error(error, ACKNOWLEDGE_INSTANCE))?;
    let response = StatusCode::NO_CONTENT.into_response();
    response_with_no_store(StatusCode::NO_CONTENT, response)
}

fn validate_same_origin(headers: &HeaderMap, application_origin: &str) -> Result<(), Problem> {
    if headers
        .get(header::ORIGIN)
        .is_some_and(|value| value.as_bytes() != application_origin.as_bytes())
        || headers
            .get(FETCH_SITE_HEADER)
            .is_some_and(|value| value.as_bytes() != b"same-origin")
    {
        return Err(Problem::forbidden(
            CREATE_INSTANCE,
            "Calculator launches must be created from the same-origin application.",
        ));
    }
    Ok(())
}

fn validate_claim_request(request: &ClaimCalculatorLaunchRequest) -> Result<(), Problem> {
    if request.companion_instance_id.is_nil()
        || !version_at_least(&request.companion_version, MINIMUM_COMPANION_VERSION)
        || !bounded_unique(&request.supported_protocol_versions)
        || !bounded_unique(&request.supported_manifest_versions)
        || !bounded_unique(&request.supported_calculator_contracts)
        || !request
            .supported_protocol_versions
            .contains(&CALCULATOR_PROTOCOL_VERSION)
        || !request
            .supported_manifest_versions
            .contains(&CALCULATOR_MANIFEST_VERSION)
        || !request
            .supported_calculator_contracts
            .iter()
            .any(|value| value == CALCULATOR_CONTRACT_VERSION)
    {
        return Err(Problem::companion_update_required(CLAIM_INSTANCE));
    }
    Ok(())
}

fn bounded_unique<T: Eq + std::hash::Hash>(values: &[T]) -> bool {
    !values.is_empty()
        && values.len() <= 8
        && values.iter().collect::<HashSet<_>>().len() == values.len()
}

fn version_at_least(actual: &str, minimum: &str) -> bool {
    parse_version(actual)
        .zip(parse_version(minimum))
        .is_some_and(|(actual, minimum)| actual >= minimum)
}

fn parse_version(value: &str) -> Option<[u32; 3]> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

fn required_if_match(headers: &HeaderMap, instance: &str) -> Result<String, Problem> {
    headers
        .get(header::IF_MATCH)
        .ok_or_else(|| Problem::precondition_required(instance))?
        .to_str()
        .map(str::to_owned)
        .map_err(|_| Problem::launch_precondition_failed(instance))
}

fn response_with_no_store(status: StatusCode, mut response: Response) -> Result<Response, Problem> {
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn map_project_error(error: RepositoryError, instance: &str) -> Problem {
    match error {
        RepositoryError::NotFound => {
            Problem::not_found(instance, "The saved project was not found.")
        }
        RepositoryError::PreconditionFailed => Problem::precondition_failed(instance, None),
        RepositoryError::PayloadTooLarge => Problem::payload_too_large(instance),
        RepositoryError::Unavailable => Problem::calculator_unavailable(instance),
    }
}

fn map_launch_error(error: CalculatorLaunchError, instance: &str) -> Problem {
    match error {
        CalculatorLaunchError::NotFound => {
            Problem::not_found(instance, "The Calculator launch was not found.")
        }
        CalculatorLaunchError::Conflict => Problem::conflict(
            instance,
            "The Calculator launch is already active or was claimed by another companion instance.",
        ),
        CalculatorLaunchError::Expired => {
            Problem::gone(instance, "The Calculator launch has expired.")
        }
        CalculatorLaunchError::PreconditionFailed => Problem::launch_precondition_failed(instance),
        CalculatorLaunchError::InvalidManifest | CalculatorLaunchError::PayloadTooLarge => {
            Problem::internal(instance)
        }
        CalculatorLaunchError::Unavailable => Problem::calculator_unavailable(instance),
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CalculatorManifestError {
    #[error("project is not eligible for Calculator launch")]
    IneligibleProject,
    #[error("project calculation is missing or does not match the persisted project")]
    CalculationMismatch,
    #[error("project contains a stale or unavailable Azure price result")]
    PricingUnavailable,
    #[error("project target is not supported by the Calculator contract")]
    UnsupportedTarget,
    #[error("project annual hours cannot be represented exactly by the Calculator contract")]
    InexactMonthlyHours,
    #[error("project or workload name cannot be represented by the Calculator contract")]
    InvalidDisplayName,
}

pub(crate) fn build_calculator_manifest(
    project: &ProjectDocument,
    generated_at: String,
) -> Result<CalculatorManifest, CalculatorManifestError> {
    if project.settings.project_type == ProjectType::SqlPayg
        || project.settings.currency != "USD"
        || project.resources.is_empty()
        || project.resources.len() > MAX_CALCULATOR_MANIFEST_ITEMS
        || !calculator_display_name(&project.name, 100)
    {
        return Err(CalculatorManifestError::IneligibleProject);
    }
    let revision = project
        .latest_calculation_revision
        .as_ref()
        .ok_or(CalculatorManifestError::CalculationMismatch)?;
    validate_revision_binding(project, revision)?;
    let calculations = indexed_calculations(revision)?;

    let mut items = Vec::with_capacity(project.resources.len());
    for (index, resource) in project.resources.iter().enumerate() {
        let calculation = calculations
            .get(&resource.shared().id)
            .copied()
            .ok_or(CalculatorManifestError::CalculationMismatch)?;
        items.push(build_item(index, project, resource, calculation)?);
    }

    Ok(CalculatorManifest {
        schema_version: CALCULATOR_MANIFEST_VERSION,
        calculator_contract_version: CALCULATOR_CONTRACT_VERSION.to_owned(),
        calculator_url: CALCULATOR_URL.to_owned(),
        generated_at,
        currency: "USD".to_owned(),
        locale: "en-US".to_owned(),
        estimate_name: project.name.clone(),
        items,
    })
}

fn validate_revision_binding(
    project: &ProjectDocument,
    revision: &CalculationRevision,
) -> Result<(), CalculatorManifestError> {
    if project.formula_version != FORMULA_VERSION
        || revision.formula_version != project.formula_version
        || project.azure_price_snapshot_id.is_none()
        || revision.azure_snapshot_id != project.azure_price_snapshot_id
        || revision.resource_results.len() != project.resources.len()
    {
        return Err(CalculatorManifestError::CalculationMismatch);
    }
    Ok(())
}

fn indexed_calculations(
    revision: &CalculationRevision,
) -> Result<HashMap<uuid::Uuid, &ResourceCalculation>, CalculatorManifestError> {
    let mut seen = HashSet::with_capacity(revision.resource_results.len());
    let mut calculations = HashMap::with_capacity(revision.resource_results.len());
    for calculation in &revision.resource_results {
        if !seen.insert(calculation.resource_id) {
            return Err(CalculatorManifestError::CalculationMismatch);
        }
        calculations.insert(calculation.resource_id, calculation);
    }
    Ok(calculations)
}

fn build_item(
    index: usize,
    project: &ProjectDocument,
    resource: &Resource,
    calculation: &ResourceCalculation,
) -> Result<CalculatorManifestItem, CalculatorManifestError> {
    if !matches!(
        calculation.azure_pricing_status,
        PricingStatus::Fresh | PricingStatus::Cached
    ) {
        return Err(CalculatorManifestError::PricingUnavailable);
    }
    if calculation.mapping_status != Some(MappingStatus::Mapped) {
        return Err(CalculatorManifestError::UnsupportedTarget);
    }
    let selected = calculation
        .target_selection
        .as_ref()
        .and_then(|selection| selection.selected.as_ref())
        .ok_or(CalculatorManifestError::UnsupportedTarget)?;
    if selected.azure_region != project.settings.azure_region
        || !is_region_code(&selected.azure_region)
    {
        return Err(CalculatorManifestError::UnsupportedTarget);
    }
    let azure_costs = calculation
        .azure_costs
        .as_ref()
        .ok_or(CalculatorManifestError::PricingUnavailable)?;
    let (purchase_option, azure_hybrid_benefit) = calculator_purchase_option(
        resource
            .sql()
            .ok_or(CalculatorManifestError::PricingUnavailable)?
            .mi_purchase_option,
    );
    let expected_total = checked_sum([
        azure_costs.compute_gross,
        azure_costs.additional_ram_gross,
        azure_costs.license_gross,
        azure_costs.storage_gross,
    ])?;
    let ordinal = index + 1;
    let item_key = format!("{ordinal:03}");
    if !calculator_display_name(&resource.shared().workload_name, 160) {
        return Err(CalculatorManifestError::InvalidDisplayName);
    }

    Ok(CalculatorManifestItem {
        display_name: resource.shared().workload_name.clone(),
        item_key,
        product: CalculatorProduct::AzureSqlManagedInstance,
        region: selected.azure_region.clone(),
        deployment_model: CalculatorDeploymentModel::SingleInstance,
        service_tier: match selected.service_tier {
            ServiceTier::NextGenerationGeneralPurpose => {
                CalculatorServiceTier::NextGenerationGeneralPurpose
            }
            ServiceTier::BusinessCritical => CalculatorServiceTier::BusinessCritical,
        },
        hardware_family: calculator_hardware_family(&selected.hardware_family)?,
        vcores: selected.vcores,
        selected_memory_gb: selected.selected_memory_gb,
        zone_redundant: selected.zone_redundant,
        quantity: resource.shared().quantity,
        hours_per_month: exact_monthly_hours(resource.shared().annual_hours_per_instance)?,
        purchase_option,
        azure_hybrid_benefit,
        data_storage_gb: calculation.storage_inputs.azure_storage_gb_per_instance,
        backup_storage_gb: DecimalValue::ZERO,
        expected_public_annual: CalculatorExpectedPublicAnnual {
            compute: azure_costs.compute_gross,
            additional_memory: azure_costs.additional_ram_gross,
            license: azure_costs.license_gross,
            storage: azure_costs.storage_gross,
            total_before_parity: expected_total,
        },
    })
}

fn calculator_hardware_family(
    value: &str,
) -> Result<CalculatorHardwareFamily, CalculatorManifestError> {
    match value {
        "Premium Series" => Ok(CalculatorHardwareFamily::PremiumSeries),
        "Premium Series Memory Optimized" => {
            Ok(CalculatorHardwareFamily::PremiumSeriesMemoryOptimized)
        }
        _ => Err(CalculatorManifestError::UnsupportedTarget),
    }
}

fn calculator_purchase_option(value: PurchaseOption) -> (CalculatorPurchaseOption, bool) {
    match value {
        PurchaseOption::Payg => (CalculatorPurchaseOption::Payg, false),
        PurchaseOption::Ahb => (CalculatorPurchaseOption::Payg, true),
        PurchaseOption::OneYear => (CalculatorPurchaseOption::OneYearReservation, false),
        PurchaseOption::AhbOneYear => (CalculatorPurchaseOption::OneYearReservation, true),
        PurchaseOption::ThreeYear => (CalculatorPurchaseOption::ThreeYearReservation, false),
        PurchaseOption::AhbThreeYear => (CalculatorPurchaseOption::ThreeYearReservation, true),
        PurchaseOption::SavingsOneYear => (CalculatorPurchaseOption::OneYearSavingsPlan, false),
        PurchaseOption::AhbSavingsOneYear => (CalculatorPurchaseOption::OneYearSavingsPlan, true),
    }
}

fn calculator_display_name(value: &str, max_chars: usize) -> bool {
    value.trim() == value
        && (1..=max_chars).contains(&value.chars().count())
        && value.chars().all(|character| !character.is_control())
}

fn exact_monthly_hours(annual: DecimalValue) -> Result<DecimalValue, CalculatorManifestError> {
    let twelve = Decimal::from(12);
    let monthly = annual
        .0
        .checked_div(twelve)
        .ok_or(CalculatorManifestError::InexactMonthlyHours)?;
    if monthly.checked_mul(twelve) != Some(annual.0) {
        return Err(CalculatorManifestError::InexactMonthlyHours);
    }
    Ok(DecimalValue(monthly))
}

fn checked_sum<const SIZE: usize>(
    values: [DecimalValue; SIZE],
) -> Result<DecimalValue, CalculatorManifestError> {
    values
        .into_iter()
        .try_fold(Decimal::ZERO, |total, value| total.checked_add(value.0))
        .map(DecimalValue)
        .ok_or(CalculatorManifestError::PricingUnavailable)
}

fn is_region_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::{
        calculation::{
            cost::AzureCostBreakdown,
            engine::{PortfolioTotals, StorageInputs},
            target_selector::{SelectedTarget, TargetSelection},
        },
        config::SCHEMA_VERSION,
        domain::{
            project::ProjectSettings,
            resource::{LicenseBasis, OnPremResource, SharedResource, SqlEdition, SqlWorkload},
        },
    };

    use super::*;

    #[test]
    fn all_purchase_options_map_to_the_reviewed_plan_and_ahb_pair() {
        let cases = [
            (PurchaseOption::Payg, CalculatorPurchaseOption::Payg, false),
            (PurchaseOption::Ahb, CalculatorPurchaseOption::Payg, true),
            (
                PurchaseOption::OneYear,
                CalculatorPurchaseOption::OneYearReservation,
                false,
            ),
            (
                PurchaseOption::AhbOneYear,
                CalculatorPurchaseOption::OneYearReservation,
                true,
            ),
            (
                PurchaseOption::ThreeYear,
                CalculatorPurchaseOption::ThreeYearReservation,
                false,
            ),
            (
                PurchaseOption::AhbThreeYear,
                CalculatorPurchaseOption::ThreeYearReservation,
                true,
            ),
            (
                PurchaseOption::SavingsOneYear,
                CalculatorPurchaseOption::OneYearSavingsPlan,
                false,
            ),
            (
                PurchaseOption::AhbSavingsOneYear,
                CalculatorPurchaseOption::OneYearSavingsPlan,
                true,
            ),
        ];

        for (source, expected_plan, expected_ahb) in cases {
            assert_eq!(
                calculator_purchase_option(source),
                (expected_plan, expected_ahb)
            );
        }
    }

    #[test]
    fn monthly_hours_must_round_trip_exactly() {
        assert_eq!(
            exact_monthly_hours(decimal("8760")).expect("exact hours"),
            decimal("730")
        );
        assert_eq!(
            exact_monthly_hours(decimal("1")),
            Err(CalculatorManifestError::InexactMonthlyHours)
        );
    }

    #[test]
    fn calculator_display_names_are_bounded_and_printable() {
        assert!(calculator_display_name("Workload 1 EC2 SQL", 100));
        assert!(calculator_display_name("VM5", 160));
        assert!(!calculator_display_name(" VM5", 160));
        assert!(!calculator_display_name("VM5\n", 160));
        assert!(!calculator_display_name(&"x".repeat(161), 160));
    }

    #[test]
    fn manifest_uses_persisted_project_and_workload_names_in_row_order() {
        let project = eligible_project();

        let manifest = build_calculator_manifest(&project, "2026-08-24T12:00:00Z".to_owned())
            .expect("eligible Calculator manifest");

        assert_eq!(manifest.estimate_name, "Workload 1 EC2 SQL");
        assert_eq!(
            manifest
                .items
                .iter()
                .map(|item| (item.item_key.as_str(), item.display_name.as_str()))
                .collect::<Vec<_>>(),
            [("001", "VM5"), ("002", "VM6")]
        );
    }

    fn eligible_project() -> ProjectDocument {
        let resources = vec![resource("VM5"), resource("VM6")];
        let resource_results = resources
            .iter()
            .map(|resource| calculation(resource.shared().id))
            .collect();
        ProjectDocument {
            id: Uuid::new_v4(),
            document_type: "project".to_owned(),
            owner_id: "entra:tenant:user".to_owned(),
            name: "Workload 1 EC2 SQL".to_owned(),
            description: None,
            settings: ProjectSettings {
                project_type: ProjectType::OnPrem,
                aws_region: None,
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
                default_mi_purchase_option: PurchaseOption::Payg,
                enterprise_license_sa_usd_per_two_core_pack: Some(decimal("1000")),
                standard_license_sa_usd_per_two_core_pack: Some(decimal("500")),
                remaining_coverage_months: Some(12),
                electricity_rate_usd_per_kwh: Some(decimal("0.12")),
                sql_payg: None,
            },
            resources,
            aws_price_snapshot_id: None,
            azure_price_snapshot_id: Some("azure-snapshot".to_owned()),
            latest_calculation_revision: Some(CalculationRevision {
                formula_version: FORMULA_VERSION.to_owned(),
                aws_snapshot_id: None,
                azure_snapshot_id: Some("azure-snapshot".to_owned()),
                resource_results,
                portfolio_totals: PortfolioTotals {
                    aws_all_rows_total: None,
                    aws_mapped_rows_total: DecimalValue::ZERO,
                    azure_mapped_rows_total: DecimalValue::ZERO,
                    required_portfolio_adjustment: DecimalValue::ZERO,
                    selected_parity_adjustment: DecimalValue::ZERO,
                    portfolio_after_selected_parity: DecimalValue::ZERO,
                    portfolio_difference: DecimalValue::ZERO,
                    comparable_resource_count: 2,
                    no_mapping_resource_count: 0,
                    price_unavailable_resource_count: 0,
                },
                warnings: Vec::new(),
                sql_payg_analysis: None,
            }),
            formula_version: FORMULA_VERSION.to_owned(),
            schema_version: SCHEMA_VERSION.to_owned(),
            created_at: "2026-08-24T12:00:00Z".to_owned(),
            updated_at: "2026-08-24T12:00:00Z".to_owned(),
            etag: "\"1\"".to_owned(),
        }
    }

    fn resource(workload_name: &str) -> Resource {
        Resource::OnPrem(OnPremResource {
            shared: SharedResource {
                id: Uuid::new_v4(),
                workload_name: workload_name.to_owned(),
                server_name: None,
                quantity: 1,
                source_ram_gb_per_instance: decimal("28"),
                annual_hours_per_instance: decimal("8760"),
            },
            sql: SqlWorkload {
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::Byol,
                sql_data_gb_per_instance: decimal("1024"),
                mi_purchase_option: PurchaseOption::Payg,
            },
            source_vcpu: 4,
            licensable_cores: 4,
            source_max_iops: 6_400,
            hardware_capex_usd: DecimalValue::ZERO,
            depreciation_years: decimal("5"),
            average_power_kw_override: None,
        })
    }

    fn calculation(resource_id: Uuid) -> ResourceCalculation {
        ResourceCalculation {
            resource_id,
            storage_inputs: StorageInputs {
                sql_data_gb_per_instance: decimal("1024"),
                persistent_ebs_gb_per_instance: DecimalValue::ZERO,
                azure_storage_gb_per_instance: decimal("1024"),
            },
            mapping_status: Some(MappingStatus::Mapped),
            aws_pricing_status: PricingStatus::NotRequired,
            azure_pricing_status: PricingStatus::Fresh,
            target_selection: Some(TargetSelection {
                mapping_status: MappingStatus::Mapped,
                requested_tier: ServiceTier::NextGenerationGeneralPurpose,
                nggp_iops_limit: 6_400,
                selected: Some(SelectedTarget {
                    configuration_key: "swedencentral-nggp-premium-4".to_owned(),
                    azure_region: "swedencentral".to_owned(),
                    service_tier: ServiceTier::NextGenerationGeneralPurpose,
                    hardware_family: "Premium Series".to_owned(),
                    vcores: 4,
                    zone_redundant: false,
                    included_memory_gb: decimal("28"),
                    selected_memory_gb: decimal("28"),
                    additional_memory_gb: DecimalValue::ZERO,
                    storage_architecture: "remote".to_owned(),
                    maximum_storage_gb: Some(decimal("16384")),
                }),
                candidates: Vec::new(),
                outcome_reasons: Vec::new(),
                storage_escalation: None,
            }),
            source_costs: None,
            azure_costs: Some(AzureCostBreakdown {
                compute_gross: decimal("100"),
                additional_ram_gb: DecimalValue::ZERO,
                additional_ram_gross: DecimalValue::ZERO,
                compute_plus_ram_net: decimal("100"),
                license_gross: decimal("25"),
                license_net: decimal("25"),
                storage_gross: decimal("10"),
                storage_net: decimal("10"),
                total_before_parity: decimal("135"),
            }),
            purchase_option_discounts: None,
            savings: None,
            explanation_steps: Vec::new(),
            unresolved_components: Vec::new(),
        }
    }

    fn decimal(value: &str) -> DecimalValue {
        DecimalValue(Decimal::from_str(value).expect("decimal"))
    }
}
