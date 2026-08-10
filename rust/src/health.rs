use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;

use crate::{
    config::{APP_VERSION, AppEnvironment, FORMULA_VERSION, SCHEMA_VERSION},
    state::AppState,
};

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

pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let local = state.config.environment == AppEnvironment::Local;
    (
        if local {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadinessResponse {
            status: if local { "ready" } else { "not_ready" },
            persistence: if local {
                "memory_local_only"
            } else {
                "not_configured"
            },
            price_providers: if local {
                "frozen_local_fixture"
            } else {
                "not_configured"
            },
        }),
    )
}

pub async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: APP_VERSION,
        formula_version: FORMULA_VERSION,
        schema_version: SCHEMA_VERSION,
    })
}
