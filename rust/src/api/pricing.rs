use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{project::ValidationIssue, resource::Resource},
    pricing::{
        coordinator::SnapshotResolution,
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
    resolve_aws_request(state, request, false).await
}

pub async fn refresh_aws(
    State(state): State<AppState>,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    resolve_aws_request(state, request, true).await
}

pub async fn resolve_azure(
    State(state): State<AppState>,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    resolve_azure_request(state, request, false).await
}

pub async fn refresh_azure(
    State(state): State<AppState>,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    resolve_azure_request(state, request, true).await
}

async fn resolve_aws_request(
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
    let resolution = if refresh {
        state
            .pricing
            .refresh_aws(&request.currency, source_region, &request.azure_region)
            .await
    } else {
        state
            .pricing
            .resolve_aws(&request.currency, source_region)
            .await
    }
    .map_err(|_| Problem::internal(AWS_INSTANCE))?;
    Ok(Json(response(resolution, Provider::Aws)))
}

async fn resolve_azure_request(
    state: AppState,
    request: Result<Json<PriceResolutionRequest>, JsonRejection>,
    refresh: bool,
) -> Result<Json<PriceResolutionResponse>, Problem> {
    let request = parse_request(request, AZURE_INSTANCE)?;
    let resolution = if refresh {
        state
            .pricing
            .refresh_azure(
                &request.currency,
                request.aws_region.as_deref(),
                &request.azure_region,
            )
            .await
    } else {
        state
            .pricing
            .resolve_azure(&request.currency, &request.azure_region)
            .await
    }
    .map_err(|_| Problem::internal(AZURE_INSTANCE))?;
    Ok(Json(response(resolution, Provider::Azure)))
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

fn response<T: SnapshotWithMetadata>(
    resolution: SnapshotResolution<T>,
    provider: Provider,
) -> PriceResolutionResponse {
    let Some(snapshot) = resolution.snapshot else {
        return unavailable(provider, resolution.warnings);
    };
    let metadata = snapshot.metadata();
    let mut warnings = metadata.warnings.clone();
    warnings.extend(resolution.warnings);
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

fn unavailable(provider: Provider, warnings: Vec<String>) -> PriceResolutionResponse {
    PriceResolutionResponse {
        provider,
        status: ResolutionStatus::Unavailable,
        snapshot_id: None,
        retrieved_at: None,
        warnings,
    }
}
