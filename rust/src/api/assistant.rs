//! Deterministic application help used directly by the UI and by the assistant tool.

use axum::{
    Json,
    extract::State,
    extract::rejection::JsonRejection,
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    assistant::{
        context::{SelectedProject, TurnContext, TurnPhase},
        help as help_catalog,
        model::ModelError,
        turn::{PROMPT_VERSION, TurnError, run_turn},
    },
    persistence::repository::RepositoryError,
    problem::Problem,
    request_context,
    state::AppState,
};

const HELP_INSTANCE: &str = "/api/v1/assistant/help";
const TURN_INSTANCE: &str = "/api/v1/assistant/turn";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelpRequest {
    question: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnRequest {
    question: String,
    project_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
struct TurnResponse {
    answer: String,
    references: Vec<help_catalog::HelpReference>,
}

/// Answer a bounded natural-language question from the reviewed help catalog.
pub async fn help(request: Result<Json<HelpRequest>, JsonRejection>) -> Result<Response, Problem> {
    let request = request.map_err(|error| super::json_rejection(error, HELP_INSTANCE))?;
    let help = help_catalog::answer_question(&request.question)
        .map_err(|issues| Problem::validation(HELP_INSTANCE, issues))?;
    let mut response = Json(help).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header value"),
    );
    Ok(response)
}

/// Run one bounded, read-only model turn for an authenticated owner.
pub async fn turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<TurnRequest>, JsonRejection>,
) -> Result<Response, Problem> {
    let principal = super::require_principal(&headers, &state, TURN_INSTANCE)?;
    let request = request
        .map_err(|error| super::json_rejection(error, TURN_INSTANCE))?
        .0;
    let owner_id = principal.owner_id();
    let request_id =
        Uuid::parse_str(&request_context::request_id()).unwrap_or_else(|_| Uuid::new_v4());
    let mut context = TurnContext::new(&owner_id, request_id, TurnPhase::ReadPlan);

    if let Some(project_id) = request.project_id {
        let project = state
            .projects
            .get(&owner_id, project_id)
            .await
            .map_err(map_project_error)?;
        context = context.with_project(SelectedProject {
            id: project.id,
            etag: project.etag,
            aws_price_snapshot_id: project.aws_price_snapshot_id,
            azure_price_snapshot_id: project.azure_price_snapshot_id,
        });
    }
    if !state.assistant_enabled {
        return Err(Problem::assistant_unavailable(TURN_INSTANCE));
    }
    match state.assistant_rate_limit.check(&owner_id) {
        Ok(Some(retry_after)) => {
            return Err(Problem::rate_limited(TURN_INSTANCE, retry_after));
        }
        Err(_) => return Err(Problem::internal(TURN_INSTANCE)),
        Ok(None) => {}
    }

    let _permit = state
        .assistant_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Problem::rate_limited(TURN_INSTANCE, 1))?;
    let outcome = run_turn(
        &state,
        state.assistant_model.as_ref(),
        &context,
        &request.question,
    )
    .await
    .map_err(map_turn_error)?;

    tracing::info!(
        request_id = %context.request_id(),
        prompt_version = PROMPT_VERSION,
        routed_model = outcome.routed_model.as_deref().unwrap_or("unknown"),
        model_requests = outcome.model_requests,
        tool_calls = outcome.tool_calls,
        "assistant turn completed"
    );
    let mut response = Json(TurnResponse {
        answer: outcome.answer,
        references: help_catalog::references_for_ids(&outcome.citations),
    })
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header value"),
    );
    Ok(response)
}

fn map_project_error(error: RepositoryError) -> Problem {
    match error {
        RepositoryError::NotFound => {
            Problem::not_found(TURN_INSTANCE, "The requested project does not exist.")
        }
        _ => Problem::assistant_unavailable(TURN_INSTANCE),
    }
}

fn map_turn_error(error: TurnError) -> Problem {
    match error {
        TurnError::Question(issues) => Problem::validation(TURN_INSTANCE, issues),
        TurnError::Model(ModelError::ContentFiltered) => Problem::assistant_rejected(TURN_INSTANCE),
        TurnError::Deadline | TurnError::Budget(_) | TurnError::Policy(_) | TurnError::Model(_) => {
            Problem::assistant_unavailable(TURN_INSTANCE)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use async_trait::async_trait;
    use axum::{body::to_bytes, extract::State, http::StatusCode, response::IntoResponse};

    use super::*;
    use crate::{
        assistant::model::{ModelClient, ModelTurnRequest, ModelTurnResponse},
        config::{AppEnvironment, Config, LocalAuthSettings},
        domain::{
            decimal::DecimalValue,
            project::{EditableProject, ProjectSettings},
            resource::{ProjectType, PurchaseOption},
        },
    };

    const TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";
    const OWNER_ID: &str = "22222222-2222-2222-2222-222222222222";

    struct StaticModelClient(Result<ModelTurnResponse, ModelError>);

    #[async_trait]
    impl ModelClient for StaticModelClient {
        async fn respond(
            &self,
            _request: ModelTurnRequest,
        ) -> Result<ModelTurnResponse, ModelError> {
            self.0.clone()
        }
    }

    #[tokio::test]
    async fn turn_requires_an_authenticated_principal() {
        let response = turn(
            State(state(false)),
            HeaderMap::new(),
            Ok(Json(request(None))),
        )
        .await
        .expect_err("anonymous turn must fail")
        .into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn turn_does_not_reveal_another_owners_project() {
        let mut state = state(true);
        enable_assistant(&mut state, Ok(message("unused")));
        let project = state
            .projects
            .create("entra:other:owner", project(), None)
            .await
            .expect("synthetic project");

        let response = turn(
            State(state),
            HeaderMap::new(),
            Ok(Json(request(Some(project.id)))),
        )
        .await
        .expect_err("cross-owner project must not resolve")
        .into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn turn_fails_closed_while_the_assistant_is_disabled() {
        let response = turn(
            State(state(true)),
            HeaderMap::new(),
            Ok(Json(request(None))),
        )
        .await
        .expect_err("disabled assistant must fail")
        .into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn turn_rejects_work_when_global_concurrency_is_exhausted() {
        let mut state = state(true);
        enable_assistant(&mut state, Ok(message("unused")));
        let _permit = state
            .assistant_slots
            .clone()
            .try_acquire_owned()
            .expect("reserve the only model slot");

        let response = turn(State(state), HeaderMap::new(), Ok(Json(request(None))))
            .await
            .expect_err("busy assistant must reject the turn")
            .into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
    }

    #[tokio::test]
    async fn turn_sanitizes_upstream_model_failures() {
        let mut state = state(true);
        enable_assistant(&mut state, Err(ModelError::Transport));

        let response = turn(State(state), HeaderMap::new(), Ok(Json(request(None))))
            .await
            .expect_err("transport failure must fail closed")
            .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), 16_384)
            .await
            .expect("bounded problem body");
        let body = String::from_utf8(body.to_vec()).expect("UTF-8 problem body");

        assert!(body.contains("assistant-unavailable"));
        assert!(!body.contains("Transport"));
        assert!(!body.contains("Foundry"));
    }

    fn state(authenticated: bool) -> AppState {
        AppState::in_memory(Config {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            environment: AppEnvironment::Local,
            local_auth: authenticated.then(|| LocalAuthSettings {
                tenant_id: Uuid::parse_str(TENANT_ID).expect("tenant UUID"),
                object_id: Uuid::parse_str(OWNER_ID).expect("owner UUID"),
                display_name: "Synthetic User".to_owned(),
            }),
            cosmos: None,
            assistant: None,
            web_asset_dir: PathBuf::from("rust/static"),
            guest_requests_per_minute: 60,
            provider_refreshes_per_hour: 8,
            provider_max_response_bytes: 64 * 1024 * 1024,
            calculation_concurrency: 10,
            assistant_requests_per_minute: 10,
        })
        .expect("in-memory application state")
    }

    fn enable_assistant(state: &mut AppState, response: Result<ModelTurnResponse, ModelError>) {
        state.assistant_enabled = true;
        state.assistant_model = Arc::new(StaticModelClient(response));
    }

    fn request(project_id: Option<Uuid>) -> TurnRequest {
        TurnRequest {
            question: "What does the Azure region control?".to_owned(),
            project_id,
        }
    }

    fn message(answer: &str) -> ModelTurnResponse {
        ModelTurnResponse {
            output: crate::assistant::model::ModelOutput::Message(answer.to_owned()),
            routed_model: Some("synthetic-model".to_owned()),
        }
    }

    fn project() -> EditableProject {
        EditableProject {
            name: "Synthetic project".to_owned(),
            description: None,
            settings: ProjectSettings {
                project_type: ProjectType::Ec2,
                aws_region: Some("eu-west-1".to_owned()),
                azure_region: "swedencentral".to_owned(),
                currency: "USD".to_owned(),
                source_compute_discount: DecimalValue::ZERO,
                source_license_discount: DecimalValue::ZERO,
                source_storage_discount: DecimalValue::ZERO,
                azure_compute_discount: DecimalValue::ZERO,
                azure_license_discount: DecimalValue::ZERO,
                azure_storage_discount: DecimalValue::ZERO,
                selected_parity_adjustment: DecimalValue::ZERO,
                default_annual_hours: DecimalValue::ZERO,
                default_mi_purchase_option: PurchaseOption::Payg,
                enterprise_license_sa_usd_per_two_core_pack: None,
                standard_license_sa_usd_per_two_core_pack: None,
                remaining_coverage_months: None,
                electricity_rate_usd_per_kwh: None,
            },
            resources: Vec::new(),
            aws_price_snapshot_id: None,
            azure_price_snapshot_id: None,
        }
    }
}
