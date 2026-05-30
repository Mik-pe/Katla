use std::sync::mpsc::{self, Receiver, Sender};

use super::Agent;
use super::session::{ActionId, AgentSession, Observation};
use crate::World;
use crate::scene_tool::{
    ComponentRegistry, SceneOp, SceneToolError, SceneToolExecutor, ToolResult,
};

/// Messages between the agent thread and the main thread.
pub enum AgentMessage {
    /// Agent wants to execute a scene operation.
    ExecuteOp(SceneOp),
    /// Agent is done.
    Finished,
}

/// Messages from the main thread back to the agent.
pub enum HarnessMessage {
    /// Result of executing an operation.
    Result(ActionId, Result<ToolResult, SceneToolError>),
    /// Current observation of the world.
    Observation(Observation),
    /// The harness is shutting down.
    Shutdown,
}

/// Maximum number of operations to process per tick call.
const MAX_OPS_PER_TICK: usize = 10;

/// The agent harness coordinates agent execution with the main render loop.
///
/// Usage:
/// 1. Create harness with `AgentHarness::new()`
/// 2. Start a synchronous agent with `run_sync_agent()` or prepare channels with `channels()`
/// 3. Each frame, call `tick(world, registry)` to process pending actions
/// 4. Check `session()` for action history and undo
pub struct AgentHarness {
    session: AgentSession,
    /// Channel for receiving agent requests (agent thread → main thread).
    agent_rx: Option<Receiver<AgentMessage>>,
    /// Channel for sending results back (main thread → agent thread).
    harness_tx: Option<Sender<HarnessMessage>>,
}

impl AgentHarness {
    pub fn new() -> Self {
        Self {
            session: AgentSession::new(),
            agent_rx: None,
            harness_tx: None,
        }
    }

    /// Get the channels for connecting an agent running on a background thread.
    /// Returns (agent_sender, harness_receiver) for the agent thread, and
    /// the harness keeps its own channels internally.
    pub fn channels(&mut self) -> (Sender<AgentMessage>, Receiver<HarnessMessage>) {
        let (agent_tx, agent_rx) = mpsc::channel();
        let (harness_tx, harness_rx) = mpsc::channel();
        self.agent_rx = Some(agent_rx);
        self.harness_tx = Some(harness_tx);
        (agent_tx, harness_rx)
    }

    /// Run a synchronous agent to completion (blocks until finished).
    /// Useful for scripted agents and testing.
    pub fn run_sync_agent(
        &mut self,
        agent: &mut dyn Agent,
        world: &mut World,
        registry: &ComponentRegistry,
    ) -> Result<&AgentSession, SceneToolError> {
        self.session = AgentSession::new();

        loop {
            let obs = super::observation::build_observation(world, registry);
            agent.observe(&obs);

            let Some(op) = agent.decide() else {
                self.session.finished = true;
                return Ok(&self.session);
            };

            let action_id = self.session.record_action(op.clone());
            let result = SceneToolExecutor::execute(op, world, registry);

            match result {
                Ok((tool_result, undo_group)) => {
                    let result_for_action = Ok(tool_result.clone());
                    self.session.push_undo(undo_group);
                    self.session.record_result(action_id, result_for_action);
                }
                Err(err) => {
                    self.session.record_result(action_id, Err(err));
                }
            }

            if let Some(action) = self.session.actions().last() {
                agent.on_result(action);
            }
        }
    }

    /// Process pending actions from a background agent thread.
    /// Call this once per frame from the main loop.
    /// Returns the number of actions processed.
    pub fn tick(
        &mut self,
        world: &mut World,
        registry: &ComponentRegistry,
    ) -> Result<usize, SceneToolError> {
        let Some(ref agent_rx) = self.agent_rx else {
            return Ok(0);
        };

        let mut processed = 0;

        while processed < MAX_OPS_PER_TICK {
            let msg = match agent_rx.try_recv() {
                Ok(msg) => msg,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.session.finished = true;
                    break;
                }
            };

            match msg {
                AgentMessage::ExecuteOp(op) => {
                    let action_id = self.session.record_action(op.clone());
                    let result = SceneToolExecutor::execute(op, world, registry);

                    match result {
                        Ok((tool_result, undo_group)) => {
                            let result_for_msg = Ok(tool_result.clone());
                            self.session.push_undo(undo_group);
                            self.session.record_result(action_id, Ok(tool_result));
                            self.send_if_connected(HarnessMessage::Result(
                                action_id,
                                result_for_msg,
                            ));
                        }
                        Err(err) => {
                            self.send_if_connected(HarnessMessage::Result(
                                action_id,
                                Err(err.clone()),
                            ));
                            self.session.record_result(action_id, Err(err));
                        }
                    }

                    processed += 1;
                }
                AgentMessage::Finished => {
                    self.session.finished = true;
                    break;
                }
            }
        }

        Ok(processed)
    }

    /// Access the session for undo/redo and action history.
    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut AgentSession {
        &mut self.session
    }

    fn send_if_connected(&self, msg: HarnessMessage) {
        if let Some(ref tx) = self.harness_tx {
            let _ = tx.send(msg);
        }
    }
}

impl Default for AgentHarness {
    fn default() -> Self {
        Self::new()
    }
}
