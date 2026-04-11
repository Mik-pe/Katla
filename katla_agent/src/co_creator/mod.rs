mod local_handler;
mod prompt;

pub use prompt::build_system_prompt;

use std::collections::HashMap;

use crate::llm::{ChatMessage, FinishReason, MessageRole, ToolCall};
use crate::runtime::PendingStreamRequest;

#[cfg(feature = "llm-assistant")]
mod tools;

#[cfg(feature = "llm-assistant")]
pub use tools::build_tool_definitions;
#[cfg(feature = "llm-assistant")]
pub use tools::{
    AddComponentArgs, CreateResourceArgs, DestroyEntityArgs, DuplicateEntityArgs,
    GenerateResourceArgs, GetComponentAttributesArgs, GetSceneHierarchyArgs,
    ListAvailableComponentsArgs, ListResourcesArgs, LoadSceneArgs, QueryEntitiesArgs,
    ReadResourceArgs, SaveSceneArgs, SetFieldArgs, SetParentArgs, SpawnEntityArgs, SpawnModelArgs,
    WriteResourceArgs,
};

/// Accumulates fragments of a single tool call across streaming chunks.
#[derive(Debug, Clone, Default)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments: String,
}

/// An agent driven by an LLM that assists with content creation.
///
/// Single source of truth for conversation history and LLM interaction.
/// The app submits requests, polls for streaming chunks, and reads history.
pub struct CoCreatorAgent {
    /// Typed conversation history.
    history: Vec<ChatMessage>,
    /// Pending LLM streaming request, if any.
    pending_stream: Option<PendingStreamRequest>,
    /// Accumulated tool call fragments during streaming.
    tool_call_accumulators: HashMap<usize, ToolCallAccumulator>,
    /// Complete tool calls from the last streaming response, awaiting execution.
    pending_tool_calls: Vec<ToolCall>,
}

impl CoCreatorAgent {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            pending_stream: None,
            tool_call_accumulators: HashMap::new(),
            pending_tool_calls: Vec::new(),
        }
    }

    /// Read the conversation history.
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Clear all conversation history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Truncate history to at most `max_messages` recent messages (excluding system messages).
    /// Preserves system messages and keeps the most recent user/assistant/tool messages.
    pub fn truncate_history(&mut self, max_messages: usize) {
        let system_count = self
            .history
            .iter()
            .take_while(|m| m.role == MessageRole::System)
            .count();
        let non_system: Vec<_> = self.history.drain(system_count..).collect();
        let keep: Vec<_> = non_system.into_iter().rev().take(max_messages).collect();
        self.history.extend(keep.into_iter().rev());
    }

    /// Whether a streaming LLM request is in progress.
    pub fn is_streaming(&self) -> bool {
        self.pending_stream.as_ref().is_some_and(|s| !s.is_done())
    }

    /// Whether there are tool calls awaiting execution.
    pub fn has_pending_tool_calls(&self) -> bool {
        !self.pending_tool_calls.is_empty()
    }

    /// Take the pending tool calls for execution, clearing them from the agent.
    pub fn take_pending_tool_calls(&mut self) -> Vec<ToolCall> {
        std::mem::take(&mut self.pending_tool_calls)
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
            tool_call_id: None,
        }];

        messages.extend(self.history.iter().cloned());

        let user_message = ChatMessage {
            role: MessageRole::User,
            content: user_text.to_string(),
            tool_calls: None,
            tool_call_id: None,
        };
        messages.push(user_message.clone());
        self.history.push(user_message);

        let tools = build_tool_definitions();
        let pending = bridge.submit_chat_stream(provider, messages, tools);
        self.pending_stream = Some(pending);
        self.tool_call_accumulators.clear();
    }

    /// Submit a continuation request (e.g. after tool results) without adding a user message.
    pub fn submit_continuation(
        &mut self,
        bridge: &crate::runtime::AsyncBridge,
        provider: std::sync::Arc<dyn crate::llm::LlmProvider>,
        scene_context_json: &str,
    ) {
        let mut system_content = build_system_prompt();
        system_content.push_str("\n\n## Current Scene Context\n```json\n");
        system_content.push_str(scene_context_json);
        system_content.push_str("\n```");

        let mut messages = vec![ChatMessage {
            role: MessageRole::System,
            content: system_content,
            tool_calls: None,
            tool_call_id: None,
        }];

        messages.extend(self.history.iter().cloned());

        let tools = build_tool_definitions();
        let pending = bridge.submit_chat_stream(provider, messages, tools);
        self.pending_stream = Some(pending);
        self.tool_call_accumulators.clear();
    }

    /// Poll for streaming chunks. Returns a `StreamEvent` for each chunk.
    ///
    /// Call this each frame while `is_streaming()` returns true.
    pub fn poll_stream(&mut self) -> Vec<StreamEvent> {
        let (chunks, done) = {
            let Some(pending) = self.pending_stream.as_mut() else {
                return Vec::new();
            };
            let chunks = pending.poll_chunks();
            let done = pending.is_done();
            (chunks, done)
        };

        let mut events = Vec::new();
        let mut has_tool_call = false;

        for chunk in chunks {
            match chunk {
                Ok(stream_chunk) => {
                    // Accumulate tool call deltas
                    for delta in &stream_chunk.tool_call_deltas {
                        let acc = self.tool_call_accumulators.entry(delta.index).or_default();
                        if let Some(ref id) = delta.id {
                            acc.id = id.clone();
                        }
                        if let Some(ref name) = delta.name {
                            acc.name = name.clone();
                        }
                        if let Some(ref args) = delta.arguments_delta {
                            acc.arguments.push_str(args);
                        }
                    }

                    if !stream_chunk.content_delta.is_empty() {
                        events.push(StreamEvent::TextDelta(stream_chunk.content_delta.clone()));
                    }
                    if stream_chunk.finish_reason.is_some() {
                        match stream_chunk.finish_reason {
                            Some(FinishReason::Length) => {
                                events.push(StreamEvent::Truncated);
                            }
                            Some(FinishReason::ToolCall) => {
                                has_tool_call = true;
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

        if has_tool_call {
            let completed = self.finalize_tool_call_accumulators();
            events.push(StreamEvent::ToolCall(completed));
        }

        if done {
            self.pending_stream = None;
        }

        events
    }

    /// Convert accumulated tool call fragments into complete ToolCall structs.
    fn finalize_tool_call_accumulators(&mut self) -> Vec<ToolCall> {
        let mut results = Vec::new();
        let mut keys: Vec<usize> = self.tool_call_accumulators.keys().copied().collect();
        keys.sort();

        for key in keys {
            if let Some(acc) = self.tool_call_accumulators.remove(&key) {
                let arguments = serde_json::from_str(&acc.arguments).unwrap_or_default();
                results.push(ToolCall {
                    id: acc.id,
                    name: acc.name,
                    arguments,
                });
            }
        }

        self.pending_tool_calls = results.clone();
        results
    }

    /// Finalize a completed streaming response.
    ///
    /// Records the full assistant response text in the conversation history.
    /// If tool calls were made, includes them in the assistant message.
    pub fn finalize_response(&mut self, full_text: &str) {
        let tool_calls = if self.pending_tool_calls.is_empty() {
            None
        } else {
            Some(self.pending_tool_calls.clone())
        };

        if !full_text.trim().is_empty() || tool_calls.is_some() {
            self.history.push(ChatMessage {
                role: MessageRole::Assistant,
                content: full_text.to_string(),
                tool_calls,
                tool_call_id: None,
            });
        }
    }

    /// Add a tool result message to history.
    pub fn add_tool_result(&mut self, tool_call_id: String, result: String) {
        self.history.push(ChatMessage {
            role: MessageRole::Tool,
            content: result,
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
        });
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
    /// LLM requested tool calls with the completed tool call data.
    ToolCall(Vec<ToolCall>),
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
