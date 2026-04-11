use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{
    ChatMessage, ChatResponse, FinishReason, LlmError, LlmProvider, StreamChunk, ToolCallDelta,
    ToolDefinition,
};
use futures::Stream;

/// A mock LLM provider for testing. Returns predefined responses in order.
pub struct MockProvider {
    responses: Vec<ChatResponse>,
    call_index: AtomicUsize,
}

impl MockProvider {
    pub fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses,
            call_index: AtomicUsize::new(0),
        }
    }

    /// Create a provider that always returns a simple text response.
    pub fn simple(text: &str) -> Self {
        Self::new(vec![ChatResponse {
            message: ChatMessage {
                role: super::MessageRole::Assistant,
                content: text.to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: FinishReason::Stop,
        }])
    }
}

impl LlmProvider for MockProvider {
    fn chat_completion(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, LlmError>> + Send + '_>> {
        let index = self.call_index.fetch_add(1, Ordering::SeqCst);
        let response = self
            .responses
            .get(index)
            .cloned()
            .ok_or_else(|| LlmError::Api("No more mock responses available".to_string()));
        Box::pin(async move { response })
    }
}

/// A mock LLM provider that streams predefined chunks.
pub struct MockStreamProvider {
    chunks: Vec<Result<StreamChunk, LlmError>>,
}

impl MockStreamProvider {
    pub fn new(chunks: Vec<Result<StreamChunk, LlmError>>) -> Self {
        Self { chunks }
    }

    /// Create a provider that streams text in chunks, then finishes with Stop.
    pub fn text_chunks(text_parts: &[&str]) -> Self {
        let mut chunks: Vec<Result<StreamChunk, LlmError>> = text_parts
            .iter()
            .map(|part| {
                Ok(StreamChunk {
                    content_delta: part.to_string(),
                    finish_reason: None,
                    tool_call_deltas: Vec::new(),
                })
            })
            .collect();
        chunks.push(Ok(StreamChunk {
            content_delta: String::new(),
            finish_reason: Some(FinishReason::Stop),
            tool_call_deltas: Vec::new(),
        }));
        Self::new(chunks)
    }

    /// Create a provider that streams a tool call with the given name and arguments.
    ///
    /// The `arguments` value is serialized to a JSON string for the streaming delta.
    pub fn tool_call(id: &str, name: &str, arguments: &impl serde::Serialize) -> Self {
        let args_str = serde_json::to_string(arguments).unwrap_or_default();
        Self::new(vec![
            Ok(StreamChunk {
                content_delta: String::new(),
                finish_reason: None,
                tool_call_deltas: vec![ToolCallDelta {
                    index: 0,
                    id: Some(id.to_string()),
                    name: Some(name.to_string()),
                    arguments_delta: None,
                }],
            }),
            Ok(StreamChunk {
                content_delta: String::new(),
                finish_reason: None,
                tool_call_deltas: vec![ToolCallDelta {
                    index: 0,
                    id: None,
                    name: None,
                    arguments_delta: Some(args_str),
                }],
            }),
            Ok(StreamChunk {
                content_delta: String::new(),
                finish_reason: Some(FinishReason::ToolCall),
                tool_call_deltas: Vec::new(),
            }),
        ])
    }

    /// Create a provider that streams an error.
    pub fn error(msg: &str) -> Self {
        Self::new(vec![Err(LlmError::Api(msg.to_string()))])
    }
}

impl LlmProvider for MockStreamProvider {
    fn chat_completion(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, LlmError>> + Send + '_>> {
        Box::pin(async {
            Err(LlmError::Api(
                "MockStreamProvider does not support non-streaming".into(),
            ))
        })
    }

    fn chat_completion_stream(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send + '_>> {
        let chunks = self.chunks.clone();
        Box::pin(futures::stream::iter(chunks))
    }
}
