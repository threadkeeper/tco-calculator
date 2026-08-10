use axum::Json;
use serde::Serialize;

use crate::config::{APP_VERSION, FORMULA_VERSION, SCHEMA_VERSION};

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    status: &'static str,
    persistence: &'static str,
    price_providers: &'static str,
}

#[derive(Serialize)]
pub struct VersionResponse {
    version: &'static str,
    formula_version: &'static str,
    schema_version: &'static str,
}

pub async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub async fn readyz() -> Json<ReadinessResponse> {
    Json(ReadinessResponse {
        status: "ready",
        persistence: "memory",
        price_providers: "stubbed",
    })
}

pub async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: APP_VERSION,
        formula_version: FORMULA_VERSION,
        schema_version: SCHEMA_VERSION,
    })
}
