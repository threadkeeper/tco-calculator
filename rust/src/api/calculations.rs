use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use serde::Deserialize;

use crate::{
    calculation::engine::{CalculationError, CalculationInput, CalculationRevision},
    domain::{
        project::{EditableProject, ProjectSettings, ValidationIssue},
        resource::Resource,
    },
    problem::Problem,
    state::AppState,
};

const INSTANCE: &str = "/api/v1/calculations";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalculationRequest {
    name: String,
    description: Option<String>,
    settings: ProjectSettings,
    resources: Vec<Resource>,
    aws_price_snapshot_id: Option<String>,
    azure_price_snapshot_id: Option<String>,
    expected_formula_version: Option<String>,
}

impl CalculationRequest {
    fn into_parts(self) -> (EditableProject, Option<String>) {
        (
            EditableProject {
                name: self.name,
                description: self.description,
                settings: self.settings,
                resources: self.resources,
                aws_price_snapshot_id: self.aws_price_snapshot_id,
                azure_price_snapshot_id: self.azure_price_snapshot_id,
            },
            self.expected_formula_version,
        )
    }
}

pub async fn calculate(
    State(state): State<AppState>,
    request: Result<Json<CalculationRequest>, JsonRejection>,
) -> Result<Json<CalculationRevision>, Problem> {
    let _permit = state
        .calculation_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Problem::rate_limited(INSTANCE, 1))?;
    let (project, expected_formula_version) = request
        .map_err(|error| super::json_rejection(error, INSTANCE))?
        .0
        .into_parts();
    calculate_project(&state, &project, expected_formula_version.as_deref())
        .await
        .map(Json)
}

pub(crate) async fn calculate_project(
    state: &AppState,
    project: &EditableProject,
    expected_formula_version: Option<&str>,
) -> Result<CalculationRevision, Problem> {
    let aws_snapshot = match project.aws_price_snapshot_id.as_deref() {
        Some(snapshot_id) => Some(
            state
                .pricing
                .get_aws(snapshot_id)
                .await
                .map_err(|_| Problem::internal(INSTANCE))?
                .ok_or_else(|| Problem::snapshot_unavailable(INSTANCE))?,
        ),
        None => None,
    };
    let azure_snapshot = match project.azure_price_snapshot_id.as_deref() {
        Some(snapshot_id) => Some(
            state
                .pricing
                .get_azure(snapshot_id)
                .await
                .map_err(|_| Problem::internal(INSTANCE))?
                .ok_or_else(|| Problem::snapshot_unavailable(INSTANCE))?,
        ),
        None => None,
    };

    state
        .calculations
        .calculate(CalculationInput {
            settings: &project.settings,
            resources: &project.resources,
            aws_snapshot: aws_snapshot.as_deref(),
            azure_snapshot: azure_snapshot.as_deref(),
            expected_formula_version,
        })
        .map_err(map_calculation_error)
}

fn map_calculation_error(error: CalculationError) -> Problem {
    match error {
        CalculationError::Validation(issues) => Problem::validation(INSTANCE, issues),
        CalculationError::FormulaVersionMismatch => Problem::validation(
            INSTANCE,
            vec![ValidationIssue {
                pointer: "/expected_formula_version".to_owned(),
                code: "mismatch",
                message: "Expected formula version does not match the server version.".to_owned(),
            }],
        ),
        CalculationError::SnapshotScopeMismatch(_) => Problem::snapshot_unavailable(INSTANCE),
        CalculationError::EmptyCapabilityCatalog
        | CalculationError::InvalidTargetSelection
        | CalculationError::TargetSelection(_)
        | CalculationError::VmTargetSelection(_)
        | CalculationError::Cost(_)
        | CalculationError::SqlPayg(_) => Problem::internal(INSTANCE),
    }
}
