use axum::{Json, extract::State, http::HeaderMap};
use serde::Serialize;

use crate::{auth::resolve_principal, problem::Problem, state::AppState};

#[derive(Serialize)]
pub struct SessionResponse {
    mode: &'static str,
    display_name: Option<String>,
}

pub async fn get_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SessionResponse>, Problem> {
    let principal = resolve_principal(&headers, &state.config)
        .map_err(|_| Problem::unauthorized("/api/v1/session", "The identity header is invalid."))?;

    Ok(Json(match principal {
        Some(principal) => SessionResponse {
            mode: "authenticated",
            display_name: principal.display_name,
        },
        None => SessionResponse {
            mode: "guest",
            display_name: None,
        },
    }))
}
