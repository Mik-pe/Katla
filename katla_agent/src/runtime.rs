use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use crate::llm::{ChatMessage, ChatResponse, LlmError, LlmProvider, StreamChunk, ToolDefinition};
use crate::rate_limiter::{RateLimitResult, RateLimiter};

/// Bridge between the synchronous render loop and async LLM calls.
///
/// Runs a tokio runtime on a background thread. The main thread sends
/// requests and polls for results each frame.
pub struct AsyncBridge {
    runtime: tokio::runtime::Runtime,
    rate_limiter: Option<Arc<RateLimiter>>,
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
        Ok(Self {
            runtime,
            rate_limiter: None,
        })
    }

    pub fn with_rate_limits(
        min_interval: Duration,
        max_calls_per_minute: u32,
    ) -> Result<Self, LlmError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| LlmError::Config(format!("Failed to create tokio runtime: {e}")))?;
        Ok(Self {
            runtime,
            rate_limiter: Some(Arc::new(RateLimiter::new(
                min_interval,
                max_calls_per_minute,
            ))),
        })
    }

    /// Submit a chat completion request to the background runtime.
    pub fn submit_chat(
        &self,
        provider: std::sync::Arc<dyn LlmProvider>,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
    ) -> PendingChatRequest {
        let (tx, rx) = mpsc::channel();
        let rate_limiter = self.rate_limiter.clone();
        self.runtime.spawn(async move {
            if let Some(ref limiter) = rate_limiter {
                match limiter.check_and_record() {
                    RateLimitResult::Allowed => {}
                    RateLimitResult::Wait(duration) => {
                        log::warn!(
                            "LLM rate limit: waiting {:.0}ms before next call",
                            duration.as_millis()
                        );
                        tokio::time::sleep(duration).await;
                        limiter.record();
                    }
                    RateLimitResult::Exceeded { retry_after } => {
                        let _ = tx.send(Err(LlmError::RateLimited(format!(
                            "Max calls per minute exceeded. Retry after {:.0}s",
                            retry_after.as_secs_f32()
                        ))));
                        return;
                    }
                }
            }

            let result = tokio::time::timeout(
                Duration::from_secs(120),
                provider.chat_completion(&messages, &tools),
            )
            .await
            .map_err(|_| LlmError::Timeout)
            .and_then(|r| r);
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
        let rate_limiter = self.rate_limiter.clone();
        self.runtime.spawn(async move {
            if let Some(ref limiter) = rate_limiter {
                match limiter.check_and_record() {
                    RateLimitResult::Allowed => {}
                    RateLimitResult::Wait(duration) => {
                        log::warn!(
                            "LLM rate limit: waiting {:.0}ms before next call",
                            duration.as_millis()
                        );
                        tokio::time::sleep(duration).await;
                        limiter.record();
                    }
                    RateLimitResult::Exceeded { retry_after } => {
                        let _ = tx.send(Err(LlmError::RateLimited(format!(
                            "Max calls per minute exceeded. Retry after {:.0}s",
                            retry_after.as_secs_f32()
                        ))));
                        return;
                    }
                }
            }

            use futures::StreamExt;
            let mut stream = std::pin::pin!(provider.chat_completion_stream(&messages, &tools));
            loop {
                let next = tokio::time::timeout(Duration::from_secs(30), stream.next()).await;
                let item = match next {
                    Ok(Some(item)) => item,
                    Ok(None) => break,
                    Err(_) => Err(LlmError::Timeout),
                };
                let is_done = match &item {
                    Ok(c) => c.finish_reason.is_some(),
                    Err(_) => true,
                };
                if tx.send(item).is_err() {
                    break;
                }
                if is_done {
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
