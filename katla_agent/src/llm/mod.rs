pub mod mock;
pub mod openai;

pub use mock::{MockProvider, MockStreamProvider};
pub use openai::OpenAiProvider;

use futures::Stream;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
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

/// A delta fragment for a tool call being streamed.
#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_delta: Option<String>,
}

/// A single chunk from a streaming chat completion response.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content_delta: String,
    pub finish_reason: Option<FinishReason>,
    pub tool_call_deltas: Vec<ToolCallDelta>,
}

/// Trait for LLM providers (OpenAI, local, mock, etc.).
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request with optional tool definitions.
    fn chat_completion(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, LlmError>> + Send + '_>>;

    /// Send a streaming chat completion request.
    ///
    /// Default implementation falls back to a non-streaming call, returning the
    /// full response as a single chunk.
    fn chat_completion_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send + '_>> {
        let future = self.chat_completion(messages, tools);
        Box::pin(futures::stream::once(async move {
            let response = future.await?;
            Ok(StreamChunk {
                content_delta: response.message.content,
                finish_reason: Some(response.finish_reason),
                tool_call_deltas: Vec::new(),
            })
        }))
    }
}

/// Error type for LLM operations.
#[derive(Debug, Clone)]
pub enum LlmError {
    Network(String),
    Api(String),
    Serialization(String),
    Timeout,
    Config(String),
    RateLimited(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::Network(s) => write!(f, "Network error: {s}"),
            LlmError::Api(s) => write!(f, "API error: {s}"),
            LlmError::Serialization(s) => write!(f, "Serialization error: {s}"),
            LlmError::Timeout => write!(f, "Request timed out"),
            LlmError::Config(s) => write!(f, "Configuration error: {s}"),
            LlmError::RateLimited(s) => write!(f, "Rate limited: {s}"),
        }
    }
}

impl std::error::Error for LlmError {}
