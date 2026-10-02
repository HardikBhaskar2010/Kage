//! Provider-neutral model abstraction for autonomous reasoning and structured tool calls.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::tool_catalog::ToolDefinition;

/// Error arising during model invocation.
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("Context limit exceeded: {tokens} tokens > {max_tokens}")]
    ContextLimitExceeded { tokens: usize, max_tokens: usize },
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Network or provider communication failure: {0}")]
    Network(String),
    #[error("Rate limited: retry after {retry_after_secs:?} seconds")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("Model cancelled by token")]
    Cancelled,
    #[error("Invalid model response: {0}")]
    InvalidResponse(String),
    #[error("Mock model error: {0}")]
    Mock(String),
}

/// Declared capabilities of an LLM provider / model instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub provider: String,
    pub model_id: String,
    pub context_window: usize,
    pub supports_tool_calling: bool,
    pub supports_vision: bool,
    pub is_local: bool,
}

/// Chat role in the model conversation transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Tool invocation proposed by the reasoning model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// Structured chat message in model context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub tool_calls: Vec<ProposedToolCall>,
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant_with_tools(
        content: impl Into<String>,
        tool_calls: Vec<ProposedToolCall>,
    ) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            tool_calls,
            tool_call_id: None,
        }
    }

    pub fn tool(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// Complete request dispatched to the reasoning model.
#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub messages: Vec<ChatMessage>,
    pub available_tools: Vec<ToolDefinition>,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
}

/// Token accounting report from model generation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Output returned by the reasoning model.
#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub message: ChatMessage,
    pub usage: TokenUsage,
    pub finish_reason: String,
}

/// Abstract reasoning provider interface.
#[async_trait]
pub trait AgentModel: Send + Sync {
    /// Return declared metadata and capability flags.
    fn capabilities(&self) -> &ModelCapabilities;

    /// Execute a chat completion with structured tool-calling support.
    async fn request(
        &self,
        req: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelResponse, ModelError>;
}

/// Deterministic scripted mock model for automated CI testing and verification gates.
pub struct MockAgentModel {
    capabilities: ModelCapabilities,
    queued_responses: Arc<Mutex<VecDeque<Result<ModelResponse, ModelError>>>>,
    recorded_requests: Arc<Mutex<Vec<ModelRequest>>>,
}

impl MockAgentModel {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            capabilities: ModelCapabilities {
                provider: "mock".to_string(),
                model_id: model_id.into(),
                context_window: 128_000,
                supports_tool_calling: true,
                supports_vision: false,
                is_local: true,
            },
            queued_responses: Arc::new(Mutex::new(VecDeque::new())),
            recorded_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Enqueue a scripted response.
    pub async fn enqueue_response(&self, response: ModelResponse) {
        self.queued_responses.lock().await.push_back(Ok(response));
    }

    /// Enqueue a simulated model error.
    pub async fn enqueue_error(&self, error: ModelError) {
        self.queued_responses.lock().await.push_back(Err(error));
    }

    /// Enqueue a message containing a proposed tool call.
    pub async fn enqueue_tool_call(
        &self,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) {
        let call_id = format!("call_{}", uuid::Uuid::new_v4());
        let resp = ModelResponse {
            message: ChatMessage::assistant_with_tools(
                "",
                vec![ProposedToolCall {
                    call_id,
                    tool_name: tool_name.into(),
                    arguments,
                }],
            ),
            usage: TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 25,
                total_tokens: 125,
            },
            finish_reason: "tool_calls".to_string(),
        };
        self.enqueue_response(resp).await;
    }

    /// Enqueue a final assistant text response (no tool calls -> goal achieved).
    pub async fn enqueue_text(&self, text: impl Into<String>) {
        let resp = ModelResponse {
            message: ChatMessage::assistant(text),
            usage: TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 20,
                total_tokens: 120,
            },
            finish_reason: "stop".to_string(),
        };
        self.enqueue_response(resp).await;
    }

    /// Inspect requests dispatched to this mock.
    pub async fn get_recorded_requests(&self) -> Vec<ModelRequest> {
        self.recorded_requests.lock().await.clone()
    }
}

#[async_trait]
impl AgentModel for MockAgentModel {
    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    async fn request(
        &self,
        req: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled);
        }
        self.recorded_requests.lock().await.push(req);

        let mut queue = self.queued_responses.lock().await;
        match queue.pop_front() {
            Some(res) => res,
            None => Err(ModelError::Mock("No scripted responses remaining in MockAgentModel queue".to_string())),
        }
    }
}
