//! Microsoft Foundry Model Router data-plane adapter.
//!
//! The adapter uses the stable Azure OpenAI Chat Completions API with only the Container App's
//! system-assigned managed identity. It performs no retries, follows no redirects, bounds every
//! response, and maps upstream failures to the sanitized model error contract.

use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use azure_core::credentials::TokenCredential;
use azure_identity::ManagedIdentityCredential;
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{APP_VERSION, FOUNDRY_API_VERSION};

use super::model::{
    ModelClient, ModelError, ModelOutput, ModelTurnRequest, ModelTurnResponse, ProposedToolCall,
    ToolSchema, TranscriptMessage,
};

pub const FOUNDRY_TOKEN_SCOPE: &str = "https://cognitiveservices.azure.com/.default";
const MAX_MODEL_RESPONSE_BYTES: usize = 1024 * 1024;
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone)]
pub struct FoundryModelClient {
    client: Client,
    credential: Arc<dyn TokenCredential>,
    chat_completions_url: Url,
}

impl FoundryModelClient {
    /// Create a fail-closed client for one approved deployment and the pinned stable API.
    pub fn new(endpoint: Url, deployment: &str, api_version: &str) -> Result<Self, ModelError> {
        if api_version != FOUNDRY_API_VERSION
            || !valid_endpoint(&endpoint)
            || !valid_deployment_name(deployment)
        {
            return Err(ModelError::Unavailable);
        }

        let mut chat_completions_url = endpoint;
        chat_completions_url.set_path(&format!(
            "/openai/deployments/{deployment}/chat/completions"
        ));
        chat_completions_url
            .query_pairs_mut()
            .append_pair("api-version", api_version);

        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .https_only(true)
            .no_proxy()
            .redirect(Policy::none())
            .user_agent(format!("azure-sql-tco/{APP_VERSION}"))
            .build()
            .map_err(|_| ModelError::Unavailable)?;
        let credential: Arc<dyn TokenCredential> =
            ManagedIdentityCredential::new(None).map_err(|_| ModelError::Unavailable)?;

        Ok(Self {
            client,
            credential,
            chat_completions_url,
        })
    }
}

#[async_trait]
impl ModelClient for FoundryModelClient {
    async fn respond(&self, request: ModelTurnRequest) -> Result<ModelTurnResponse, ModelError> {
        let started_at = Instant::now();
        let body = encode_request(&request)?;
        let token = tokio::time::timeout(
            request.timeout,
            self.credential.get_token(&[FOUNDRY_TOKEN_SCOPE], None),
        )
        .await
        .map_err(|_| ModelError::Timeout)?
        .map_err(|_| ModelError::Unavailable)?;
        let remaining = request
            .timeout
            .checked_sub(started_at.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(ModelError::Timeout)?;

        let mut response = self
            .client
            .post(self.chat_completions_url.clone())
            .bearer_auth(token.token.secret())
            .json(&body)
            .timeout(remaining)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_MODEL_RESPONSE_BYTES as u64)
        {
            return Err(ModelError::MalformedResponse);
        }

        let mut response_body = Vec::with_capacity(
            response
                .content_length()
                .unwrap_or_default()
                .min(MAX_MODEL_RESPONSE_BYTES as u64) as usize,
        );
        while let Some(chunk) = response.chunk().await.map_err(map_reqwest_error)? {
            let next_length = response_body
                .len()
                .checked_add(chunk.len())
                .ok_or(ModelError::MalformedResponse)?;
            if next_length > MAX_MODEL_RESPONSE_BYTES {
                return Err(ModelError::MalformedResponse);
            }
            response_body.extend_from_slice(&chunk);
        }

        if !status.is_success() {
            return Err(classify_status(status, &response_body));
        }
        parse_response(&response_body)
    }
}

fn valid_endpoint(endpoint: &Url) -> bool {
    endpoint.scheme() == "https"
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && endpoint.port().is_none()
        && endpoint.path() == "/"
        && endpoint.query().is_none()
        && endpoint.fragment().is_none()
        && endpoint.host_str().is_some_and(|host| {
            host.ends_with(".openai.azure.com") || host.ends_with(".services.ai.azure.com")
        })
}

fn valid_deployment_name(deployment: &str) -> bool {
    (1..=64).contains(&deployment.len())
        && deployment
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, b'-' | b'_'))
}

fn map_reqwest_error(error: reqwest::Error) -> ModelError {
    if error.is_timeout() {
        ModelError::Timeout
    } else {
        ModelError::Transport
    }
}

fn classify_status(status: StatusCode, body: &[u8]) -> ModelError {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return ModelError::QuotaExceeded;
    }
    if matches!(
        status,
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT
    ) {
        return ModelError::Timeout;
    }
    if status == StatusCode::BAD_REQUEST
        && service_error_code(body).is_some_and(|code| {
            let code = code.to_ascii_lowercase();
            code.contains("content_filter") || code.contains("responsibleai")
        })
    {
        return ModelError::ContentFiltered;
    }
    if matches!(
        status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ) {
        return ModelError::Unavailable;
    }
    if status.is_server_error() {
        ModelError::Transport
    } else {
        ModelError::MalformedResponse
    }
}

fn service_error_code(body: &[u8]) -> Option<String> {
    let body: Value = serde_json::from_slice(body).ok()?;
    body.pointer("/error/code")
        .or_else(|| body.get("code"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool<'a>>,
    max_completion_tokens: u32,
    n: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

#[derive(Serialize)]
#[serde(tag = "role")]
enum WireMessage<'a> {
    #[serde(rename = "system")]
    System { content: &'a str },
    #[serde(rename = "user")]
    User { content: &'a str },
    #[serde(rename = "assistant")]
    Assistant { content: &'a str },
    #[serde(rename = "assistant")]
    AssistantToolCalls { tool_calls: Vec<WireToolCall<'a>> },
    #[serde(rename = "tool")]
    Tool {
        tool_call_id: &'a str,
        content: &'a str,
    },
}

#[derive(Serialize)]
struct WireToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunctionCall<'a>,
}

#[derive(Serialize)]
struct WireFunctionCall<'a> {
    name: &'a str,
    arguments: &'a str,
}

#[derive(Serialize)]
struct WireTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunction<'a>,
}

#[derive(Serialize)]
struct WireFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: Value,
}

fn encode_request(request: &ModelTurnRequest) -> Result<Value, ModelError> {
    let mut messages = Vec::with_capacity(request.messages.len() + 1);
    messages.push(WireMessage::System {
        content: request.system_instruction,
    });
    for message in &request.messages {
        messages.push(match message {
            TranscriptMessage::User { content } => WireMessage::User { content },
            TranscriptMessage::Assistant { content } => WireMessage::Assistant { content },
            TranscriptMessage::AssistantToolCalls { calls } => WireMessage::AssistantToolCalls {
                tool_calls: calls
                    .iter()
                    .map(|call| WireToolCall {
                        id: &call.id,
                        kind: "function",
                        function: WireFunctionCall {
                            name: &call.name,
                            arguments: &call.arguments,
                        },
                    })
                    .collect(),
            },
            TranscriptMessage::ToolResult {
                call_id, content, ..
            } => WireMessage::Tool {
                tool_call_id: call_id,
                content,
            },
        });
    }

    let tools = request
        .tools
        .iter()
        .map(wire_tool)
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_value(ChatCompletionRequest {
        messages,
        parallel_tool_calls: (!tools.is_empty()).then_some(true),
        tools,
        max_completion_tokens: request.max_output_tokens,
        n: 1,
    })
    .map_err(|_| ModelError::MalformedResponse)
}

fn wire_tool(schema: &ToolSchema) -> Result<WireTool<'_>, ModelError> {
    let parameters =
        serde_json::from_str(schema.parameters).map_err(|_| ModelError::MalformedResponse)?;
    Ok(WireTool {
        kind: "function",
        function: WireFunction {
            name: schema.name,
            description: schema.description,
            parameters,
        },
    })
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ChatCompletionEnvelope {
    Direct(ChatCompletionResponse),
    Wrapped { data: ChatCompletionResponse },
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    model: String,
    #[serde(default)]
    prompt_filter_results: Vec<Value>,
}

#[derive(Deserialize)]
struct ChatChoice {
    finish_reason: String,
    message: ChatMessage,
    #[serde(default)]
    content_filter_results: Value,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
    #[serde(default)]
    refusal: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCall>,
}

#[derive(Deserialize)]
struct ChatToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: ChatFunctionCall,
}

#[derive(Deserialize)]
struct ChatFunctionCall {
    name: String,
    arguments: String,
}

fn parse_response(body: &[u8]) -> Result<ModelTurnResponse, ModelError> {
    let response = match serde_json::from_slice::<ChatCompletionEnvelope>(body)
        .map_err(|_| ModelError::MalformedResponse)?
    {
        ChatCompletionEnvelope::Direct(response)
        | ChatCompletionEnvelope::Wrapped { data: response } => response,
    };
    if response.choices.len() != 1
        || !valid_routed_model(&response.model)
        || response
            .prompt_filter_results
            .iter()
            .any(has_filtered_value)
    {
        return Err(ModelError::MalformedResponse);
    }

    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or(ModelError::MalformedResponse)?;
    if has_filtered_value(&choice.content_filter_results)
        || choice
            .message
            .refusal
            .as_deref()
            .is_some_and(|refusal| !refusal.trim().is_empty())
    {
        return Err(ModelError::ContentFiltered);
    }

    let output = match choice.finish_reason.as_str() {
        "stop" => {
            if !choice.message.tool_calls.is_empty() {
                return Err(ModelError::MalformedResponse);
            }
            let content = choice
                .message
                .content
                .ok_or(ModelError::MalformedResponse)?;
            if content.trim().is_empty() {
                return Err(ModelError::MalformedResponse);
            }
            ModelOutput::Message(content)
        }
        "tool_calls" => {
            if choice.message.tool_calls.is_empty()
                || choice
                    .message
                    .content
                    .as_deref()
                    .is_some_and(|content| !content.trim().is_empty())
            {
                return Err(ModelError::MalformedResponse);
            }
            ModelOutput::ToolCalls(
                choice
                    .message
                    .tool_calls
                    .into_iter()
                    .map(|call| {
                        if call.kind != "function" {
                            return Err(ModelError::MalformedResponse);
                        }
                        Ok(ProposedToolCall {
                            id: call.id,
                            name: call.function.name,
                            arguments: call.function.arguments,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }
        "content_filter" => return Err(ModelError::ContentFiltered),
        _ => return Err(ModelError::MalformedResponse),
    };

    Ok(ModelTurnResponse {
        output,
        routed_model: Some(response.model),
    })
}

fn valid_routed_model(model: &str) -> bool {
    (1..=128).contains(&model.len()) && !model.chars().any(char::is_control)
}

fn has_filtered_value(value: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("filtered") == Some(&Value::Bool(true))
                || fields.values().any(has_filtered_value)
        }
        Value::Array(values) => values.iter().any(has_filtered_value),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn request() -> ModelTurnRequest {
        ModelTurnRequest {
            system_instruction: "Use tools.",
            prompt_version: "test/1",
            messages: vec![
                TranscriptMessage::User {
                    content: "Explain Azure region".to_owned(),
                },
                TranscriptMessage::AssistantToolCalls {
                    calls: vec![ProposedToolCall {
                        id: "call-1".to_owned(),
                        name: "get_application_help".to_owned(),
                        arguments: r#"{"question":"Azure region"}"#.to_owned(),
                    }],
                },
                TranscriptMessage::ToolResult {
                    call_id: "call-1".to_owned(),
                    tool_name: "get_application_help",
                    content: r#"{"status":"ok"}"#.to_owned(),
                },
            ],
            tools: vec![ToolSchema {
                name: "get_application_help",
                description: "Read reviewed help.",
                parameters: r#"{"type":"object","additionalProperties":false}"#,
            }],
            max_output_tokens: 4000,
            timeout: Duration::from_secs(10),
        }
    }

    #[test]
    fn endpoint_and_deployment_are_allowlisted() {
        assert_eq!(
            FOUNDRY_TOKEN_SCOPE,
            "https://cognitiveservices.azure.com/.default"
        );
        assert!(valid_endpoint(
            &Url::parse("https://tco.openai.azure.com/").expect("URL")
        ));
        assert!(valid_endpoint(
            &Url::parse("https://tco.services.ai.azure.com/").expect("URL")
        ));
        assert!(!valid_endpoint(
            &Url::parse("https://example.invalid/").expect("URL")
        ));
        assert!(!valid_endpoint(
            &Url::parse("https://tco.openai.azure.com/other").expect("URL")
        ));
        assert!(valid_deployment_name("tco-model-router"));
        assert!(!valid_deployment_name("../deployment"));
    }

    #[test]
    fn request_uses_structured_roles_and_closed_tool_schema() {
        let body = encode_request(&request()).expect("request should serialize");

        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][2]["role"], "assistant");
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["messages"][3]["tool_call_id"], "call-1");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(
            body["tools"][0]["function"]["parameters"]["additionalProperties"],
            false
        );
        assert_eq!(body["max_completion_tokens"], 4000);
        assert_eq!(body["n"], 1);
        assert_eq!(body["parallel_tool_calls"], true);
        assert!(body.get("model").is_none());
    }

    #[test]
    fn terminal_response_records_the_actual_routed_model() {
        let response = parse_response(
            br#"{"choices":[{"finish_reason":"stop","message":{"content":"Use South Africa North.","tool_calls":[]}}],"model":"gpt-4.1-mini-2025-04-14"}"#,
        )
        .expect("valid terminal response");

        assert_eq!(
            response,
            ModelTurnResponse {
                output: ModelOutput::Message("Use South Africa North.".to_owned()),
                routed_model: Some("gpt-4.1-mini-2025-04-14".to_owned()),
            }
        );
    }

    #[test]
    fn tool_calls_remain_untrusted_raw_arguments() {
        let response = parse_response(
            br#"{"choices":[{"finish_reason":"tool_calls","message":{"content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"get_application_help","arguments":"{\"question\":\"Azure region\"}"}}]}}],"model":"gpt-5-mini-2025-08-07"}"#,
        )
        .expect("valid tool response");

        assert_eq!(
            response.output,
            ModelOutput::ToolCalls(vec![ProposedToolCall {
                id: "call-1".to_owned(),
                name: "get_application_help".to_owned(),
                arguments: r#"{"question":"Azure region"}"#.to_owned(),
            }])
        );
    }

    #[test]
    fn guardrail_and_ambiguous_outputs_fail_closed() {
        let filtered = br#"{"choices":[{"finish_reason":"content_filter","message":{"content":null}}],"model":"gpt-4.1-mini"}"#;
        assert_eq!(parse_response(filtered), Err(ModelError::ContentFiltered));

        let mixed = br#"{"choices":[{"finish_reason":"stop","message":{"content":"done","tool_calls":[{"id":"call-1","type":"function","function":{"name":"tool","arguments":"{}"}}]}}],"model":"gpt-4.1-mini"}"#;
        assert_eq!(parse_response(mixed), Err(ModelError::MalformedResponse));

        let filtered_result = br#"{"choices":[{"finish_reason":"stop","content_filter_results":{"hate":{"filtered":true}},"message":{"content":"redacted"}}],"model":"gpt-4.1-mini"}"#;
        assert_eq!(
            parse_response(filtered_result),
            Err(ModelError::ContentFiltered)
        );
    }

    #[test]
    fn upstream_statuses_map_to_sanitized_errors() {
        assert_eq!(
            classify_status(StatusCode::TOO_MANY_REQUESTS, b"anything"),
            ModelError::QuotaExceeded
        );
        assert_eq!(
            classify_status(
                StatusCode::BAD_REQUEST,
                br#"{"error":{"code":"content_filter"}}"#
            ),
            ModelError::ContentFiltered
        );
        assert_eq!(
            classify_status(StatusCode::FORBIDDEN, b"secret upstream detail"),
            ModelError::Unavailable
        );
        assert_eq!(
            classify_status(StatusCode::SERVICE_UNAVAILABLE, b"internal detail"),
            ModelError::Transport
        );
    }
}
