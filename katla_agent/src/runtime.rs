use std::sync::mpsc;

use crate::llm::{ChatMessage, ChatResponse, LlmError, LlmProvider, StreamChunk, ToolDefinition};

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

/// A pending streaming chat request that yields chunks.
pub struct PendingStreamRequest {
    receiver: mpsc::Receiver<Result<StreamChunk, LlmError>>,
    done: bool,
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

    /// Submit a streaming chat completion request to the background runtime.
    pub fn submit_chat_stream(
        &self,
        provider: std::sync::Arc<dyn LlmProvider>,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
    ) -> PendingStreamRequest {
        let (tx, rx) = mpsc::channel();
        self.runtime.spawn(async move {
            use futures::StreamExt;
            let mut stream = std::pin::pin!(provider.chat_completion_stream(&messages, &tools));
            while let Some(item) = stream.next().await {
                if tx.send(item).is_err() {
                    break;
                }
            }
        });
        PendingStreamRequest {
            receiver: rx,
            done: false,
        }
    }
}

impl PendingChatRequest {
    /// Poll for the result. Returns None if not ready yet.
    pub fn poll(&self) -> Option<Result<ChatResponse, LlmError>> {
        self.receiver.try_recv().ok()
    }
}

impl PendingStreamRequest {
    /// Drain all available chunks from the receiver (non-blocking).
    pub fn poll_chunks(&mut self) -> Vec<Result<StreamChunk, LlmError>> {
        if self.done {
            return Vec::new();
        }
        let mut chunks = Vec::new();
        while let Ok(chunk) = self.receiver.try_recv() {
            let is_done = match &chunk {
                Ok(c) => c.finish_reason.is_some(),
                Err(_) => true,
            };
            if is_done {
                self.done = true;
            }
            chunks.push(chunk);
            if self.done {
                break;
            }
        }
        chunks
    }

    /// Returns true when the stream has ended or an error was received.
    pub fn is_done(&self) -> bool {
        self.done
    }
}
