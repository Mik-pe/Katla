mod harness;
mod observation;
mod session;

#[cfg(test)]
mod tests;

pub use harness::{AgentHarness, AgentMessage, HarnessMessage};
pub use observation::build_observation;
pub use session::{ActionId, AgentAction, AgentSession, Observation};

use crate::scene_tool::SceneOp;

/// Core trait for an AI agent that operates on the scene.
///
/// The agent follows an observe-decide-act loop:
/// 1. `observe()` — the harness provides world state
/// 2. `decide()` — the agent returns a scene operation to execute
/// 3. `on_result()` — the harness provides the result of the operation
///
/// The loop continues until `decide()` returns `None`.
pub trait Agent {
    /// Called before each decide cycle. The agent receives world observations.
    fn observe(&mut self, observation: &Observation);

    /// The agent decides what to do next. Return `None` to end the session.
    fn decide(&mut self) -> Option<SceneOp>;

    /// Called after an action is executed. The agent receives the result.
    fn on_result(&mut self, action: &AgentAction);

    /// Human-readable name for this agent.
    fn name(&self) -> &str {
        "Unnamed Agent"
    }
}
