use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct PriceResolutionResponse {
    provider: &'static str,
    status: &'static str,
    snapshot_id: Option<&'static str>,
    warnings: [&'static str; 1],
}

pub async fn resolve_aws() -> Json<PriceResolutionResponse> {
    unavailable("aws")
}

pub async fn refresh_aws() -> Json<PriceResolutionResponse> {
    unavailable("aws")
}

pub async fn resolve_azure() -> Json<PriceResolutionResponse> {
    unavailable("azure")
}

pub async fn refresh_azure() -> Json<PriceResolutionResponse> {
    unavailable("azure")
}

fn unavailable(provider: &'static str) -> Json<PriceResolutionResponse> {
    Json(PriceResolutionResponse {
        provider,
        status: "unavailable",
        snapshot_id: None,
        warnings: ["Live price resolution is not implemented in pass 1."],
    })
}
