//! Protocol-neutral model boundary.
//!
//! The runtime depends on this trait rather than on a provider SDK, endpoint, or credential.
//! Transcript roles carry the trust boundary structurally: only the system instruction is an
//! instruction, and user and tool messages are always data.

use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

/// One entry in the bounded turn transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptMessage {
    /// Untrusted text the signed-in user typed.
    User { content: String },
    /// Model prose from an earlier iteration of the same turn.
    Assistant { content: String },
    /// Tool calls the model proposed, recorded so tool results have a parent.
    AssistantToolCalls { calls: Vec<ProposedToolCall> },
    /// Untrusted, host-produced, bounded result of one executed tool call.
    ToolResult {
        call_id: String,
        tool_name: &'static str,
        content: String,
    },
}

impl TranscriptMessage {
    /// Character cost of this message against the prompt-context budget.
    pub fn character_count(&self) -> usize {
        match self {
            Self::User { content } | Self::Assistant { content } => content.chars().count(),
            Self::AssistantToolCalls { calls } => calls
                .iter()
                .map(|call| call.name.chars().count() + call.arguments.chars().count())
                .sum(),
            Self::ToolResult {
                tool_name, content, ..
            } => tool_name.chars().count() + content.chars().count(),
        }
    }
}

/// A tool call as returned by the model. Both fields are untrusted until preflight accepts them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposedToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON text. It is parsed against a closed typed schema, never evaluated.
    pub arguments: String,
}

/// The subset of a tool definition that is safe to expose to a model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolSchema {
    pub name: &'static str,
    pub description: &'static str,
    /// Closed JSON Schema for the tool arguments.
    pub parameters: &'static str,
}

/// One model call within a turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelTurnRequest {
    pub system_instruction: &'static str,
    pub prompt_version: &'static str,
    pub messages: Vec<TranscriptMessage>,
    pub tools: Vec<ToolSchema>,
    pub max_output_tokens: u32,
    pub timeout: Duration,
}

/// Terminal prose or a proposed tool batch. The host never receives both.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelOutput {
    Message(String),
    ToolCalls(Vec<ProposedToolCall>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelTurnResponse {
    pub output: ModelOutput,
    /// Actual routed model reported by the service, recorded for audit only.
    pub routed_model: Option<String>,
}

/// Stable, sanitized failure codes. No upstream body, header, or endpoint is carried.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ModelError {
    #[error("no approved model deployment is configured")]
    Unavailable,
    #[error("the model call exceeded its timeout")]
    Timeout,
    #[error("the model service could not be reached")]
    Transport,
    #[error("the model returned output the host could not parse")]
    MalformedResponse,
    #[error("the model service filtered the request or response")]
    ContentFiltered,
    #[error("the model deployment quota is exhausted")]
    QuotaExceeded,
}

impl ModelError {
    /// Whether a retry is safe. A retry is only ever attempted before any side effect.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::Transport)
    }
}

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn respond(&self, request: ModelTurnRequest) -> Result<ModelTurnResponse, ModelError>;
}

/// Fail-closed client used whenever no approved model deployment is configured.
#[derive(Clone, Copy, Debug, Default)]
pub struct DisabledModelClient;

#[async_trait]
impl ModelClient for DisabledModelClient {
    async fn respond(&self, _request: ModelTurnRequest) -> Result<ModelTurnResponse, ModelError> {
        Err(ModelError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_default_client_fails_closed() {
        let client = DisabledModelClient;
        let request = ModelTurnRequest {
            system_instruction: "",
            prompt_version: "test",
            messages: vec![TranscriptMessage::User {
                content: "What does the Azure region control?".to_owned(),
            }],
            tools: Vec::new(),
            max_output_tokens: 1,
            timeout: Duration::from_secs(1),
        };

        assert_eq!(
            client.respond(request).await,
            Err(ModelError::Unavailable),
            "no model call may succeed without an explicitly configured client"
        );
    }

    #[test]
    fn only_transport_failures_are_retryable() {
        assert!(ModelError::Timeout.is_retryable());
        assert!(ModelError::Transport.is_retryable());
        assert!(!ModelError::ContentFiltered.is_retryable());
        assert!(!ModelError::MalformedResponse.is_retryable());
        assert!(!ModelError::QuotaExceeded.is_retryable());
        assert!(!ModelError::Unavailable.is_retryable());
    }

    #[test]
    fn transcript_messages_report_their_context_cost() {
        let message = TranscriptMessage::ToolResult {
            call_id: "call-1".to_owned(),
            tool_name: "get_application_help",
            content: "abcd".to_owned(),
        };

        assert_eq!(
            message.character_count(),
            "get_application_help".len() + "abcd".len()
        );
    }
}
