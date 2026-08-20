//! Opt-in live evaluation of the synthetic image-classification corpus.

use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use azure_core::credentials::TokenCredential;
use azure_identity::{AzureCliCredential, ManagedIdentityCredential};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    api::assistant::{IMAGE_CLASSIFICATION_UNCERTAIN_ANSWER, IMAGE_DRAFT_EXTRACTION_REQUEST},
    config::{AppEnvironment, Config, FOUNDRY_API_VERSION},
    pricing::snapshot::utc_now_rfc3339,
    state::AppState,
};

use super::{
    classification::{
        CLASSIFICATION_PROMPT_VERSION, ClassificationConfidence, ClassifiedProjectType,
        ImageProjectClassification, classify_project_image,
    },
    context::{TurnContext, TurnPhase},
    foundry::FoundryModelClient,
    image::{ImageMediaType, normalize_image},
    model::{ModelError, ModelImage},
    policy::PolicyError,
    tools::AssistantProposal,
    turn::{PROMPT_VERSION, TurnError, run_turn_with_image},
};

const ACKNOWLEDGEMENT_VARIABLE: &str = "TCO_LIVE_FOUNDRY_EVALUATION";
const ACKNOWLEDGEMENT_VALUE: &str = "SYNTHETIC_FIXTURES_ONLY";
const IDENTITY_VARIABLE: &str = "TCO_LIVE_FOUNDRY_IDENTITY";
const AZURE_CLI_USER_IDENTITY: &str = "azure_cli_user";
const SYSTEM_ASSIGNED_MANAGED_IDENTITY: &str = "system_assigned_managed_identity";
const FIXTURE_MANIFEST: &str = "cases.json";
const EXPECTED_CASES: usize = 12;
const EXPECTED_CASES_PER_FAMILY: usize = 3;

#[derive(Debug, Error)]
pub enum EvaluationError {
    #[error("live evaluation requires the explicit synthetic-egress acknowledgement")]
    MissingAcknowledgement,
    #[error("Azure CLI must be signed in with an interactive user account")]
    InteractiveIdentityRequired,
    #[error("a system-assigned managed identity token is required")]
    SystemAssignedManagedIdentityRequired,
    #[error("Foundry evaluation configuration is missing or invalid")]
    InvalidConfiguration,
    #[error("the synthetic fixture corpus is missing or invalid")]
    InvalidFixtures,
    #[error("one or more fixture evaluations failed; inspect the local result files")]
    CaseFailures,
}

#[derive(Debug, Deserialize)]
struct FixtureCase {
    id: String,
    family: String,
    expected: ExpectedCase,
}

#[derive(Debug, Deserialize)]
struct ExpectedCase {
    case_id: String,
    project_type: ClassifiedProjectType,
    minimum_confidence: ClassificationConfidence,
    draft_assertions: Vec<DraftAssertion>,
}

#[derive(Debug, Deserialize)]
struct DraftAssertion {
    pointer: String,
    equals: Value,
}

#[derive(Debug, Serialize)]
struct AssertionResult {
    assertion: String,
    passed: bool,
    expected: Value,
    actual: Value,
}

struct CompletedCase {
    classification: ImageProjectClassification,
    response: Value,
    assertions: Vec<AssertionResult>,
    passed: bool,
}

struct FailedCase {
    code: &'static str,
    classification: Option<ImageProjectClassification>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EvaluationIdentity {
    AzureCliUser,
    SystemAssignedManagedIdentity,
}

impl EvaluationIdentity {
    fn as_str(self) -> &'static str {
        match self {
            Self::AzureCliUser => AZURE_CLI_USER_IDENTITY,
            Self::SystemAssignedManagedIdentity => SYSTEM_ASSIGNED_MANAGED_IDENTITY,
        }
    }

    fn unavailable_error(self) -> EvaluationError {
        match self {
            Self::AzureCliUser => EvaluationError::InteractiveIdentityRequired,
            Self::SystemAssignedManagedIdentity => {
                EvaluationError::SystemAssignedManagedIdentityRequired
            }
        }
    }
}

/// Run every committed synthetic screenshot through the live classifier and draft loop.
pub async fn run() -> Result<(), EvaluationError> {
    require_acknowledgement()?;
    let identity = evaluation_identity(env::var(IDENTITY_VARIABLE).ok().as_deref())?;

    let endpoint =
        env::var("FOUNDRY_ENDPOINT").map_err(|_| EvaluationError::InvalidConfiguration)?;
    let deployment =
        env::var("FOUNDRY_MODEL_DEPLOYMENT").map_err(|_| EvaluationError::InvalidConfiguration)?;
    let endpoint = Url::parse(&endpoint).map_err(|_| EvaluationError::InvalidConfiguration)?;
    let credential = evaluation_credential(identity)?;
    verify_credential_token(credential.as_ref(), identity).await?;
    let client = FoundryModelClient::new_with_credential(
        endpoint,
        &deployment,
        FOUNDRY_API_VERSION,
        credential,
    )
    .map_err(|_| EvaluationError::InvalidConfiguration)?;
    let state = AppState::in_memory(evaluation_config())
        .map_err(|_| EvaluationError::InvalidConfiguration)?;
    let fixture_root = fixture_root()?;
    let cases = load_cases(&fixture_root)?;

    let mut failures = 0usize;
    for fixture in cases {
        let result = evaluate_case(&state, &client, &fixture_root, &fixture).await;
        let passed = match &result {
            Ok(completed) => completed.passed,
            Err(_) => false,
        };
        if !passed {
            failures += 1;
        }
        write_result(&fixture_root, &fixture, identity, result)?;
        println!(
            "{}: {}",
            fixture.id,
            if passed { "passed" } else { "failed" }
        );
    }

    if failures == 0 {
        Ok(())
    } else {
        Err(EvaluationError::CaseFailures)
    }
}

async fn verify_credential_token(
    credential: &dyn TokenCredential,
    identity: EvaluationIdentity,
) -> Result<(), EvaluationError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        credential.get_token(&[super::foundry::FOUNDRY_TOKEN_SCOPE], None),
    )
    .await
    .map_err(|_| identity.unavailable_error())?
    .map(|_| ())
    .map_err(|_| identity.unavailable_error())
}

fn require_acknowledgement() -> Result<(), EvaluationError> {
    if env::var(ACKNOWLEDGEMENT_VARIABLE).as_deref() == Ok(ACKNOWLEDGEMENT_VALUE) {
        Ok(())
    } else {
        Err(EvaluationError::MissingAcknowledgement)
    }
}

fn evaluation_identity(value: Option<&str>) -> Result<EvaluationIdentity, EvaluationError> {
    match value {
        None | Some(AZURE_CLI_USER_IDENTITY) => Ok(EvaluationIdentity::AzureCliUser),
        Some(SYSTEM_ASSIGNED_MANAGED_IDENTITY) => {
            Ok(EvaluationIdentity::SystemAssignedManagedIdentity)
        }
        Some(_) => Err(EvaluationError::InvalidConfiguration),
    }
}

fn evaluation_credential(
    identity: EvaluationIdentity,
) -> Result<Arc<dyn TokenCredential>, EvaluationError> {
    match identity {
        EvaluationIdentity::AzureCliUser => {
            verify_interactive_azure_cli_identity()?;
            AzureCliCredential::new(None)
                .map(|credential| credential as Arc<dyn TokenCredential>)
                .map_err(|_| identity.unavailable_error())
        }
        EvaluationIdentity::SystemAssignedManagedIdentity => ManagedIdentityCredential::new(None)
            .map(|credential| credential as Arc<dyn TokenCredential>)
            .map_err(|_| identity.unavailable_error()),
    }
}

fn verify_interactive_azure_cli_identity() -> Result<(), EvaluationError> {
    let executable = if cfg!(windows) { "az.cmd" } else { "az" };
    let output = Command::new(executable)
        .args([
            "account",
            "show",
            "--query",
            "user.type",
            "--output",
            "tsv",
            "--only-show-errors",
        ])
        .output()
        .map_err(|_| EvaluationError::InteractiveIdentityRequired)?;
    if output.status.success() && interactive_user_type(&output.stdout) {
        Ok(())
    } else {
        Err(EvaluationError::InteractiveIdentityRequired)
    }
}

fn interactive_user_type(output: &[u8]) -> bool {
    std::str::from_utf8(output).is_ok_and(|value| value.trim().eq_ignore_ascii_case("user"))
}

fn evaluation_config() -> Config {
    Config {
        bind_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        environment: AppEnvironment::Local,
        local_auth: None,
        cosmos: None,
        assistant: None,
        web_asset_dir: PathBuf::from("rust/static"),
        guest_requests_per_minute: 60,
        provider_refreshes_per_hour: 40,
        provider_max_response_bytes: 8 * 1024 * 1024,
        calculation_concurrency: 1,
        assistant_requests_per_minute: 10,
    }
}

fn fixture_root() -> Result<PathBuf, EvaluationError> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() > 1 {
        return Err(EvaluationError::InvalidFixtures);
    }
    let root = arguments.into_iter().next().map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/assistant-workload-classification"),
        PathBuf::from,
    );
    root.canonicalize()
        .map_err(|_| EvaluationError::InvalidFixtures)
}

fn load_cases(root: &Path) -> Result<Vec<FixtureCase>, EvaluationError> {
    let manifest = fs::read_to_string(root.join(FIXTURE_MANIFEST))
        .map_err(|_| EvaluationError::InvalidFixtures)?;
    let cases: Vec<FixtureCase> =
        serde_json::from_str(&manifest).map_err(|_| EvaluationError::InvalidFixtures)?;
    validate_cases(&cases)?;
    Ok(cases)
}

fn validate_cases(cases: &[FixtureCase]) -> Result<(), EvaluationError> {
    if cases.len() != EXPECTED_CASES {
        return Err(EvaluationError::InvalidFixtures);
    }
    let mut identifiers = HashSet::new();
    let mut family_counts = BTreeMap::<&str, usize>::new();
    for fixture in cases {
        if fixture.expected.case_id != fixture.id
            || !valid_case_id(&fixture.id, &fixture.family)
            || fixture.family != fixture.expected.project_type.as_str()
            || fixture.expected.project_type == ClassifiedProjectType::Unknown
            || !identifiers.insert(fixture.id.as_str())
            || fixture.expected.draft_assertions.is_empty()
            || fixture.expected.draft_assertions.iter().any(|assertion| {
                !assertion.pointer.starts_with('/') || assertion.pointer.chars().count() > 256
            })
        {
            return Err(EvaluationError::InvalidFixtures);
        }
        *family_counts.entry(&fixture.family).or_default() += 1;
    }
    if family_counts.len() != 4
        || family_counts
            .values()
            .any(|count| *count != EXPECTED_CASES_PER_FAMILY)
    {
        return Err(EvaluationError::InvalidFixtures);
    }
    Ok(())
}

fn valid_case_id(id: &str, family: &str) -> bool {
    let Some((directory, name)) = id.split_once('/') else {
        return false;
    };
    directory == family
        && !name.is_empty()
        && name.len() <= 64
        && !name.contains('/')
        && name.bytes().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == b'-'
        })
}

async fn evaluate_case(
    state: &AppState,
    client: &FoundryModelClient,
    fixture_root: &Path,
    fixture: &FixtureCase,
) -> Result<CompletedCase, FailedCase> {
    let image_path = fixture_root.join(&fixture.id).join("input.png");
    let bytes = fs::read(image_path).map_err(|_| FailedCase {
        code: "fixture_read",
        classification: None,
    })?;
    let normalized = normalize_image(ImageMediaType::Png, &bytes).map_err(|_| FailedCase {
        code: "image_normalization",
        classification: None,
    })?;
    let model_image = ModelImage::normalized_jpeg(normalized.bytes);
    let context = TurnContext::new("evaluation:synthetic", Uuid::new_v4(), TurnPhase::Propose);
    let classification_outcome = classify_project_image(client, &context, model_image.clone())
        .await
        .map_err(|error| FailedCase {
            code: model_error_code(error),
            classification: None,
        })?;
    let classifier_model_requests = classification_outcome.model_requests;
    let classification = classification_outcome.classification;

    let Some(project_type) = classification.resolved_project_type() else {
        let uncertainties = if classification.ambiguities.is_empty() {
            vec!["The image did not contain a decisive supported project-type marker.".to_owned()]
        } else {
            classification.ambiguities.clone()
        };
        let response = json!({
            "answer": IMAGE_CLASSIFICATION_UNCERTAIN_ANSWER,
            "classification": classification,
            "proposal": Value::Null,
            "omissions": [],
            "uncertainties": uncertainties,
        });
        return Ok(CompletedCase {
            classification: classification.clone(),
            response,
            assertions: classification_assertions(&fixture.expected, &classification),
            passed: false,
        });
    };

    let context = context.with_classification_usage(project_type, classifier_model_requests);
    let outcome = run_turn_with_image(
        state,
        client,
        &context,
        IMAGE_DRAFT_EXTRACTION_REQUEST,
        Some(model_image),
    )
    .await
    .map_err(|error| FailedCase {
        code: turn_error_code(&error),
        classification: Some(classification.clone()),
    })?;

    let mut assertions = classification_assertions(&fixture.expected, &classification);
    let proposal = match outcome.proposal {
        Some(AssistantProposal::NewProjectDraft(proposal)) => {
            let mut project = serde_json::to_value(proposal.project).map_err(|_| FailedCase {
                code: "result_serialization",
                classification: Some(classification.clone()),
            })?;
            scrub_generated_uuids(&mut project, &mut 0);
            assertions.extend(draft_assertions(&fixture.expected, &project));
            json!({
                "proposal_id": "<generated-proposal-id>",
                "action": proposal.action,
                "project": project,
            })
        }
        Some(AssistantProposal::ProjectPatch(_)) => {
            return Err(FailedCase {
                code: "unexpected_patch_proposal",
                classification: Some(classification),
            });
        }
        None => {
            assertions.extend(missing_draft_assertions(&fixture.expected));
            Value::Null
        }
    };
    let passed = assertions.iter().all(|assertion| assertion.passed) && !proposal.is_null();
    let response = json!({
        "answer": outcome.answer,
        "classification": classification,
        "proposal": proposal,
        "omissions": outcome.omissions,
        "uncertainties": outcome.uncertainties,
    });

    Ok(CompletedCase {
        classification,
        response,
        assertions,
        passed,
    })
}

fn classification_assertions(
    expected: &ExpectedCase,
    classification: &ImageProjectClassification,
) -> Vec<AssertionResult> {
    vec![
        AssertionResult {
            assertion: "classification.project_type".to_owned(),
            passed: classification.project_type == expected.project_type,
            expected: json!(expected.project_type.as_str()),
            actual: json!(classification.project_type.as_str()),
        },
        AssertionResult {
            assertion: "classification.minimum_confidence".to_owned(),
            passed: confidence_rank(classification.confidence)
                >= confidence_rank(expected.minimum_confidence),
            expected: json!(expected.minimum_confidence.as_str()),
            actual: json!(classification.confidence.as_str()),
        },
    ]
}

fn confidence_rank(confidence: ClassificationConfidence) -> u8 {
    match confidence {
        ClassificationConfidence::Low => 0,
        ClassificationConfidence::Medium => 1,
        ClassificationConfidence::High => 2,
    }
}

fn draft_assertions(expected: &ExpectedCase, project: &Value) -> Vec<AssertionResult> {
    expected
        .draft_assertions
        .iter()
        .map(|assertion| {
            let actual = project
                .pointer(&assertion.pointer)
                .cloned()
                .unwrap_or(Value::Null);
            AssertionResult {
                assertion: assertion.pointer.clone(),
                passed: actual == assertion.equals,
                expected: assertion.equals.clone(),
                actual,
            }
        })
        .collect()
}

fn missing_draft_assertions(expected: &ExpectedCase) -> Vec<AssertionResult> {
    expected
        .draft_assertions
        .iter()
        .map(|assertion| AssertionResult {
            assertion: assertion.pointer.clone(),
            passed: false,
            expected: assertion.equals.clone(),
            actual: Value::Null,
        })
        .collect()
}

fn scrub_generated_uuids(value: &mut Value, next_id: &mut usize) {
    match value {
        Value::String(text) if Uuid::parse_str(text).is_ok() => {
            *next_id += 1;
            *text = format!("<generated-uuid-{next_id}>");
        }
        Value::Array(values) => {
            for value in values {
                scrub_generated_uuids(value, next_id);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                scrub_generated_uuids(value, next_id);
            }
        }
        _ => {}
    }
}

fn write_result(
    fixture_root: &Path,
    fixture: &FixtureCase,
    identity: EvaluationIdentity,
    result: Result<CompletedCase, FailedCase>,
) -> Result<(), EvaluationError> {
    let evaluated_at = utc_now_rfc3339().map_err(|_| EvaluationError::InvalidConfiguration)?;
    let (status, observed_family, observed_confidence, assertions, response) = match result {
        Ok(completed) => {
            let observed_family = completed.classification.project_type.as_str().to_owned();
            let observed_confidence = completed.classification.confidence.as_str().to_owned();
            (
                if completed.passed { "passed" } else { "failed" },
                observed_family,
                observed_confidence,
                serde_json::to_value(completed.assertions)
                    .map_err(|_| EvaluationError::InvalidFixtures)?,
                completed.response,
            )
        }
        Err(failure) => {
            let observed_family = failure
                .classification
                .as_ref()
                .map_or("not_available", |value| value.project_type.as_str())
                .to_owned();
            let observed_confidence = failure
                .classification
                .as_ref()
                .map_or("not_available", |value| value.confidence.as_str())
                .to_owned();
            (
                "failed",
                observed_family,
                observed_confidence,
                json!([]),
                json!({
                    "error": { "code": failure.code },
                    "classification": failure.classification,
                }),
            )
        }
    };
    let assertions =
        serde_json::to_string_pretty(&assertions).map_err(|_| EvaluationError::InvalidFixtures)?;
    let response =
        serde_json::to_string_pretty(&response).map_err(|_| EvaluationError::InvalidFixtures)?;
    let markdown = format!(
        "# Live Foundry Evaluation Result\n\n\
         - Status: `{status}`\n\
         - Case: `{case_id}`\n\
         - Evaluated UTC: `{evaluated_at}`\n\
         - Evaluation identity: `{evaluation_identity}`\n\
         - Classifier prompt: `{CLASSIFICATION_PROMPT_VERSION}`\n\
         - Draft prompt: `{PROMPT_VERSION}`\n\
         - Expected family: `{expected_family}`\n\
         - Expected minimum confidence: `{expected_confidence}`\n\
         - Observed family: `{observed_family}`\n\
         - Observed confidence: `{observed_confidence}`\n\n\
         ## Assertions\n\n```json\n{assertions}\n```\n\n\
         ## Complete Sanitized Response\n\n```json\n{response}\n```\n",
        case_id = fixture.id,
        evaluation_identity = identity.as_str(),
        expected_family = fixture.expected.project_type.as_str(),
        expected_confidence = fixture.expected.minimum_confidence.as_str(),
    );
    let result_path = fixture_root.join(&fixture.id).join("result.md");
    let temporary_path = fixture_root.join(&fixture.id).join("result.md.tmp");
    fs::write(&temporary_path, markdown).map_err(|_| EvaluationError::InvalidFixtures)?;
    fs::rename(temporary_path, result_path).map_err(|_| EvaluationError::InvalidFixtures)
}

fn model_error_code(error: ModelError) -> &'static str {
    match error {
        ModelError::Unavailable => "model_unavailable",
        ModelError::Timeout => "model_timeout",
        ModelError::Transport => "model_transport",
        ModelError::MalformedResponse => "model_malformed_response",
        ModelError::ContentFiltered => "model_content_filtered",
        ModelError::QuotaExceeded => "model_quota_exceeded",
    }
}

fn turn_error_code(error: &TurnError) -> &'static str {
    match error {
        TurnError::Question(_) => "turn_question_rejected",
        TurnError::Deadline => "turn_deadline",
        TurnError::Budget(_) => "turn_budget",
        TurnError::Policy(error) => policy_error_code(*error),
        TurnError::Model(error) => model_error_code(*error),
    }
}

fn policy_error_code(error: PolicyError) -> &'static str {
    match error {
        PolicyError::EmptyBatch => "policy_empty_batch",
        PolicyError::UngroundedResponse => "policy_ungrounded_response",
        PolicyError::BatchTooLarge => "policy_batch_too_large",
        PolicyError::BudgetExhausted => "policy_budget_exhausted",
        PolicyError::InvalidCallId => "policy_invalid_call_id",
        PolicyError::UnknownTool => "policy_unknown_tool",
        PolicyError::PhaseNotAllowed => "policy_phase_not_allowed",
        PolicyError::ProjectContextNotAllowed => "policy_project_context_not_allowed",
        PolicyError::TooManyMutations => "policy_too_many_mutations",
        PolicyError::InvalidArguments(error) => invalid_tool_arguments_code(error),
        PolicyError::ClassificationMismatch => "policy_classification_mismatch",
        PolicyError::MissingConfirmation => "policy_missing_confirmation",
    }
}

fn invalid_tool_arguments_code(error: super::tools::InvalidToolArguments) -> &'static str {
    match error {
        super::tools::InvalidToolArguments::MalformedJson => "policy_arguments_malformed_json",
        super::tools::InvalidToolArguments::UnknownField => "policy_arguments_unknown_field",
        super::tools::InvalidToolArguments::MissingField => "policy_arguments_missing_field",
        super::tools::InvalidToolArguments::TypeMismatch => "policy_arguments_type_mismatch",
        super::tools::InvalidToolArguments::UnknownVariant => "policy_arguments_unknown_variant",
        super::tools::InvalidToolArguments::InvalidScalarValue => {
            "policy_arguments_invalid_scalar_value"
        }
        super::tools::InvalidToolArguments::InvalidShape => "policy_arguments_invalid_shape",
        super::tools::InvalidToolArguments::InputBounds => "policy_arguments_input_bounds",
        super::tools::InvalidToolArguments::ExtractionNotes => "policy_arguments_extraction_notes",
        super::tools::InvalidToolArguments::ResourceFieldMismatch => {
            "policy_arguments_resource_field_mismatch"
        }
        super::tools::InvalidToolArguments::UnregisteredTool => {
            "policy_arguments_unregistered_tool"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_interactive_user_type_is_accepted() {
        assert!(interactive_user_type(b"user\r\n"));
        assert!(interactive_user_type(b"USER\n"));
        assert!(!interactive_user_type(b"servicePrincipal\n"));
        assert!(!interactive_user_type(b""));
    }

    #[test]
    fn evaluator_identity_requires_an_explicit_supported_mode() {
        assert_eq!(
            evaluation_identity(None).expect("default identity"),
            EvaluationIdentity::AzureCliUser
        );
        assert_eq!(
            evaluation_identity(Some(AZURE_CLI_USER_IDENTITY)).expect("CLI identity"),
            EvaluationIdentity::AzureCliUser
        );
        assert_eq!(
            evaluation_identity(Some(SYSTEM_ASSIGNED_MANAGED_IDENTITY)).expect("managed identity"),
            EvaluationIdentity::SystemAssignedManagedIdentity
        );
        assert!(evaluation_identity(Some("service_principal")).is_err());
    }

    #[test]
    fn policy_failures_keep_a_sanitized_discriminating_code() {
        assert_eq!(
            turn_error_code(&TurnError::Policy(PolicyError::InvalidArguments(
                super::super::tools::InvalidToolArguments::UnknownField
            ))),
            "policy_arguments_unknown_field"
        );
        assert_eq!(
            turn_error_code(&TurnError::Policy(PolicyError::InvalidArguments(
                super::super::tools::InvalidToolArguments::ResourceFieldMismatch
            ))),
            "policy_arguments_resource_field_mismatch"
        );
        assert_eq!(
            turn_error_code(&TurnError::Policy(PolicyError::ClassificationMismatch)),
            "policy_classification_mismatch"
        );
    }

    #[test]
    fn case_ids_cannot_escape_the_fixture_root() {
        assert!(valid_case_id("ec2/ec2-01", "ec2"));
        assert!(!valid_case_id("ec2/../secret", "ec2"));
        assert!(!valid_case_id("rds/rds-01", "ec2"));
        assert!(!valid_case_id("ec2/EC2-01", "ec2"));
    }

    #[test]
    fn generated_uuids_are_removed_from_review_output() {
        let mut value = json!({
            "id": "10203040-5060-7080-90a0-b0c0d0e0f000",
            "provider_id": "i-0123456789abcdef0",
            "items": [{ "id": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee" }]
        });

        scrub_generated_uuids(&mut value, &mut 0);

        assert_eq!(value["id"], "<generated-uuid-1>");
        assert_eq!(value["provider_id"], "i-0123456789abcdef0");
        assert_eq!(value["items"][0]["id"], "<generated-uuid-2>");
    }

    #[test]
    fn draft_assertions_use_json_pointers_without_coercion() {
        let expected = ExpectedCase {
            case_id: "ec2/ec2-01".to_owned(),
            project_type: ClassifiedProjectType::Ec2,
            minimum_confidence: ClassificationConfidence::Medium,
            draft_assertions: vec![DraftAssertion {
                pointer: "/resources/0/quantity".to_owned(),
                equals: json!(3),
            }],
        };
        let project = json!({ "resources": [{ "quantity": 3 }] });

        let assertions = draft_assertions(&expected, &project);

        assert_eq!(assertions.len(), 1);
        assert!(assertions[0].passed);
    }

    #[test]
    fn committed_fixture_corpus_is_valid_and_normalizable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/assistant-workload-classification")
            .canonicalize()
            .expect("committed fixture root");
        let cases = load_cases(&root).expect("valid committed fixture manifest");

        for fixture in cases {
            let case_directory = root.join(&fixture.id);
            let sidecar = fs::read_to_string(case_directory.join("expected.json"))
                .expect("fixture expectation sidecar");
            let sidecar: ExpectedCase =
                serde_json::from_str(&sidecar).expect("valid fixture expectation sidecar");
            assert_eq!(sidecar.case_id, fixture.expected.case_id);
            assert_eq!(sidecar.project_type, fixture.expected.project_type);
            assert_eq!(
                sidecar.minimum_confidence,
                fixture.expected.minimum_confidence
            );

            let image = fs::read(case_directory.join("input.png")).expect("fixture screenshot");
            normalize_image(ImageMediaType::Png, &image).expect("normalizable fixture screenshot");
        }
    }

    #[test]
    fn result_writer_replaces_an_existing_review_file() {
        let root = env::temp_dir().join(format!("tco-assistant-evaluation-{}", Uuid::new_v4()));
        let case_directory = root.join("ec2/ec2-01");
        fs::create_dir_all(&case_directory).expect("temporary case directory");
        fs::write(case_directory.join("result.md"), "old result").expect("existing result marker");
        let fixture = FixtureCase {
            id: "ec2/ec2-01".to_owned(),
            family: "ec2".to_owned(),
            expected: ExpectedCase {
                case_id: "ec2/ec2-01".to_owned(),
                project_type: ClassifiedProjectType::Ec2,
                minimum_confidence: ClassificationConfidence::Medium,
                draft_assertions: vec![DraftAssertion {
                    pointer: "/settings/project_type".to_owned(),
                    equals: json!("ec2"),
                }],
            },
        };
        let completed = CompletedCase {
            classification: ImageProjectClassification {
                project_type: ClassifiedProjectType::Ec2,
                confidence: ClassificationConfidence::High,
                evidence: vec!["Amazon EC2".to_owned()],
                ambiguities: Vec::new(),
            },
            response: json!({ "classification": { "project_type": "ec2", "confidence": "high" }, "proposal": null }),
            assertions: Vec::new(),
            passed: false,
        };

        write_result(
            &root,
            &fixture,
            EvaluationIdentity::SystemAssignedManagedIdentity,
            Ok(completed),
        )
        .expect("replace result");

        let result =
            fs::read_to_string(case_directory.join("result.md")).expect("replacement result");
        assert!(result.contains("Complete Sanitized Response"));
        assert!(result.contains("Expected family: `ec2`"));
        assert!(result.contains("Expected minimum confidence: `medium`"));
        assert!(result.contains("Observed family: `ec2`"));
        assert!(result.contains("Observed confidence: `high`"));
        assert!(result.contains("Evaluation identity: `system_assigned_managed_identity`"));
        assert!(!result.contains("old result"));
        fs::remove_dir_all(root).expect("remove temporary corpus");
    }
}
