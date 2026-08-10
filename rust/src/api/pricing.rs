use std::sync::Arc;

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{project::ValidationIssue, resource::Resource},
    pricing::{
        provider::{Provider, ResolutionStatus},
        snapshot::{AwsPriceSnapshot, AzurePriceSnapshot, SnapshotMetadata},
    },
    problem::Problem,
    state::AppState,
};

const AWS_INSTANCE: &str = "/api/v1/pricing/aws/resolve";
const AZURE_INSTANCE: &str = "/api/v1/pricing/azure/resolve";

#[derive(Serialize)]
pub struct PriceResolutionResponse {
    provider: Provider,
    status: ResolutionStatus,
    snapshot_id: Option<String>,
    retrieved_at: Option<String>,
    warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceResolutionRequest {
    currency: String,
    aws_region: Option<String>,
    azure_region: String,
    resources: Vec<Resource>,
}

pub async fn resolve_aws(
    State(state): State<AppState>,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    resolve_aws_request(state, request, false)
}

pub async fn refresh_aws(
    State(state): State<AppState>,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    resolve_aws_request(state, request, true)
}

pub async fn resolve_azure(
    State(state): State<AppState>,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    resolve_azure_request(state, request, false)
}

pub async fn refresh_azure(
    State(state): State<AppState>,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    resolve_azure_request(state, request, true)
}

fn resolve_aws_request(
    state: AppState,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
    refresh: bool,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    let request = parse_request(request, AWS_INSTANCE)?;
    let source_region = request.aws_region.as_deref().ok_or_else(|| {
        Problem::validation(
            AWS_INSTANCE,
            vec![ValidationIssue {
                pointer: "/aws_region".to_owned(),
                code: "required",
                message: "AWS region is required for AWS price resolution.".to_owned(),
            }],
        )
    })?;
    let snapshot = state
        .snapshots
        .find_aws(&request.currency, source_region)
        .map_err(|_| Problem::internal(AWS_INSTANCE))?;
    Ok(Json(match snapshot {
        Some(snapshot) => resolved(snapshot, refresh),
        None => unavailable(Provider::Aws),
    }))
}

fn resolve_azure_request(
    state: AppState,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
    refresh: bool,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    let request = parse_request(request, AZURE_INSTANCE)?;
    let snapshot = state
        .snapshots
        .find_azure(&request.currency, &request.azure_region)
        .map_err(|_| Problem::internal(AZURE_INSTANCE))?;
    Ok(Json(match snapshot {
        Some(snapshot) => resolved(snapshot, refresh),
        None => unavailable(Provider::Azure),
    }))
}

fn parse_request(
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
    instance: &str,
) -> Result<PriceResolutionRequest, Problem> {
    let request = request
        .map_err(|error| super::json_rejection(error, instance))?
        .0;
    let mut issues = Vec::new();
    if request.currency != "USD" {
        issues.push(ValidationIssue {
            pointer: "/currency".to_owned(),
            code: "unsupported",
            message: "Currency must be USD.".to_owned(),
        });
    }
    if request.azure_region.is_empty() {
        issues.push(ValidationIssue {
            pointer: "/azure_region".to_owned(),
            code: "required",
            message: "Azure region is required.".to_owned(),
        });
    }
    if request.resources.len() > 100 {
        issues.push(ValidationIssue {
            pointer: "/resources".to_owned(),
            code: "limit",
            message: "A price request may contain at most 100 resources.".to_owned(),
        });
    }
    if issues.is_empty() {
        Ok(request)
    } else {
        Err(Problem::validation(instance, issues))
    }
}

trait SnapshotWithMetadata {
    fn metadata(&self) -> &SnapshotMetadata;
}

impl SnapshotWithMetadata for AwsPriceSnapshot {
    fn metadata(&self) -> &SnapshotMetadata {
        &self.metadata
    }
}

impl SnapshotWithMetadata for AzurePriceSnapshot {
    fn metadata(&self) -> &SnapshotMetadata {
        &self.metadata
    }
}

fn resolved<T: SnapshotWithMetadata>(snapshot: Arc<T>, refresh: bool) -> PriceResolutionResponse {
    let metadata = snapshot.metadata();
    let mut warnings = metadata.warnings.clone();
    if refresh {
        warnings.push(
            "The immutable local fixture cannot be refreshed; the existing snapshot was returned."
                .to_owned(),
        );
    }
    warnings.sort();
    warnings.dedup();
    PriceResolutionResponse {
        provider: metadata.provider,
        status: metadata.status,
        snapshot_id: Some(metadata.snapshot_id.clone()),
        retrieved_at: Some(metadata.retrieved_at.clone()),
        warnings,
    }
}

fn unavailable(provider: Provider) -> PriceResolutionResponse {
    PriceResolutionResponse {
        provider,
        status: ResolutionStatus::Unavailable,
        snapshot_id: None,
        retrieved_at: None,
        warnings: vec![
            "No usable snapshot exists for this scope and live provider transport is not configured in this build."
                .to_owned(),
        ],
    }
}
