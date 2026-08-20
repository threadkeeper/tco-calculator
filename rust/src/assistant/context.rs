//! Immutable, host-owned context for one assistant turn.
//!
//! Identity, owner scope, project selection, ETags, price-snapshot identifiers, and user
//! confirmations live here and are never model-visible. Tools read them from this context
//! instead of from tool arguments so a model cannot widen its own authorization.

use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::domain::resource::ProjectType;

use super::budget::MAX_TURN_WALL_CLOCK;

/// Phase of a bounded assistant turn.
///
/// Tool availability is fixed per phase so a read-only request cannot reach a mutating
/// capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnPhase {
    /// Reviewed help, owner-scoped reads, validation, and deterministic calculation.
    ReadPlan,
    /// Reversible browser-draft proposals that still require a user preview.
    Propose,
    /// Explicitly confirmed side effects.
    Execute,
}

/// The owner-scoped project the host selected for this turn.
///
/// Price-snapshot identifiers are host-owned. A model may never choose or supply them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedProject {
    pub id: Uuid,
    pub etag: String,
    pub aws_price_snapshot_id: Option<String>,
    pub azure_price_snapshot_id: Option<String>,
}

/// Server-derived context supplied to every tool invocation in a turn.
#[derive(Clone, Debug)]
pub struct TurnContext {
    owner_id: String,
    request_id: Uuid,
    phase: TurnPhase,
    project: Option<SelectedProject>,
    classified_project_type: Option<ProjectType>,
    classification_model_requests: u32,
    confirmed_actions: Vec<String>,
    started_at: Instant,
    wall_clock: Duration,
}

impl TurnContext {
    /// Build a turn context from the already-authenticated principal's owner identifier.
    pub fn new(owner_id: impl Into<String>, request_id: Uuid, phase: TurnPhase) -> Self {
        Self {
            owner_id: owner_id.into(),
            request_id,
            phase,
            project: None,
            classified_project_type: None,
            classification_model_requests: 0,
            confirmed_actions: Vec::new(),
            started_at: Instant::now(),
            wall_clock: MAX_TURN_WALL_CLOCK,
        }
    }

    /// Attach the owner-scoped project the server resolved for this turn.
    #[must_use]
    pub fn with_project(mut self, project: SelectedProject) -> Self {
        self.project = Some(project);
        self
    }

    /// Attach the host-validated image classification that constrains a new draft.
    #[must_use]
    pub fn with_classified_project_type(mut self, project_type: ProjectType) -> Self {
        self.classified_project_type = Some(project_type);
        self.classification_model_requests = 1;
        self
    }

    /// Attach a classification and the model requests the host actually spent obtaining it.
    #[must_use]
    pub fn with_classification_usage(
        mut self,
        project_type: ProjectType,
        model_requests: u32,
    ) -> Self {
        self.classified_project_type = Some(project_type);
        self.classification_model_requests = model_requests;
        self
    }

    /// Record an action identifier the user explicitly confirmed in the browser.
    #[must_use]
    pub fn with_confirmed_action(mut self, action_id: impl Into<String>) -> Self {
        self.confirmed_actions.push(action_id.into());
        self
    }

    /// Shorten the whole-turn deadline. The approved maximum is never exceeded.
    #[must_use]
    pub fn with_wall_clock(mut self, wall_clock: Duration) -> Self {
        self.wall_clock = wall_clock.min(MAX_TURN_WALL_CLOCK);
        self
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub fn phase(&self) -> TurnPhase {
        self.phase
    }

    pub fn project(&self) -> Option<&SelectedProject> {
        self.project.as_ref()
    }

    pub fn classified_project_type(&self) -> Option<ProjectType> {
        self.classified_project_type
    }

    pub fn classification_model_requests(&self) -> u32 {
        self.classification_model_requests
    }

    /// Report whether the user confirmed this exact action identifier for this turn.
    pub fn is_confirmed(&self, action_id: &str) -> bool {
        self.confirmed_actions
            .iter()
            .any(|confirmed| confirmed == action_id)
    }

    /// Time left before the whole-turn deadline expires.
    pub fn remaining(&self) -> Duration {
        self.wall_clock.saturating_sub(self.started_at.elapsed())
    }

    pub fn is_expired(&self) -> bool {
        self.remaining().is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> TurnContext {
        TurnContext::new("entra:tenant:owner", Uuid::nil(), TurnPhase::ReadPlan)
    }

    #[test]
    fn wall_clock_cannot_exceed_the_approved_maximum() {
        let context = context().with_wall_clock(Duration::from_secs(3_600));

        assert!(context.remaining() <= MAX_TURN_WALL_CLOCK);
    }

    #[test]
    fn a_zero_wall_clock_expires_immediately() {
        let context = context().with_wall_clock(Duration::ZERO);

        assert!(context.is_expired());
        assert_eq!(context.remaining(), Duration::ZERO);
    }

    #[test]
    fn only_explicitly_confirmed_actions_are_reported_as_confirmed() {
        let context = context().with_confirmed_action("delete_project:1");

        assert!(context.is_confirmed("delete_project:1"));
        assert!(!context.is_confirmed("delete_project:2"));
        assert!(!context.is_confirmed(""));
    }

    #[test]
    fn a_turn_starts_without_a_project_or_confirmation() {
        let context = context();

        assert!(context.project().is_none());
        assert!(context.classified_project_type().is_none());
        assert!(!context.is_confirmed("apply_confirmed_project_patch"));
        assert_eq!(context.phase(), TurnPhase::ReadPlan);
    }
}
