use std::sync::mpsc;

use crate::llm::{ChatMessage, ChatResponse, LlmError, LlmProvider, ToolDefinition};

/// Bridge between the synchronous render loop and async LLM calls.
///
/// Runs a tokio runtime on a background thread. The main thread sends
/// requests and polls for results each frame.
pub struct AsyncBridge {
    runtime: tokio::runtime::Runtime,
}

/// A pending chat request that can be polled for completion.
pub struct PendingChatRequest {
    receiver: mpsc::Receiver<Result<ChatResponse, LlmError>>,
}

impl AsyncBridge {
    pub fn new() -> Result<Self, LlmError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| LlmError::Config(format!("Failed to create tokio runtime: {e}")))?;
        Ok(Self { runtime })
    }

    /// Submit a chat completion request to the background runtime.
    pub fn submit_chat(
        &self,
        provider: std::sync::Arc<dyn LlmProvider>,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
    ) -> PendingChatRequest {
        let (tx, rx) = mpsc::channel();
        self.runtime.spawn(async move {
            let result = provider.chat_completion(&messages, &tools).await;
            let _ = tx.send(result);
        });
        PendingChatRequest { receiver: rx }
    }
}

impl PendingChatRequest {
    /// Poll for the result. Returns None if not ready yet.
    pub fn poll(&self) -> Option<Result<ChatResponse, LlmError>> {
        self.receiver.try_recv().ok()
    }
}
