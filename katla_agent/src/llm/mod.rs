pub mod mock;
pub mod openai;

pub use mock::MockProvider;
pub use openai::OpenAiProvider;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// A message in the LLM conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool call from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Definition of a tool the LLM can call.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Response from the LLM.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub message: ChatMessage,
    pub finish_reason: FinishReason,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FinishReason {
    Stop,
    ToolCall,
    /// Response was truncated due to token limit.
    Length,
}

/// Trait for LLM providers (OpenAI, local, mock, etc.).
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request with optional tool definitions.
    fn chat_completion(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, LlmError>> + Send + '_>>;
}

/// Error type for LLM operations.
#[derive(Debug)]
pub enum LlmError {
    Network(String),
    Api(String),
    Serialization(String),
    Timeout,
    Config(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::Network(s) => write!(f, "Network error: {s}"),
            LlmError::Api(s) => write!(f, "API error: {s}"),
            LlmError::Serialization(s) => write!(f, "Serialization error: {s}"),
            LlmError::Timeout => write!(f, "Request timed out"),
            LlmError::Config(s) => write!(f, "Configuration error: {s}"),
        }
    }
}

impl std::error::Error for LlmError {}
