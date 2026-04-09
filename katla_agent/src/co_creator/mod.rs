mod local_handler;
mod prompt;

pub use prompt::build_system_prompt;

use crate::llm::{ChatMessage, FinishReason, MessageRole};
use crate::runtime::PendingStreamRequest;

#[cfg(feature = "llm-assistant")]
mod tools;

#[cfg(feature = "llm-assistant")]
pub use tools::build_tool_definitions;

/// An agent driven by an LLM that assists with content creation.
///
/// Single source of truth for conversation history and LLM interaction.
/// The app submits requests, polls for streaming chunks, and reads history.
pub struct CoCreatorAgent {
    /// Typed conversation history.
    history: Vec<ChatMessage>,
    /// Pending LLM streaming request, if any.
    pending_stream: Option<PendingStreamRequest>,
}

impl CoCreatorAgent {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            pending_stream: None,
        }
    }

    /// Read the conversation history.
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Whether a streaming LLM request is in progress.
    pub fn is_streaming(&self) -> bool {
        self.pending_stream.as_ref().is_some_and(|s| !s.is_done())
    }

    /// Submit a chat request via the async bridge.
    ///
    /// Builds system prompt + scene context + history + user message, then
    /// submits a streaming request. The app should call `poll_stream()` each
    /// frame to collect chunks.
    pub fn submit_request(
        &mut self,
        bridge: &crate::runtime::AsyncBridge,
        provider: std::sync::Arc<dyn crate::llm::LlmProvider>,
        scene_context_json: &str,
        user_text: &str,
    ) {
        let mut system_content = build_system_prompt();
        system_content.push_str("\n\n## Current Scene Context\n```json\n");
        system_content.push_str(scene_context_json);
        system_content.push_str("\n```");

        let mut messages = vec![ChatMessage {
            role: MessageRole::System,
            content: system_content,
            tool_calls: None,
        }];

        messages.extend(self.history.iter().cloned());

        let user_message = ChatMessage {
            role: MessageRole::User,
            content: user_text.to_string(),
            tool_calls: None,
        };
        messages.push(user_message.clone());
        self.history.push(user_message);

        let tools = build_tool_definitions();
        let pending = bridge.submit_chat_stream(provider, messages, tools);
        self.pending_stream = Some(pending);
    }

    /// Poll for streaming chunks. Returns a `StreamEvent` for each chunk.
    ///
    /// Call this each frame while `is_streaming()` returns true.
    pub fn poll_stream(&mut self) -> Vec<StreamEvent> {
        let Some(pending) = self.pending_stream.as_mut() else {
            return Vec::new();
        };

        let chunks = pending.poll_chunks();
        let mut events = Vec::new();

        for chunk in chunks {
            match chunk {
                Ok(stream_chunk) => {
                    if !stream_chunk.content_delta.is_empty() {
                        events.push(StreamEvent::TextDelta(stream_chunk.content_delta.clone()));
                    }
                    if stream_chunk.finish_reason.is_some() {
                        match stream_chunk.finish_reason {
                            Some(FinishReason::Length) => {
                                events.push(StreamEvent::Truncated);
                            }
                            Some(FinishReason::ToolCall) => {
                                events.push(StreamEvent::ToolCall);
                            }
                            Some(FinishReason::Stop) | None => {}
                        }
                    }
                }
                Err(e) => {
                    events.push(StreamEvent::Error(e.to_string()));
                }
            }
        }

        if pending.is_done() {
            self.pending_stream = None;
        }

        events
    }

    /// Finalize a completed streaming response.
    ///
    /// Records the full assistant response text in the conversation history.
    pub fn finalize_response(&mut self, full_text: &str) {
        if !full_text.trim().is_empty() {
            self.history.push(ChatMessage {
                role: MessageRole::Assistant,
                content: full_text.to_string(),
                tool_calls: None,
            });
        }
    }

    /// Handle a local (non-LLM) pattern-matching request.
    ///
    /// Returns the response text and a list of requested actions.
    pub fn handle_local_request(&self, text: &str) -> LocalResponse {
        local_handler::process_local_request(text)
    }
}

impl Default for CoCreatorAgent {
    fn default() -> Self {
        Self::new()
    }
}

/// Events produced during streaming.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Incremental text from the LLM.
    TextDelta(String),
    /// Response was truncated due to token limit.
    Truncated,
    /// LLM requested a tool call.
    ToolCall,
    /// An error occurred.
    Error(String),
}

/// Result from a local (pattern-matching) request.
#[derive(Debug, Clone)]
pub struct LocalResponse {
    /// Text to display to the user.
    pub text: String,
    /// Actions the app should execute.
    pub actions: Vec<LocalAction>,
}

/// Actions that the local handler can request.
#[derive(Debug, Clone)]
pub enum LocalAction {
    /// Spawn a cube at the given position with the given size.
    SpawnCube { position: [f32; 3], size: [f32; 3] },
    /// Spawn a sphere at the given position with the given radius.
    SpawnSphere { position: [f32; 3], radius: f32 },
    /// Spawn a point light at the given position.
    SpawnLight { position: [f32; 3] },
    /// Spawn multiple cubes in a ring formation.
    SpawnCubeRing { count: usize },
}
