use axum::{
    Json,
    body::Body,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, Request, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{Principal, is_email_address},
    domain::project::ValidationIssue,
    persistence::privacy_consent::{
        CURRENT_PRIVACY_NOTICE_VERSION, PrivacyConsentError, PrivacyConsentProfile,
    },
    problem::Problem,
    state::AppState,
};

use super::require_principal;

const CONSENT_INSTANCE: &str = "/api/v1/privacy-consent";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePrivacyConsentRequest {
    notice_version: String,
    accepted: bool,
    allow_contact: bool,
    email_address: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PrivacyConsentStatus {
    notice_version: &'static str,
    required: bool,
    accepted_at: Option<String>,
    allow_contact: bool,
    email_address: Option<String>,
}

impl PrivacyConsentStatus {
    pub fn guest() -> Self {
        Self {
            notice_version: CURRENT_PRIVACY_NOTICE_VERSION,
            required: false,
            accepted_at: None,
            allow_contact: false,
            email_address: None,
        }
    }
}

pub async fn status(
    state: &AppState,
    principal: &Principal,
) -> Result<PrivacyConsentStatus, Problem> {
    let record = state
        .privacy_consents
        .get(&principal.owner_id())
        .await
        .map_err(|_| Problem::internal(CONSENT_INSTANCE))?;
    let required = record
        .as_ref()
        .is_none_or(|record| record.notice_version != CURRENT_PRIVACY_NOTICE_VERSION);
    Ok(PrivacyConsentStatus {
        notice_version: CURRENT_PRIVACY_NOTICE_VERSION,
        required,
        accepted_at: record.as_ref().map(|record| record.accepted_at.clone()),
        allow_contact: record.as_ref().is_some_and(|record| record.allow_contact),
        email_address: record.and_then(|record| record.email_address),
    })
}

pub async fn save(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<SavePrivacyConsentRequest>, JsonRejection>,
) -> Result<Response, Problem> {
    let principal = require_principal(&headers, &state, CONSENT_INSTANCE)?;
    let request = request
        .map_err(|error| super::json_rejection(error, CONSENT_INSTANCE))?
        .0;
    let email_address = validate_request(&request)?;
    let document = state
        .privacy_consents
        .save(
            &principal.owner_id(),
            PrivacyConsentProfile {
                display_name: principal.display_name,
                email_address,
                allow_contact: request.allow_contact,
            },
        )
        .await
        .map_err(map_consent_error)?;
    let mut response = Json(PrivacyConsentStatus {
        notice_version: CURRENT_PRIVACY_NOTICE_VERSION,
        required: false,
        accepted_at: Some(document.accepted_at),
        allow_contact: document.allow_contact,
        email_address: document.email_address,
    })
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header value"),
    );
    Ok(response)
}

fn validate_request(request: &SavePrivacyConsentRequest) -> Result<Option<String>, Problem> {
    let mut issues = Vec::new();
    if request.notice_version != CURRENT_PRIVACY_NOTICE_VERSION {
        issues.push(issue(
            "/notice_version",
            "current_version",
            "Review and accept the current privacy notice version.",
        ));
    }
    if !request.accepted {
        issues.push(issue(
            "/accepted",
            "required",
            "Privacy notice acceptance is required for signed-in use.",
        ));
    }
    let email_address = request
        .allow_contact
        .then(|| request.email_address.as_deref().map(str::trim))
        .flatten();
    if request.allow_contact && email_address.is_none_or(|email| !is_email_address(email)) {
        issues.push(issue(
            "/email_address",
            "email",
            "Enter one valid contact email address.",
        ));
    }
    if issues.is_empty() {
        Ok(email_address.map(str::to_owned))
    } else {
        Err(Problem::validation(CONSENT_INSTANCE, issues))
    }
}

fn issue(pointer: &str, code: &'static str, message: &str) -> ValidationIssue {
    ValidationIssue {
        pointer: pointer.to_owned(),
        code,
        message: message.to_owned(),
    }
}

fn map_consent_error(error: PrivacyConsentError) -> Problem {
    match error {
        PrivacyConsentError::InvalidRecord => Problem::malformed_request(CONSENT_INSTANCE),
        PrivacyConsentError::Unavailable => Problem::internal(CONSENT_INSTANCE),
    }
}

pub async fn enforce_accepted_consent(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match consent_required(&state, request.headers(), request.uri().path()).await {
        Ok(false) => next.run(request).await,
        Ok(true) => Problem::privacy_consent_required(request.uri().path()).into_response(),
        Err(problem) => problem.into_response(),
    }
}

async fn consent_required(
    state: &AppState,
    headers: &HeaderMap,
    instance: &str,
) -> Result<bool, Problem> {
    let principal = crate::auth::resolve_principal(headers, &state.config)
        .map_err(|_| Problem::unauthorized(instance, "A valid Microsoft identity is required."))?;
    match principal {
        Some(principal) => Ok(status(state, &principal).await?.required),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::{
        config::{AppEnvironment, Config, LocalAuthSettings},
        persistence::privacy_consent::{
            PRIVACY_CONSENT_DOCUMENT_ID, PRIVACY_CONSENT_DOCUMENT_TYPE, PrivacyConsentDocument,
            PrivacyConsentRepository,
        },
    };

    use super::*;

    #[test]
    fn contact_permission_requires_a_valid_email() {
        let request = SavePrivacyConsentRequest {
            notice_version: CURRENT_PRIVACY_NOTICE_VERSION.to_owned(),
            accepted: true,
            allow_contact: true,
            email_address: Some("person@example.com".to_owned()),
        };
        let Ok(email_address) = validate_request(&request) else {
            panic!("expected a valid request");
        };
        assert_eq!(email_address.as_deref(), Some("person@example.com"));

        let missing = SavePrivacyConsentRequest {
            email_address: None,
            ..request
        };
        assert!(validate_request(&missing).is_err());
    }

    #[test]
    fn contact_email_is_discarded_when_permission_is_off() {
        let request = SavePrivacyConsentRequest {
            notice_version: CURRENT_PRIVACY_NOTICE_VERSION.to_owned(),
            accepted: true,
            allow_contact: false,
            email_address: Some("discard@example.com".to_owned()),
        };

        let Ok(email_address) = validate_request(&request) else {
            panic!("expected a valid request");
        };
        assert_eq!(email_address, None);
    }

    #[tokio::test]
    async fn guest_requests_do_not_require_consent() {
        let state = AppState::in_memory(local_config(None)).expect("guest state");

        let Ok(required) = consent_required(&state, &HeaderMap::new(), "/test").await else {
            panic!("expected a consent decision");
        };
        assert!(!required);
    }

    #[tokio::test]
    async fn authenticated_requests_require_missing_consent() {
        let state = authenticated_state();

        let Ok(required) = consent_required(&state, &HeaderMap::new(), "/test").await else {
            panic!("expected a consent decision");
        };
        assert!(required);
    }

    #[tokio::test]
    async fn authenticated_requests_accept_current_consent() {
        let state = authenticated_state();
        let principal = crate::auth::resolve_principal(&HeaderMap::new(), &state.config)
            .expect("valid local identity")
            .expect("authenticated principal");
        state
            .privacy_consents
            .save(
                &principal.owner_id(),
                PrivacyConsentProfile {
                    display_name: principal.display_name,
                    email_address: None,
                    allow_contact: false,
                },
            )
            .await
            .expect("save consent");

        let Ok(required) = consent_required(&state, &HeaderMap::new(), "/test").await else {
            panic!("expected a consent decision");
        };
        assert!(!required);
    }

    #[tokio::test]
    async fn authenticated_requests_require_outdated_consent() {
        let mut state = authenticated_state();
        let principal = crate::auth::resolve_principal(&HeaderMap::new(), &state.config)
            .expect("valid local identity")
            .expect("authenticated principal");
        state.privacy_consents = Arc::new(StaticPrivacyConsentRepository {
            record: Some(PrivacyConsentDocument {
                id: PRIVACY_CONSENT_DOCUMENT_ID.to_owned(),
                document_type: PRIVACY_CONSENT_DOCUMENT_TYPE.to_owned(),
                owner_id: principal.owner_id(),
                notice_version: "superseded-notice".to_owned(),
                accepted_at: "2026-01-01T00:00:00Z".to_owned(),
                display_name: None,
                email_address: None,
                allow_contact: false,
            }),
        });

        let Ok(required) = consent_required(&state, &HeaderMap::new(), "/test").await else {
            panic!("expected a consent decision");
        };
        assert!(required);
    }

    fn authenticated_state() -> AppState {
        let local_auth = LocalAuthSettings {
            tenant_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                .expect("tenant UUID"),
            object_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222")
                .expect("object UUID"),
            display_name: "Synthetic User".to_owned(),
        };
        AppState::in_memory(local_config(Some(local_auth))).expect("authenticated state")
    }

    fn local_config(local_auth: Option<LocalAuthSettings>) -> Config {
        Config {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            environment: AppEnvironment::Local,
            local_auth,
            cosmos: None,
            assistant: None,
            web_asset_dir: PathBuf::from("rust/static"),
            guest_requests_per_minute: 60,
            provider_refreshes_per_hour: 8,
            provider_max_response_bytes: 64 * 1024 * 1024,
            calculation_concurrency: 10,
            assistant_requests_per_minute: 10,
        }
    }

    struct StaticPrivacyConsentRepository {
        record: Option<PrivacyConsentDocument>,
    }

    #[async_trait]
    impl PrivacyConsentRepository for StaticPrivacyConsentRepository {
        async fn get(
            &self,
            owner_id: &str,
        ) -> Result<Option<PrivacyConsentDocument>, PrivacyConsentError> {
            Ok(self
                .record
                .as_ref()
                .filter(|record| record.owner_id == owner_id)
                .cloned())
        }

        async fn save(
            &self,
            _owner_id: &str,
            _profile: PrivacyConsentProfile,
        ) -> Result<PrivacyConsentDocument, PrivacyConsentError> {
            Err(PrivacyConsentError::Unavailable)
        }
    }
}
