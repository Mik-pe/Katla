use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{ChatMessage, ChatResponse, FinishReason, LlmError, LlmProvider, ToolDefinition};

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
