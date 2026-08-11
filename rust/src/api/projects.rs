use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection, rejection::PathRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    calculation::engine::CalculationRevision,
    domain::{
        decimal::DecimalValue,
        project::{EditableProject, ProjectDocument, ProjectSettings},
        resource::{ProjectType, Resource},
    },
    persistence::repository::{RepositoryError, pricing_inputs_unchanged},
    problem::Problem,
    state::AppState,
};

use super::{calculations::calculate_project, require_principal};

const COLLECTION_INSTANCE: &str = "/api/v1/projects";
const ITEM_INSTANCE: &str = "/api/v1/projects/{project_id}";

#[derive(Serialize)]
struct ProjectResponse {
    id: Uuid,
    name: String,
    description: Option<String>,
    settings: ProjectSettings,
    resources: Vec<Resource>,
    aws_price_snapshot_id: Option<String>,
    azure_price_snapshot_id: Option<String>,
    created_at: String,
    updated_at: String,
    formula_version: String,
    schema_version: String,
    latest_calculation_revision: Option<CalculationRevision>,
}

#[derive(Serialize)]
pub(crate) struct ProjectSummary {
    id: Uuid,
    name: String,
    project_type: ProjectType,
    modified_at: String,
    source_region: Option<String>,
    azure_region: String,
    resource_count: usize,
    source_annual_total: Option<DecimalValue>,
    azure_annual_total: Option<DecimalValue>,
}

pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ProjectSummary>>, Problem> {
    let principal = require_principal(&headers, &state, COLLECTION_INSTANCE)?;
    let projects = state
        .projects
        .list(&principal.owner_id())
        .await
        .map_err(|error| map_repository_error(error, COLLECTION_INSTANCE))?;
    Ok(Json(
        projects.into_iter().map(ProjectSummary::from).collect(),
    ))
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<EditableProject>, JsonRejection>,
) -> Result<Response, Problem> {
    let principal = require_principal(&headers, &state, COLLECTION_INSTANCE)?;
    let project = request
        .map_err(|error| super::json_rejection(error, COLLECTION_INSTANCE))?
        .0;
    validate_project(&project, COLLECTION_INSTANCE)?;
    let revision = calculate_revision_if_priced(&state, &project, COLLECTION_INSTANCE).await?;
    let document = state
        .projects
        .create(&principal.owner_id(), project, revision)
        .await
        .map_err(|error| map_repository_error(error, COLLECTION_INSTANCE))?;
    project_response(StatusCode::CREATED, document)
}

pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, Problem> {
    let principal = require_principal(&headers, &state, ITEM_INSTANCE)?;
    let project_id = project_id(path)?;
    let document = state
        .projects
        .get(&principal.owner_id(), project_id)
        .await
        .map_err(|error| map_repository_error(error, ITEM_INSTANCE))?;
    project_response(StatusCode::OK, document)
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    request: Result<Json<EditableProject>, JsonRejection>,
) -> Result<Response, Problem> {
    let principal = require_principal(&headers, &state, ITEM_INSTANCE)?;
    let project_id = project_id(path)?;
    let if_match = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| Problem::precondition_required(ITEM_INSTANCE))?
        .to_str()
        .map_err(|_| Problem::precondition_failed(ITEM_INSTANCE, None))?
        .to_owned();
    let project = request
        .map_err(|error| super::json_rejection(error, ITEM_INSTANCE))?
        .0;
    validate_project(&project, ITEM_INSTANCE)?;
    let current = state
        .projects
        .get(&principal.owner_id(), project_id)
        .await
        .map_err(|error| map_repository_error(error, ITEM_INSTANCE))?;
    if current.etag != if_match {
        return Err(Problem::precondition_failed(
            ITEM_INSTANCE,
            Some(&current.etag),
        ));
    }
    let pricing_unchanged = pricing_inputs_unchanged(&current, &project)
        .map_err(|error| map_repository_error(error, ITEM_INSTANCE))?;
    let revision = if pricing_unchanged {
        None
    } else {
        calculate_revision_if_priced(&state, &project, ITEM_INSTANCE).await?
    };

    match state
        .projects
        .update(
            &principal.owner_id(),
            project_id,
            &if_match,
            project,
            revision,
        )
        .await
    {
        Ok(document) => project_response(StatusCode::OK, document),
        Err(RepositoryError::PreconditionFailed) => {
            let current = state
                .projects
                .get(&principal.owner_id(), project_id)
                .await
                .ok();
            Err(Problem::precondition_failed(
                ITEM_INSTANCE,
                current.as_ref().map(|document| document.etag.as_str()),
            ))
        }
        Err(error) => Err(map_repository_error(error, ITEM_INSTANCE)),
    }
}

pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<StatusCode, Problem> {
    let principal = require_principal(&headers, &state, ITEM_INSTANCE)?;
    let project_id = project_id(path)?;
    state
        .project_shares
        .revoke_project(&principal.owner_id(), project_id)
        .await
        .map_err(|_| Problem::internal(ITEM_INSTANCE))?;
    state
        .projects
        .delete(&principal.owner_id(), project_id)
        .await
        .map_err(|error| map_repository_error(error, ITEM_INSTANCE))?;
    Ok(StatusCode::NO_CONTENT)
}

fn project_id(path: Result<Path<Uuid>, PathRejection>) -> Result<Uuid, Problem> {
    path.map(|Path(project_id)| project_id)
        .map_err(|_| Problem::malformed_request(ITEM_INSTANCE))
}

fn validate_project(project: &EditableProject, instance: &str) -> Result<(), Problem> {
    let issues = project.validate();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(Problem::validation(instance, issues))
    }
}

async fn calculate_revision_if_priced(
    state: &AppState,
    project: &EditableProject,
    instance: &str,
) -> Result<Option<CalculationRevision>, Problem> {
    let source_snapshot_available = project.settings.project_type == ProjectType::OnPrem
        || project.aws_price_snapshot_id.is_some();
    if source_snapshot_available && project.azure_price_snapshot_id.is_some() {
        let _permit = state
            .calculation_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Problem::rate_limited(instance, 1))?;
        calculate_project(state, project, None).await.map(Some)
    } else {
        Ok(None)
    }
}

fn project_response(status: StatusCode, document: ProjectDocument) -> Result<Response, Problem> {
    let etag =
        HeaderValue::from_str(&document.etag).map_err(|_| Problem::internal(ITEM_INSTANCE))?;
    let mut response = (status, Json(ProjectResponse::from(document))).into_response();
    response.headers_mut().insert(header::ETAG, etag);
    Ok(response)
}

fn map_repository_error(error: RepositoryError, instance: &str) -> Problem {
    match error {
        RepositoryError::NotFound => {
            Problem::not_found(instance, "The requested project does not exist.")
        }
        RepositoryError::PreconditionFailed => Problem::precondition_failed(instance, None),
        RepositoryError::PayloadTooLarge => Problem::payload_too_large(instance),
        RepositoryError::Unavailable => Problem::internal(instance),
    }
}

impl From<ProjectDocument> for ProjectResponse {
    fn from(document: ProjectDocument) -> Self {
        Self {
            id: document.id,
            name: document.name,
            description: document.description,
            settings: document.settings,
            resources: document.resources,
            aws_price_snapshot_id: document.aws_price_snapshot_id,
            azure_price_snapshot_id: document.azure_price_snapshot_id,
            created_at: document.created_at,
            updated_at: document.updated_at,
            formula_version: document.formula_version,
            schema_version: document.schema_version,
            latest_calculation_revision: document.latest_calculation_revision,
        }
    }
}

impl From<ProjectDocument> for ProjectSummary {
    fn from(document: ProjectDocument) -> Self {
        let source_annual_total = document
            .latest_calculation_revision
            .as_ref()
            .and_then(|revision| revision.portfolio_totals.aws_all_rows_total);
        let azure_annual_total =
            document
                .latest_calculation_revision
                .as_ref()
                .and_then(|revision| {
                    (revision.portfolio_totals.comparable_resource_count > 0)
                        .then_some(revision.portfolio_totals.azure_mapped_rows_total)
                });

        Self {
            id: document.id,
            name: document.name,
            project_type: document.settings.project_type,
            modified_at: document.updated_at,
            source_region: document.settings.aws_region,
            azure_region: document.settings.azure_region,
            resource_count: document.resources.len(),
            source_annual_total,
            azure_annual_total,
        }
    }
}
