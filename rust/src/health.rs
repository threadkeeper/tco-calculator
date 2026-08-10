use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;

use crate::{
    config::{APP_VERSION, AppEnvironment, FORMULA_VERSION, SCHEMA_VERSION},
    state::{AppState, PersistenceBackend},
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
    let persistence_ready = state.projects.check_health().await.is_ok();
    let providers_ready = local;
    let ready = persistence_ready && providers_ready;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadinessResponse {
            status: if ready { "ready" } else { "not_ready" },
            persistence: match (state.persistence_backend, persistence_ready) {
                (PersistenceBackend::MemoryLocal, true) => "memory_local_only",
                (PersistenceBackend::Cosmos, true) => "cosmos_ready",
                (_, false) => "unavailable",
            },
            price_providers: if providers_ready {
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
