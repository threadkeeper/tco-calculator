pub mod assistant;
pub mod calculations;
pub mod calculator_launches;
pub mod catalog;
pub mod pricing;
pub mod privacy;
pub mod project_shares;
pub mod projects;
pub mod session;

use axum::{
    extract::rejection::JsonRejection,
    http::{HeaderMap, StatusCode},
};

use crate::{auth::Principal, problem::Problem, state::AppState};

pub(crate) fn require_principal(
    headers: &HeaderMap,
    state: &AppState,
    instance: &str,
) -> Result<Principal, Problem> {
    crate::auth::resolve_principal(headers, &state.config)
        .map_err(|_| Problem::unauthorized(instance, "A valid Microsoft identity is required."))?
        .ok_or_else(|| {
            Problem::unauthorized(instance, "Sign in with Microsoft to access saved projects.")
        })
}

pub(crate) fn json_rejection(rejection: JsonRejection, instance: &str) -> Problem {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        Problem::payload_too_large(instance)
    } else {
        Problem::malformed_request(instance)
    }
}
