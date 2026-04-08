use crate::World;
use crate::scene_tool::{SceneOp, SceneToolError, ToolResult, UndoGroup};

/// Unique identifier for an agent action within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionId(pub u64);

/// An action taken by an agent during a session.
#[derive(Debug, Clone)]
pub struct AgentAction {
    pub id: ActionId,
    pub operation: SceneOp,
    pub result: Option<ToolResult>,
    pub error: Option<SceneToolError>,
}

/// A snapshot of what the agent can observe about the world.
#[derive(Debug, Clone)]
pub struct Observation {
    /// Summary of the scene state for the agent.
    pub scene_summary: String,
    /// Number of entities in the scene.
    pub entity_count: usize,
    /// Result of the last action, if any.
    pub last_action_result: Option<ToolResult>,
}

/// An agent session manages the lifecycle of an agent running on the scene.
///
/// The session records all actions taken, maintains the undo stack,
/// and coordinates between the agent and the ECS world.
pub struct AgentSession {
    /// Actions taken during this session.
    pub actions: Vec<AgentAction>,
    /// Undo stack — each entry corresponds to one agent turn.
    pub undo_stack: Vec<UndoGroup>,
    /// Next action ID.
    next_action_id: u64,
    /// Whether the session is paused.
    pub paused: bool,
    /// Whether the session is finished.
    pub finished: bool,
}

impl AgentSession {
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            undo_stack: Vec::new(),
            next_action_id: 0,
            paused: false,
            finished: false,
        }
    }

    /// Record an action and return its ID.
    pub fn record_action(&mut self, op: SceneOp) -> ActionId {
        let id = ActionId(self.next_action_id);
        self.next_action_id += 1;
        self.actions.push(AgentAction {
            id,
            operation: op,
            result: None,
            error: None,
        });
        id
    }

    /// Record the result of an action.
    pub fn record_result(&mut self, id: ActionId, result: Result<ToolResult, SceneToolError>) {
        if let Some(action) = self.actions.iter_mut().find(|a| a.id == id) {
            match result {
                Ok(tool_result) => action.result = Some(tool_result),
                Err(err) => action.error = Some(err),
            }
        }
    }

    /// Push an undo group onto the stack.
    pub fn push_undo(&mut self, group: UndoGroup) {
        self.undo_stack.push(group);
    }

    /// Undo the last agent turn.
    pub fn undo_last(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if let Some(mut group) = self.undo_stack.pop() {
            group.undo_all(world)?;
            if let Some(action) = self.actions.pop() {
                self.next_action_id = action.id.0;
            }
        }
        Ok(())
    }

    /// Undo all agent turns (restore to pre-session state).
    pub fn undo_all(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        while let Some(mut group) = self.undo_stack.pop() {
            group.undo_all(world)?;
        }
        self.actions.clear();
        self.next_action_id = 0;
        Ok(())
    }

    /// Get the number of actions in this session.
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    /// Get all actions.
    pub fn actions(&self) -> &[AgentAction] {
        &self.actions
    }
}

impl Default for AgentSession {
    fn default() -> Self {
        Self::new()
    }
}
