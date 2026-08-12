//! Host-enforced budgets for one assistant turn.
//!
//! A model cannot raise these limits. Exhausting one ends the turn with a structured error
//! rather than allowing an unbounded loop.

use std::time::Duration;

use thiserror::Error;

/// Model requests allowed in one turn.
pub const MAX_MODEL_REQUESTS: u32 = 8;
/// Tool calls allowed in one turn.
pub const MAX_TOOL_CALLS: u32 = 12;
/// Tool calls allowed in one model response.
pub const MAX_TOOL_CALLS_PER_RESPONSE: usize = 4;
/// Mutating tool calls allowed in one batch, so a partial failure has one clear outcome.
pub const MAX_MUTATING_CALLS_PER_BATCH: usize = 1;
/// Whole-turn wall clock.
pub const MAX_TURN_WALL_CLOCK: Duration = Duration::from_secs(120);
/// Upper bound on one model call, leaving time for validation, tools, and a useful error.
pub const MAX_MODEL_CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Conservative host-side bound approximating the 32,000-token prompt budget at four
/// characters per token. It guards transport size and disclosure; it is not a tokenizer.
pub const MAX_PROMPT_CONTEXT_CHARS: usize = 128_000;
/// Model output ceiling requested for every call.
pub const MAX_MODEL_OUTPUT_TOKENS: u32 = 4_000;
/// Upper bound on one serialized tool result appended to model context.
pub const MAX_TOOL_RESULT_CHARS: usize = 8_000;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BudgetError {
    #[error("model request budget is exhausted")]
    ModelRequests,
    #[error("tool call budget is exhausted")]
    ToolCalls,
    #[error("prompt context budget is exhausted")]
    PromptContext,
}

/// Mutable per-turn counters. One instance belongs to exactly one turn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TurnBudget {
    model_requests: u32,
    tool_calls: u32,
}

impl TurnBudget {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserve one model request, or fail when the turn has used its allowance.
    pub fn charge_model_request(&mut self) -> Result<(), BudgetError> {
        if self.model_requests >= MAX_MODEL_REQUESTS {
            return Err(BudgetError::ModelRequests);
        }
        self.model_requests += 1;
        Ok(())
    }

    /// Reserve a whole tool batch. The batch is rejected as a unit when it does not fit.
    pub fn charge_tool_calls(&mut self, count: usize) -> Result<(), BudgetError> {
        let requested = u32::try_from(count).map_err(|_| BudgetError::ToolCalls)?;
        let used = self
            .tool_calls
            .checked_add(requested)
            .ok_or(BudgetError::ToolCalls)?;
        if used > MAX_TOOL_CALLS {
            return Err(BudgetError::ToolCalls);
        }
        self.tool_calls = used;
        Ok(())
    }

    pub fn remaining_tool_calls(&self) -> usize {
        usize::try_from(MAX_TOOL_CALLS.saturating_sub(self.tool_calls)).unwrap_or(0)
    }

    pub fn model_requests_used(&self) -> u32 {
        self.model_requests
    }

    pub fn tool_calls_used(&self) -> u32 {
        self.tool_calls
    }
}

/// Timeout for one model call: the smaller of the remaining turn budget and the per-call cap.
pub fn model_call_timeout(remaining_turn_budget: Duration) -> Duration {
    remaining_turn_budget.min(MAX_MODEL_CALL_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_requests_are_bounded_per_turn() {
        let mut budget = TurnBudget::new();
        for _ in 0..MAX_MODEL_REQUESTS {
            budget
                .charge_model_request()
                .expect("requests within the budget are allowed");
        }

        assert_eq!(
            budget.charge_model_request(),
            Err(BudgetError::ModelRequests)
        );
        assert_eq!(budget.model_requests_used(), MAX_MODEL_REQUESTS);
    }

    #[test]
    fn a_tool_batch_that_does_not_fit_is_rejected_as_a_unit() {
        let mut budget = TurnBudget::new();
        budget
            .charge_tool_calls(usize::try_from(MAX_TOOL_CALLS).expect("budget fits in usize") - 1)
            .expect("a batch within the budget is allowed");

        assert_eq!(budget.remaining_tool_calls(), 1);
        assert_eq!(budget.charge_tool_calls(2), Err(BudgetError::ToolCalls));
        assert_eq!(budget.tool_calls_used(), MAX_TOOL_CALLS - 1);
    }

    #[test]
    fn oversized_batch_counts_cannot_overflow_the_budget() {
        let mut budget = TurnBudget::new();

        assert_eq!(
            budget.charge_tool_calls(usize::MAX),
            Err(BudgetError::ToolCalls)
        );
        assert_eq!(budget.tool_calls_used(), 0);
    }

    #[test]
    fn a_model_call_never_outlives_the_turn_deadline() {
        assert_eq!(
            model_call_timeout(Duration::from_secs(5)),
            Duration::from_secs(5)
        );
        assert_eq!(
            model_call_timeout(MAX_TURN_WALL_CLOCK),
            MAX_MODEL_CALL_TIMEOUT
        );
    }
}
