use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{
    api::privacy::{self, PrivacyConsentStatus},
    auth::resolve_principal,
    problem::Problem,
    state::AppState,
};

#[derive(Serialize)]
pub struct SessionResponse {
    mode: &'static str,
    display_name: Option<String>,
    email_address: Option<String>,
    privacy_consent: PrivacyConsentStatus,
}

pub async fn get_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let principal = resolve_principal(&headers, &state.config)
        .map_err(|_| Problem::unauthorized("/api/v1/session", "The identity header is invalid."))?;

    let session = match principal {
        Some(principal) => {
            let privacy_consent = privacy::status(&state, &principal).await?;
            SessionResponse {
                mode: "authenticated",
                display_name: principal.display_name,
                email_address: principal.email_address,
                privacy_consent,
            }
        }
        None => SessionResponse {
            mode: "guest",
            display_name: None,
            email_address: None,
            privacy_consent: PrivacyConsentStatus::guest(),
        },
    };
    let mut response = Json(session).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header value"),
    );
    Ok(response)
}
