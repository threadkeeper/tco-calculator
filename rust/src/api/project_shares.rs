use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection, rejection::PathRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    domain::project::{EditableProject, ProjectDocument},
    persistence::{
        project_share::{CreatedProjectShare, ProjectShareCredentials, ProjectShareError},
        repository::RepositoryError,
    },
    problem::Problem,
    state::AppState,
};

use super::require_principal;

const CREATE_INSTANCE: &str = "/api/v1/projects/{project_id}/shares";
const RESOLVE_INSTANCE: &str = "/api/v1/project-shares/resolve";
const REVOKE_INSTANCE: &str = "/api/v1/projects/{project_id}/shares/{share_id}";

#[derive(Serialize)]
struct CreatedProjectShareResponse {
    share_id: Uuid,
    secret: Uuid,
    expires_at: String,
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, Problem> {
    let principal = require_principal(&headers, &state, CREATE_INSTANCE)?;
    let project_id = path
        .map(|Path(project_id)| project_id)
        .map_err(|_| Problem::malformed_request(CREATE_INSTANCE))?;
    let owner_id = principal.owner_id();
    let source = state
        .projects
        .get(&owner_id, project_id)
        .await
        .map_err(|error| map_project_error(error, CREATE_INSTANCE))?;
    let created = state
        .project_shares
        .create(&owner_id, project_id, editable_project(source))
        .await
        .map_err(|error| map_share_error(error, CREATE_INSTANCE))?;
    if let Err(error) = state.projects.get(&owner_id, project_id).await {
        state
            .project_shares
            .revoke(&owner_id, project_id, created.credentials.share_id)
            .await
            .map_err(|cleanup_error| map_share_error(cleanup_error, CREATE_INSTANCE))?;
        return Err(map_project_error(error, CREATE_INSTANCE));
    }
    no_store_json(
        StatusCode::CREATED,
        CreatedProjectShareResponse::from(created),
    )
}

pub async fn resolve(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<ProjectShareCredentials>, JsonRejection>,
) -> Result<Response, Problem> {
    require_principal(&headers, &state, RESOLVE_INSTANCE)?;
    let credentials = request
        .map_err(|error| super::json_rejection(error, RESOLVE_INSTANCE))?
        .0;
    let project = state
        .project_shares
        .resolve(&credentials)
        .await
        .map_err(|error| map_share_error(error, RESOLVE_INSTANCE))?;
    no_store_json(StatusCode::OK, project)
}

pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<StatusCode, Problem> {
    let principal = require_principal(&headers, &state, REVOKE_INSTANCE)?;
    let (project_id, share_id) = path
        .map(|Path(ids)| ids)
        .map_err(|_| Problem::malformed_request(REVOKE_INSTANCE))?;
    state
        .project_shares
        .revoke(&principal.owner_id(), project_id, share_id)
        .await
        .map_err(|error| map_share_error(error, REVOKE_INSTANCE))?;
    Ok(StatusCode::NO_CONTENT)
}

fn editable_project(document: ProjectDocument) -> EditableProject {
    EditableProject {
        name: document.name,
        description: document.description,
        settings: document.settings,
        resources: document.resources,
        aws_price_snapshot_id: document.aws_price_snapshot_id,
        azure_price_snapshot_id: document.azure_price_snapshot_id,
    }
}

fn no_store_json<T: Serialize>(status: StatusCode, payload: T) -> Result<Response, Problem> {
    let mut response = (status, Json(payload)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store"
            .parse()
            .map_err(|_| Problem::internal(RESOLVE_INSTANCE))?,
    );
    Ok(response)
}

fn map_project_error(error: RepositoryError, instance: &str) -> Problem {
    match error {
        RepositoryError::NotFound => {
            Problem::not_found(instance, "The requested project does not exist.")
        }
        RepositoryError::PayloadTooLarge => Problem::payload_too_large(instance),
        RepositoryError::PreconditionFailed | RepositoryError::Unavailable => {
            Problem::internal(instance)
        }
    }
}

fn map_share_error(error: ProjectShareError, instance: &str) -> Problem {
    match error {
        ProjectShareError::NotFound => {
            Problem::not_found(instance, "The requested project share does not exist.")
        }
        ProjectShareError::Expired => Problem::gone(
            instance,
            "The project share has expired. Ask its owner for a new link.",
        ),
        ProjectShareError::PayloadTooLarge => Problem::payload_too_large(instance),
        ProjectShareError::Unavailable => Problem::internal(instance),
    }
}

impl From<CreatedProjectShare> for CreatedProjectShareResponse {
    fn from(created: CreatedProjectShare) -> Self {
        Self {
            share_id: created.credentials.share_id,
            secret: created.credentials.secret,
            expires_at: created.expires_at,
        }
    }
}
