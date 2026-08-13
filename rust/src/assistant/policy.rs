//! All-or-nothing preflight for a proposed tool batch.
//!
//! Every call in a batch is validated before any call executes, so a batch that mixes a valid
//! read with an unknown tool, a wrong-phase capability, or malformed arguments produces no side
//! effects at all.

use std::collections::HashSet;

use thiserror::Error;

use super::{
    budget::{MAX_MUTATING_CALLS_PER_BATCH, MAX_TOOL_CALLS_PER_RESPONSE, TurnBudget},
    context::{TurnContext, TurnPhase},
    model::ProposedToolCall,
    tools::{self, ToolDefinition, ToolInput, ToolRisk},
};

const MAX_CALL_ID_CHARS: usize = 128;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PolicyError {
    #[error("the model proposed an empty tool batch")]
    EmptyBatch,
    #[error("the model returned an answer before reading an authoritative tool result")]
    UngroundedResponse,
    #[error("the tool batch exceeded the per-response limit")]
    BatchTooLarge,
    #[error("the tool batch exceeded the remaining tool budget")]
    BudgetExhausted,
    #[error("a tool call identifier was malformed or repeated")]
    InvalidCallId,
    #[error("the model proposed a tool that is not registered")]
    UnknownTool,
    #[error("the model proposed a tool that is unavailable in this phase")]
    PhaseNotAllowed,
    #[error("the model proposed a tool that is unavailable for the selected-project state")]
    ProjectContextNotAllowed,
    #[error("the tool batch contained more than one mutating call")]
    TooManyMutations,
    #[error("the model supplied arguments the tool schema rejects")]
    InvalidArguments,
    #[error("a mutating tool was not confirmed by the user")]
    MissingConfirmation,
}

/// One accepted call, ready to dispatch.
#[derive(Clone, Debug)]
pub struct ValidatedToolCall {
    pub id: String,
    pub definition: &'static ToolDefinition,
    pub input: ToolInput,
}

/// Validate a whole proposed batch. Nothing executes unless every call is acceptable.
pub fn preflight(
    batch: &[ProposedToolCall],
    context: &TurnContext,
    budget: &TurnBudget,
    executed_call_ids: &HashSet<String>,
) -> Result<Vec<ValidatedToolCall>, PolicyError> {
    if batch.is_empty() {
        return Err(PolicyError::EmptyBatch);
    }
    if batch.len() > MAX_TOOL_CALLS_PER_RESPONSE {
        return Err(PolicyError::BatchTooLarge);
    }
    if batch.len() > budget.remaining_tool_calls() {
        return Err(PolicyError::BudgetExhausted);
    }

    let mut batch_call_ids = HashSet::with_capacity(batch.len());
    let mut mutating_calls = 0usize;
    let mut validated = Vec::with_capacity(batch.len());

    for call in batch {
        if !is_acceptable_call_id(&call.id)
            || executed_call_ids.contains(&call.id)
            || !batch_call_ids.insert(call.id.as_str())
        {
            return Err(PolicyError::InvalidCallId);
        }

        let definition = tools::find(&call.name).ok_or(PolicyError::UnknownTool)?;
        let is_authoritative_read =
            definition.phase == TurnPhase::ReadPlan && definition.risk == ToolRisk::Read;
        if definition.phase != context.phase() && !is_authoritative_read {
            return Err(PolicyError::PhaseNotAllowed);
        }
        if !definition.is_available(context) {
            return Err(PolicyError::ProjectContextNotAllowed);
        }
        if definition.risk.is_mutating() {
            mutating_calls += 1;
            if mutating_calls > MAX_MUTATING_CALLS_PER_BATCH {
                return Err(PolicyError::TooManyMutations);
            }
            if definition.risk.requires_confirmation() && !context.is_confirmed(definition.name) {
                return Err(PolicyError::MissingConfirmation);
            }
        }

        let input = tools::parse_input(definition, &call.arguments)
            .map_err(|_| PolicyError::InvalidArguments)?;
        validated.push(ValidatedToolCall {
            id: call.id.clone(),
            definition,
            input,
        });
    }

    Ok(validated)
}

fn is_acceptable_call_id(call_id: &str) -> bool {
    (1..=MAX_CALL_ID_CHARS).contains(&call_id.chars().count())
        && call_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::assistant::{
        budget::MAX_TOOL_CALLS,
        context::{SelectedProject, TurnPhase},
    };

    fn context(phase: TurnPhase) -> TurnContext {
        TurnContext::new("entra:tenant:owner", Uuid::nil(), phase)
    }

    fn context_with_project(phase: TurnPhase) -> TurnContext {
        context(phase).with_project(SelectedProject {
            id: Uuid::nil(),
            etag: "etag".to_owned(),
            aws_price_snapshot_id: None,
            azure_price_snapshot_id: None,
        })
    }

    fn call(id: &str, name: &str, arguments: &str) -> ProposedToolCall {
        ProposedToolCall {
            id: id.to_owned(),
            name: name.to_owned(),
            arguments: arguments.to_owned(),
        }
    }

    fn help_call(id: &str) -> ProposedToolCall {
        call(
            id,
            "get_application_help",
            r#"{"question":"What is NO MAPPING?"}"#,
        )
    }

    fn stage_call(id: &str) -> ProposedToolCall {
        call(
            id,
            "stage_project_patch",
            r#"{"patch":{"name":"Imported estimate"},"omissions":[],"uncertainties":[]}"#,
        )
    }

    #[test]
    fn a_valid_read_batch_is_accepted() {
        let accepted = preflight(
            &[
                help_call("call-1"),
                call("call-2", "get_current_project", "{}"),
            ],
            &context_with_project(TurnPhase::ReadPlan),
            &TurnBudget::new(),
            &HashSet::new(),
        )
        .expect("a valid read batch is allowed");

        assert_eq!(accepted.len(), 2);
        assert_eq!(accepted[0].definition.name, "get_application_help");
        assert_eq!(accepted[1].definition.name, "get_current_project");
    }

    #[test]
    fn one_malformed_call_rejects_the_whole_batch() {
        let error = preflight(
            &[
                help_call("call-1"),
                call("call-2", "get_current_project", "{\"x\":1}"),
            ],
            &context_with_project(TurnPhase::ReadPlan),
            &TurnBudget::new(),
            &HashSet::new(),
        )
        .expect_err("a batch with malformed arguments must be rejected as a unit");

        assert_eq!(error, PolicyError::InvalidArguments);
    }

    #[test]
    fn an_unknown_tool_rejects_the_whole_batch() {
        let error = preflight(
            &[help_call("call-1"), call("call-2", "run_shell", "{}")],
            &context_with_project(TurnPhase::ReadPlan),
            &TurnBudget::new(),
            &HashSet::new(),
        )
        .expect_err("an unregistered tool must be rejected");

        assert_eq!(error, PolicyError::UnknownTool);
    }

    #[test]
    fn authoritative_reads_are_available_in_later_phases() {
        for phase in [TurnPhase::Propose, TurnPhase::Execute] {
            let accepted = preflight(
                &[help_call("call-1")],
                &context(phase),
                &TurnBudget::new(),
                &HashSet::new(),
            )
            .expect("a proposal or execution phase may ground itself with authoritative reads");

            assert_eq!(accepted[0].definition.risk, ToolRisk::Read);
        }
    }

    #[test]
    fn a_draft_capability_is_available_only_in_the_proposal_phase() {
        let accepted = preflight(
            &[stage_call("call-1")],
            &context_with_project(TurnPhase::Propose),
            &TurnBudget::new(),
            &HashSet::new(),
        )
        .expect("the proposal phase may stage an undoable draft");
        assert_eq!(accepted[0].definition.risk, ToolRisk::Draft);

        for phase in [TurnPhase::ReadPlan, TurnPhase::Execute] {
            assert_eq!(
                preflight(
                    &[stage_call("call-1")],
                    &context(phase),
                    &TurnBudget::new(),
                    &HashSet::new(),
                )
                .expect_err("a draft capability must not cross its phase boundary"),
                PolicyError::PhaseNotAllowed
            );
        }
    }

    #[test]
    fn project_context_requirements_are_enforced_even_for_known_tool_names() {
        assert_eq!(
            preflight(
                &[call("call-1", "get_current_project", "{}")],
                &context(TurnPhase::ReadPlan),
                &TurnBudget::new(),
                &HashSet::new(),
            )
            .expect_err("a project read requires a selected project"),
            PolicyError::ProjectContextNotAllowed
        );
        assert_eq!(
            preflight(
                &[call(
                    "call-1",
                    "stage_new_project_draft",
                    r#"{"project_type":"on_prem","omissions":[],"uncertainties":[]}"#,
                )],
                &context_with_project(TurnPhase::Propose),
                &TurnBudget::new(),
                &HashSet::new(),
            )
            .expect_err("a new draft requires no selected project"),
            PolicyError::ProjectContextNotAllowed
        );
    }

    #[test]
    fn batch_size_is_bounded() {
        let batch = (0..=MAX_TOOL_CALLS_PER_RESPONSE)
            .map(|index| help_call(&format!("call-{index}")))
            .collect::<Vec<_>>();

        let error = preflight(
            &batch,
            &context_with_project(TurnPhase::ReadPlan),
            &TurnBudget::new(),
            &HashSet::new(),
        )
        .expect_err("an oversized batch must be rejected");

        assert_eq!(error, PolicyError::BatchTooLarge);
        assert_eq!(
            preflight(
                &[],
                &context(TurnPhase::ReadPlan),
                &TurnBudget::new(),
                &HashSet::new()
            )
            .expect_err("an empty batch must be rejected"),
            PolicyError::EmptyBatch
        );
    }

    #[test]
    fn a_batch_larger_than_the_remaining_budget_is_rejected() {
        let mut budget = TurnBudget::new();
        budget
            .charge_tool_calls(usize::try_from(MAX_TOOL_CALLS).expect("budget fits in usize"))
            .expect("the full budget can be charged once");

        let error = preflight(
            &[help_call("call-1")],
            &context(TurnPhase::ReadPlan),
            &budget,
            &HashSet::new(),
        )
        .expect_err("an exhausted budget must reject the batch");

        assert_eq!(error, PolicyError::BudgetExhausted);
    }

    #[test]
    fn repeated_and_malformed_call_identifiers_are_rejected() {
        let duplicate_in_batch = preflight(
            &[help_call("call-1"), help_call("call-1")],
            &context(TurnPhase::ReadPlan),
            &TurnBudget::new(),
            &HashSet::new(),
        )
        .expect_err("a repeated identifier must be rejected");
        assert_eq!(duplicate_in_batch, PolicyError::InvalidCallId);

        let already_executed = preflight(
            &[help_call("call-1")],
            &context(TurnPhase::ReadPlan),
            &TurnBudget::new(),
            &HashSet::from(["call-1".to_owned()]),
        )
        .expect_err("replaying an executed identifier must be rejected");
        assert_eq!(already_executed, PolicyError::InvalidCallId);

        for identifier in ["", "call 1", "call/1", &"c".repeat(MAX_CALL_ID_CHARS + 1)] {
            assert_eq!(
                preflight(
                    &[help_call(identifier)],
                    &context(TurnPhase::ReadPlan),
                    &TurnBudget::new(),
                    &HashSet::new()
                )
                .expect_err("a malformed identifier must be rejected"),
                PolicyError::InvalidCallId,
                "identifier {identifier:?} must be rejected"
            );
        }
    }

    #[test]
    fn identity_arguments_are_rejected_rather_than_ignored() {
        let error = preflight(
            &[call(
                "call-1",
                "get_current_project",
                r#"{"owner_id":"entra:other-tenant:other-owner"}"#,
            )],
            &context_with_project(TurnPhase::ReadPlan),
            &TurnBudget::new(),
            &HashSet::new(),
        )
        .expect_err("a model may not supply an owner identifier");

        assert_eq!(error, PolicyError::InvalidArguments);
    }
}
