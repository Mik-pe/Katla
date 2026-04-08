mod prompt;

pub use prompt::build_system_prompt;

use katla_ecs::agent::{Agent, AgentAction, Observation};
use katla_ecs::scene_tool::SceneOp;

/// An agent driven by an LLM that assists with content creation.
///
/// The CoCreatorAgent wraps an LLM provider and translates between
/// the LLM's chat/tool-calling interface and the Agent trait's
/// observe-decide-act loop.
pub struct CoCreatorAgent {
    /// Conversation history as string pairs (role, content).
    messages: Vec<(String, String)>,
    /// Pending user request.
    pending_request: Option<String>,
    /// Next operation to execute (set by decide from LLM response).
    next_op: Option<SceneOp>,
}

impl CoCreatorAgent {
    pub fn new(system_prompt: &str) -> Self {
        Self {
            messages: vec![("system".to_string(), system_prompt.to_string())],
            pending_request: None,
            next_op: None,
        }
    }

    /// Submit a natural language request from the developer.
    pub fn submit_request(&mut self, request: &str) {
        self.pending_request = Some(request.to_string());
    }

    /// Read the pending request without consuming it.
    pub fn pending_request(&self) -> Option<&str> {
        self.pending_request.as_deref()
    }

    /// Read the conversation history.
    pub fn messages(&self) -> &[(String, String)] {
        &self.messages
    }
}

impl Agent for CoCreatorAgent {
    fn observe(&mut self, observation: &Observation) {
        self.messages.push((
            "user".to_string(),
            format!(
                "Scene observation: {} ({} entities)",
                observation.scene_summary, observation.entity_count
            ),
        ));
    }

    fn decide(&mut self) -> Option<SceneOp> {
        self.next_op.take()
    }

    fn on_result(&mut self, action: &AgentAction) {
        let msg = if let Some(ref result) = action.result {
            format!(
                "Action result: {} (success={})",
                result.message, result.success
            )
        } else if let Some(ref err) = action.error {
            format!("Action error: {err}")
        } else {
            "Action completed (no result)".to_string()
        };
        self.messages.push(("tool".to_string(), msg));
    }

    fn name(&self) -> &str {
        "Co-Creator"
    }
}
