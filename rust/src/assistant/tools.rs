//! Tool contract and dispatch.
//!
//! Each tool is declared once with a stable name, a concise description, a closed JSON Schema,
//! a risk class, and the single phase it belongs to. Tools call existing application services
//! directly so authorization and financial rules cannot diverge from the normal API.
//!
//! Model-visible schemas never contain identity, tenant, partition, ETag, credential,
//! endpoint, price-snapshot, or confirmation fields. Those arrive through
//! [`TurnContext`](super::context::TurnContext).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    calculation::{
        engine::{CalculationRevision, PortfolioTotals, PricingStatus},
        target_selector::MappingStatus,
    },
    domain::{
        project::{EditableProject, ProjectSettings, ValidationIssue},
        resource::Resource,
    },
    persistence::repository::RepositoryError,
    state::AppState,
};

use super::{
    budget::MAX_TOOL_RESULT_CHARS,
    context::{TurnContext, TurnPhase},
    help,
    model::ToolSchema,
};

/// Documented Azure OpenAI limit for a function description.
pub const MAX_TOOL_DESCRIPTION_CHARS: usize = 1_024;

/// Effect class of a tool, used by preflight to decide batching and confirmation rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolRisk {
    /// Reads reviewed or owner-scoped data. No state changes.
    Read,
    /// Changes only the unsaved browser draft and stays undoable.
    Draft,
    /// Writes owner-scoped persisted state.
    Persist,
    /// Deletes or shares owner-scoped state.
    Destructive,
}

impl ToolRisk {
    pub fn is_mutating(self) -> bool {
        matches!(self, Self::Persist | Self::Destructive)
    }

    /// Whether an explicit user confirmation is required before execution.
    pub fn requires_confirmation(self) -> bool {
        self.is_mutating()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    /// Closed JSON Schema for the arguments. Runtime typed validation still applies.
    pub parameters: &'static str,
    pub phase: TurnPhase,
    pub risk: ToolRisk,
}

impl ToolDefinition {
    pub fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name,
            description: self.description,
            parameters: self.parameters,
        }
    }
}

const APPLICATION_HELP_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["question"],
  "properties": {
    "question": {
      "type": "string",
      "minLength": 1,
      "maxLength": 1000,
      "description": "The user's question about a visible field, button, state, or supported workflow."
    },
    "control_id": {
      "type": "string",
      "minLength": 1,
      "maxLength": 64,
      "description": "Optional stable control identifier returned by an earlier help result."
    }
  }
}"#;

const CURRENT_PROJECT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": {}
}"#;

const PROJECT_PATCH_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "patch": {
      "type": "object",
      "additionalProperties": false,
      "description": "Candidate changes to the selected project. Omitted members keep their current value.",
      "properties": {
        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
        "description": { "type": "string", "maxLength": 500 },
        "settings": {
          "type": "object",
          "description": "Complete replacement project settings using the documented project schema."
        },
        "resources": {
          "type": "array",
          "maxItems": 100,
          "items": { "type": "object" },
          "description": "Complete replacement workload list using the documented project schema."
        }
      }
    }
  }
}"#;

/// Every registered tool. Dispatch matches explicitly over this list.
pub const TOOLS: &[ToolDefinition] = &[
    ToolDefinition {
        name: "get_application_help",
        description: "Read the reviewed application help catalog for a field, button, state, or workflow. Returns product behaviour statements and the control identifiers they came from. This is the only approved source of product behaviour claims.",
        parameters: APPLICATION_HELP_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
    },
    ToolDefinition {
        name: "get_current_project",
        description: "Read the project the user currently has open. Returns its settings and workloads with names and provider identifiers removed. The project is chosen by the application, not by this call.",
        parameters: CURRENT_PROJECT_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
    },
    ToolDefinition {
        name: "validate_project_patch",
        description: "Check candidate changes to the open project against server-side validation without saving anything. Returns field-level errors, or confirms the candidate project is valid.",
        parameters: PROJECT_PATCH_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
    },
    ToolDefinition {
        name: "calculate_project_draft",
        description: "Run the authoritative server-side calculation for the open project with candidate changes applied, without saving anything. Returns deterministic totals and per-workload mapping and pricing status. Never calculate or estimate money yourself.",
        parameters: PROJECT_PATCH_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
    },
];

pub fn find(name: &str) -> Option<&'static ToolDefinition> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// Tools exposed to the model for a phase. A model never sees a capability it cannot use.
pub fn schemas_for_phase(phase: TurnPhase) -> Vec<ToolSchema> {
    TOOLS
        .iter()
        .filter(|tool| tool.phase == phase)
        .map(ToolDefinition::schema)
        .collect()
}

/// Candidate project changes a model may propose.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub settings: Option<ProjectSettings>,
    pub resources: Option<Vec<Resource>>,
}

impl ProjectPatch {
    /// Apply the candidate changes to a host-supplied base project.
    pub fn apply(&self, base: &EditableProject) -> EditableProject {
        EditableProject {
            name: self.name.clone().unwrap_or_else(|| base.name.clone()),
            description: self
                .description
                .clone()
                .or_else(|| base.description.clone()),
            settings: self
                .settings
                .clone()
                .unwrap_or_else(|| base.settings.clone()),
            resources: self
                .resources
                .clone()
                .unwrap_or_else(|| base.resources.clone()),
            aws_price_snapshot_id: base.aws_price_snapshot_id.clone(),
            azure_price_snapshot_id: base.azure_price_snapshot_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationHelpInput {
    pub question: String,
    #[serde(default)]
    pub control_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentProjectInput {}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPatchInput {
    #[serde(default)]
    pub patch: ProjectPatch,
}

/// Typed, validated arguments for one registered tool.
#[derive(Clone, Debug)]
pub enum ToolInput {
    ApplicationHelp(ApplicationHelpInput),
    CurrentProject,
    ValidateProjectPatch(ProjectPatchInput),
    CalculateProjectDraft(ProjectPatchInput),
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("the tool arguments did not match the tool contract")]
pub struct InvalidToolArguments;

/// Parse untrusted model arguments into a typed input for a registered tool.
pub fn parse_input(
    definition: &ToolDefinition,
    arguments: &str,
) -> Result<ToolInput, InvalidToolArguments> {
    let arguments = if arguments.trim().is_empty() {
        "{}"
    } else {
        arguments
    };
    match definition.name {
        "get_application_help" => {
            let input: ApplicationHelpInput =
                serde_json::from_str(arguments).map_err(|_| InvalidToolArguments)?;
            let question_chars = input.question.trim().chars().count();
            if !(1..=help::MAX_QUESTION_CHARS).contains(&question_chars) {
                return Err(InvalidToolArguments);
            }
            if input
                .control_id
                .as_ref()
                .is_some_and(|control_id| !(1..=64).contains(&control_id.chars().count()))
            {
                return Err(InvalidToolArguments);
            }
            Ok(ToolInput::ApplicationHelp(input))
        }
        "get_current_project" => {
            let _input: CurrentProjectInput =
                serde_json::from_str(arguments).map_err(|_| InvalidToolArguments)?;
            Ok(ToolInput::CurrentProject)
        }
        "validate_project_patch" => serde_json::from_str(arguments)
            .map(ToolInput::ValidateProjectPatch)
            .map_err(|_| InvalidToolArguments),
        "calculate_project_draft" => serde_json::from_str(arguments)
            .map(ToolInput::CalculateProjectDraft)
            .map_err(|_| InvalidToolArguments),
        _ => Err(InvalidToolArguments),
    }
}

/// Structured result of one tool call. Failure codes are stable and carry no internal detail.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolOutcome {
    Ok { result: Value },
    Invalid { errors: Vec<ValidationIssue> },
    Unavailable { code: &'static str },
    Error { code: &'static str },
}

impl ToolOutcome {
    /// Serialize the outcome for model context, replacing anything oversized with a code.
    pub fn to_bounded_json(&self) -> String {
        let Ok(serialized) = serde_json::to_string(self) else {
            return r#"{"status":"error","code":"result_not_serializable"}"#.to_owned();
        };
        if serialized.chars().count() > MAX_TOOL_RESULT_CHARS {
            return r#"{"status":"error","code":"result_too_large"}"#.to_owned();
        }
        serialized
    }
}

/// Execute one preflighted tool call against existing application services.
pub async fn dispatch(state: &AppState, context: &TurnContext, input: &ToolInput) -> ToolOutcome {
    match input {
        ToolInput::ApplicationHelp(input) => application_help(input),
        ToolInput::CurrentProject => current_project(state, context).await,
        ToolInput::ValidateProjectPatch(input) => {
            validate_project_patch(state, context, &input.patch).await
        }
        ToolInput::CalculateProjectDraft(input) => {
            calculate_project_draft(state, context, &input.patch).await
        }
    }
}

fn application_help(input: &ApplicationHelpInput) -> ToolOutcome {
    let response = match input.control_id.as_deref() {
        Some(control_id) => match help::explain_control(control_id) {
            Some(response) => response,
            None => {
                return ToolOutcome::Error {
                    code: "unknown_control_id",
                };
            }
        },
        None => match help::answer_question(&input.question) {
            Ok(response) => response,
            Err(errors) => return ToolOutcome::Invalid { errors },
        },
    };
    into_ok(&response)
}

async fn current_project(state: &AppState, context: &TurnContext) -> ToolOutcome {
    let Some(project) = read_selected_project(state, context).await else {
        return ToolOutcome::Unavailable {
            code: "no_selected_project",
        };
    };
    let project = match project {
        Ok(project) => project,
        Err(outcome) => return outcome,
    };
    into_ok(&ProjectView::from_project(&project))
}

async fn validate_project_patch(
    state: &AppState,
    context: &TurnContext,
    patch: &ProjectPatch,
) -> ToolOutcome {
    let Some(project) = read_selected_project(state, context).await else {
        return ToolOutcome::Unavailable {
            code: "no_selected_project",
        };
    };
    let project = match project {
        Ok(project) => project,
        Err(outcome) => return outcome,
    };

    let candidate = patch.apply(&project);
    let issues = candidate.validate();
    if issues.is_empty() {
        into_ok(&json!({ "valid": true }))
    } else {
        ToolOutcome::Invalid { errors: issues }
    }
}

async fn calculate_project_draft(
    state: &AppState,
    context: &TurnContext,
    patch: &ProjectPatch,
) -> ToolOutcome {
    let Some(project) = read_selected_project(state, context).await else {
        return ToolOutcome::Unavailable {
            code: "no_selected_project",
        };
    };
    let project = match project {
        Ok(project) => project,
        Err(outcome) => return outcome,
    };

    let candidate = patch.apply(&project);
    let issues = candidate.validate();
    if !issues.is_empty() {
        return ToolOutcome::Invalid { errors: issues };
    }

    let Ok(permit) = state.calculation_slots.clone().try_acquire_owned() else {
        return ToolOutcome::Unavailable {
            code: "calculation_busy",
        };
    };
    let revision = crate::api::calculations::calculate_project(state, &candidate, None).await;
    drop(permit);

    match revision {
        Ok(revision) => into_ok(&CalculationView::from_revision(&revision)),
        Err(_) => ToolOutcome::Error {
            code: "calculation_failed",
        },
    }
}

async fn read_selected_project(
    state: &AppState,
    context: &TurnContext,
) -> Option<Result<EditableProject, ToolOutcome>> {
    let selected = context.project()?;
    let document = match state.projects.get(context.owner_id(), selected.id).await {
        Ok(document) => document,
        Err(RepositoryError::NotFound) => {
            return Some(Err(ToolOutcome::Error {
                code: "project_not_found",
            }));
        }
        Err(_) => {
            return Some(Err(ToolOutcome::Unavailable {
                code: "storage_unavailable",
            }));
        }
    };

    Some(Ok(EditableProject {
        name: document.name,
        description: document.description,
        settings: document.settings,
        resources: document.resources,
        aws_price_snapshot_id: document.aws_price_snapshot_id,
        azure_price_snapshot_id: document.azure_price_snapshot_id,
    }))
}

fn into_ok<T: Serialize>(value: &T) -> ToolOutcome {
    match serde_json::to_value(value) {
        Ok(result) => ToolOutcome::Ok { result },
        Err(_) => ToolOutcome::Error {
            code: "result_not_serializable",
        },
    }
}

/// Model-visible projection of a project.
#[derive(Debug, Serialize)]
struct ProjectView {
    settings: ProjectSettings,
    resources: Vec<Resource>,
    resource_count: usize,
    has_aws_price_snapshot: bool,
    has_azure_price_snapshot: bool,
}

impl ProjectView {
    fn from_project(project: &EditableProject) -> Self {
        let resources = project
            .resources
            .iter()
            .enumerate()
            .map(|(index, resource)| redact_resource(resource, index))
            .collect::<Vec<_>>();
        Self {
            settings: project.settings.clone(),
            resource_count: resources.len(),
            resources,
            has_aws_price_snapshot: project.aws_price_snapshot_id.is_some(),
            has_azure_price_snapshot: project.azure_price_snapshot_id.is_some(),
        }
    }
}

fn redact_resource(resource: &Resource, index: usize) -> Resource {
    let mut redacted = resource.clone();
    let workload_name = format!("workload-{}", index + 1);
    match &mut redacted {
        Resource::Ec2(ec2) => {
            ec2.shared.workload_name = workload_name;
            for (volume_index, volume) in ec2.volumes.iter_mut().enumerate() {
                volume.label = format!("volume-{}", volume_index + 1);
                volume.aws_volume_id = None;
            }
        }
        Resource::Rds(rds) => rds.shared.workload_name = workload_name,
        Resource::OnPrem(on_prem) => on_prem.shared.workload_name = workload_name,
    }
    redacted
}

/// Bounded projection of an authoritative calculation.
#[derive(Debug, Serialize)]
struct CalculationView {
    formula_version: String,
    portfolio_totals: PortfolioTotals,
    warnings: Vec<String>,
    resources: Vec<CalculatedResourceView>,
}

#[derive(Debug, Serialize)]
struct CalculatedResourceView {
    resource_id: Uuid,
    mapping_status: Option<MappingStatus>,
    aws_pricing_status: PricingStatus,
    azure_pricing_status: PricingStatus,
    selected_target_configuration_key: Option<String>,
    unresolved_component_codes: Vec<String>,
}

impl CalculationView {
    fn from_revision(revision: &CalculationRevision) -> Self {
        Self {
            formula_version: revision.formula_version.clone(),
            portfolio_totals: revision.portfolio_totals.clone(),
            warnings: revision.warnings.clone(),
            resources: revision
                .resource_results
                .iter()
                .map(|result| CalculatedResourceView {
                    resource_id: result.resource_id,
                    mapping_status: result.mapping_status,
                    aws_pricing_status: result.aws_pricing_status,
                    azure_pricing_status: result.azure_pricing_status,
                    selected_target_configuration_key: result
                        .target_selection
                        .as_ref()
                        .and_then(|selection| selection.selected.as_ref())
                        .map(|target| target.configuration_key.clone()),
                    unresolved_component_codes: result
                        .unresolved_components
                        .iter()
                        .map(|component| component.code.clone())
                        .collect(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORBIDDEN_FIELD_TERMS: &[&str] = &[
        "owner_id",
        "tenant_id",
        "\"tid\"",
        "\"oid\"",
        "etag",
        "if_match",
        "partition",
        "endpoint",
        "api_key",
        "access_token",
        "credential",
        "connection_string",
        "snapshot_id",
        "confirmation",
        "revision",
    ];

    fn schema(name: &str) -> Value {
        let definition = find(name).expect("the tool must be registered");
        serde_json::from_str(definition.parameters).expect("the schema must be valid JSON")
    }

    #[test]
    fn every_tool_schema_is_a_closed_object() {
        for tool in TOOLS {
            let schema: Value =
                serde_json::from_str(tool.parameters).expect("the schema must be valid JSON");
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(
                schema["additionalProperties"], false,
                "{} must reject unknown properties",
                tool.name
            );
            for required in schema["required"].as_array().unwrap_or(&Vec::new()) {
                let name = required.as_str().expect("required entries are strings");
                assert!(
                    schema["properties"].get(name).is_some(),
                    "{} requires undeclared property {name}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn tool_names_are_unique_and_descriptions_fit_the_documented_limit() {
        let mut names = std::collections::HashSet::new();
        for tool in TOOLS {
            assert!(names.insert(tool.name), "duplicate {}", tool.name);
            assert!(!tool.description.trim().is_empty(), "{}", tool.name);
            assert!(
                tool.description.chars().count() <= MAX_TOOL_DESCRIPTION_CHARS,
                "{} description exceeds {MAX_TOOL_DESCRIPTION_CHARS} characters",
                tool.name
            );
        }
    }

    #[test]
    fn no_model_visible_schema_exposes_a_host_owned_field() {
        for tool in TOOLS {
            let surface = format!("{} {}", tool.parameters, tool.description).to_ascii_lowercase();
            for term in FORBIDDEN_FIELD_TERMS {
                assert!(
                    !surface.contains(term),
                    "{} exposes host-owned field {term}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn this_slice_registers_read_only_tools() {
        for tool in TOOLS {
            assert_eq!(tool.phase, TurnPhase::ReadPlan, "{}", tool.name);
            assert_eq!(tool.risk, ToolRisk::Read, "{}", tool.name);
            assert!(!tool.risk.is_mutating(), "{}", tool.name);
            assert!(!tool.risk.requires_confirmation(), "{}", tool.name);
        }
        assert_eq!(schemas_for_phase(TurnPhase::ReadPlan).len(), TOOLS.len());
        assert!(schemas_for_phase(TurnPhase::Propose).is_empty());
        assert!(schemas_for_phase(TurnPhase::Execute).is_empty());
    }

    #[test]
    fn mutating_risk_classes_always_require_confirmation() {
        assert!(ToolRisk::Persist.requires_confirmation());
        assert!(ToolRisk::Destructive.requires_confirmation());
        assert!(!ToolRisk::Read.requires_confirmation());
        assert!(!ToolRisk::Draft.requires_confirmation());
        assert!(!ToolRisk::Draft.is_mutating());
    }

    #[test]
    fn help_arguments_are_bounded_at_runtime() {
        let definition = find("get_application_help").expect("registered");

        assert!(parse_input(definition, r#"{"question":"What is NO MAPPING?"}"#).is_ok());
        assert!(parse_input(definition, r#"{"question":"   "}"#).is_err());
        assert!(parse_input(definition, r#"{"question":""}"#).is_err());
        assert!(parse_input(definition, "{}").is_err());
        assert!(parse_input(definition, "not json").is_err());

        let oversized = format!(
            r#"{{"question":"{}"}}"#,
            "a".repeat(help::MAX_QUESTION_CHARS + 1)
        );
        assert!(parse_input(definition, &oversized).is_err());
    }

    #[test]
    fn unknown_and_extra_arguments_are_rejected() {
        let help_tool = find("get_application_help").expect("registered");
        assert!(
            parse_input(
                help_tool,
                r#"{"question":"hello","owner_id":"entra:tenant:owner"}"#
            )
            .is_err()
        );

        let project_tool = find("get_current_project").expect("registered");
        assert!(parse_input(project_tool, r#"{"project_id":"any"}"#).is_err());
        assert!(parse_input(project_tool, "{}").is_ok());
        assert!(parse_input(project_tool, "").is_ok());
    }

    #[test]
    fn a_patch_cannot_carry_price_snapshot_or_result_fields() {
        let definition = find("calculate_project_draft").expect("registered");

        assert!(
            parse_input(
                definition,
                r#"{"patch":{"aws_price_snapshot_id":"aws-2026-01-01"}}"#
            )
            .is_err()
        );
        assert!(
            parse_input(
                definition,
                r#"{"patch":{"latest_calculation_revision":{"formula_version":"1.0.0"}}}"#
            )
            .is_err()
        );
        assert!(parse_input(definition, r#"{"patch":{"name":"Estimate"}}"#).is_ok());
    }

    #[test]
    fn an_unregistered_tool_name_never_parses() {
        let borrowed = ToolDefinition {
            name: "run_sql",
            description: "unregistered",
            parameters: CURRENT_PROJECT_SCHEMA,
            phase: TurnPhase::ReadPlan,
            risk: ToolRisk::Read,
        };

        assert!(parse_input(&borrowed, "{}").is_err());
        assert!(find("run_sql").is_none());
    }

    #[test]
    fn the_patch_schema_declares_the_documented_workload_ceiling() {
        let schema = schema("validate_project_patch");

        assert_eq!(
            schema["properties"]["patch"]["properties"]["resources"]["maxItems"],
            json!(crate::domain::project::MAX_PROJECT_RESOURCES)
        );
        assert_eq!(
            schema["properties"]["patch"]["additionalProperties"],
            json!(false)
        );
    }

    #[test]
    fn an_oversized_tool_result_is_replaced_with_a_stable_code() {
        let outcome = ToolOutcome::Ok {
            result: json!({ "text": "a".repeat(MAX_TOOL_RESULT_CHARS) }),
        };

        assert_eq!(
            outcome.to_bounded_json(),
            r#"{"status":"error","code":"result_too_large"}"#
        );
    }

    #[test]
    fn a_help_outcome_serializes_with_its_control_references() {
        let input = ApplicationHelpInput {
            question: "What does the Azure region control?".to_owned(),
            control_id: None,
        };

        let serialized = application_help(&input).to_bounded_json();

        assert!(serialized.contains(r#""status":"ok""#));
        assert!(serialized.contains("project.azure-region"));
    }

    #[test]
    fn an_unknown_control_identifier_returns_a_stable_code() {
        let input = ApplicationHelpInput {
            question: "anything".to_owned(),
            control_id: Some("project.not-a-control".to_owned()),
        };

        assert_eq!(
            application_help(&input).to_bounded_json(),
            r#"{"status":"error","code":"unknown_control_id"}"#
        );
    }
}
