//! Scene questions carry committed view context to the existing external owner.
use crate::application::Application;
use katla_agent::codex_host::{CodexHostBridge, CodexHostConfig, CodexHostEvent};
use katla_agent::mcp::McpResponseReceiver;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct ExternalChatState {
    bridge: Option<CodexHostBridge>,
    connected: bool,
    active_turn: Option<String>,
    finished_turns: std::collections::HashSet<String>,
    pending: Vec<(String, Instant, McpResponseReceiver)>,
}

impl ExternalChatState {
    pub(crate) fn connection(socket: String, thread_id: String) -> Self {
        Self {
            bridge: Some(CodexHostBridge::connect(CodexHostConfig {
                socket: socket.into(),
                thread_id,
            })),
            ..Default::default()
        }
    }

    fn handle(&mut self, event: CodexHostEvent, panel: &mut crate::ui::CoCreatorState) {
        match event {
            CodexHostEvent::Connected { name } => {
                self.connected = true;
                panel.host_name = name;
                panel.status_message = "Connected".into();
            }
            CodexHostEvent::Accepted(turn_id) => {
                if !self.finished_turns.contains(&turn_id) {
                    self.active_turn = Some(turn_id);
                    panel.processing = true;
                }
                panel.status_message = "Sent with the current view.".into();
            }
            CodexHostEvent::Text {
                turn_id,
                item_id,
                delta,
            } => {
                panel.append_host_text(&turn_id, &item_id, &delta);
            }
            CodexHostEvent::Finished { turn_id, status } => {
                self.finished_turns.insert(turn_id.clone());
                if self
                    .active_turn
                    .as_ref()
                    .is_none_or(|active| active == &turn_id)
                {
                    self.active_turn = None;
                    panel.finalize_streaming();
                    panel.status_message = match status.as_str() {
                        "completed" => "Ready".into(),
                        "interrupted" => "Stopped in Codex".into(),
                        _ => "Codex could not finish this question".into(),
                    };
                }
            }
            CodexHostEvent::Disconnected(error) => {
                self.connected = false;
                self.active_turn = None;
                self.pending.clear();
                panel.status_message = "Disconnected — reconnect in Connection settings.".into();
                panel.add_system_message(&error);
            }
            CodexHostEvent::HostAttention => {
                panel.status_message = "Continue in Codex to approve or sign in.".into();
            }
            CodexHostEvent::Error(error) => {
                panel.status_message = error.clone();
                panel.add_system_message(&error);
                panel.processing = self.active_turn.is_some();
            }
        }
    }
}

pub(super) fn connect(app: &mut Application, socket: String, thread_id: String) {
    app.editor.external_chat = ExternalChatState::connection(socket, thread_id);
    let panel = &mut app.editor.editor_ui.co_creator;
    panel.processing = false;
    panel.host_name = None;
    panel.status_message = "Connecting…".into();
}

pub(super) fn submit(app: &mut Application, text: String) {
    if text.trim().is_empty() {
        return;
    }
    if !app.editor.external_chat.connected {
        app.editor.editor_ui.co_creator.add_system_message(
            "Conversation is disconnected. Open Connection settings to reconnect.",
        );
        return;
    }
    if app.play_mode != crate::application::game_state::PlayMode::Editing {
        app.editor
            .editor_ui
            .co_creator
            .add_system_message("Return to edit mode to ask about the editor view.");
        return;
    }
    app.editor.editor_ui.co_creator.submit_message(&text);
    let response = app.editor.mcp_state.observe();
    app.editor
        .external_chat
        .pending
        .push((text, Instant::now(), response));
    app.editor.editor_ui.co_creator.status_message = "Capturing current view…".into();
}

pub(super) fn poll(app: &mut Application) {
    let state = &mut app.editor.external_chat;
    let panel = &mut app.editor.editor_ui.co_creator;
    if let Some(bridge) = &state.bridge {
        for event in bridge.poll() {
            state.handle(event, panel);
        }
    }
    let pending = std::mem::take(&mut state.pending);
    for (text, started, mut response) in pending {
        match response.try_recv() {
            Ok(response) => {
                let result = response.result.and_then(|mut metadata| {
                    let image = metadata
                        .as_object_mut()
                        .and_then(|m| m.remove("image_png_base64"));
                    let Some(serde_json::Value::String(png)) = image else {
                        return Err("Committed view capture returned no image".into());
                    };
                    state
                        .bridge
                        .as_ref()
                        .ok_or("External host is disconnected")?
                        .submit(text, metadata, png)
                });
                if let Err(error) = result {
                    panel.add_system_message(&error);
                }
            }
            Err(katla_agent::mcp::McpTryRecvError::Empty)
                if started.elapsed() < Duration::from_secs(15) =>
            {
                state.pending.push((text, started, response))
            }
            Err(_) => panel.add_system_message(
                "View capture did not complete; no question was sent to the external conversation.",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::CoCreatorState;

    #[test]
    fn test_disconnect_stops_processing_and_reconnect_reuses_display() {
        let mut state = ExternalChatState::default();
        let mut panel = CoCreatorState::new();
        state.handle(
            CodexHostEvent::Connected {
                name: Some("My room".into()),
            },
            &mut panel,
        );
        state.handle(CodexHostEvent::Accepted("a".into()), &mut panel);
        assert!(state.connected && panel.processing);
        assert_eq!(panel.host_name.as_deref(), Some("My room"));
        state.handle(CodexHostEvent::Disconnected("EOF".into()), &mut panel);
        assert!(!state.connected && !panel.processing);
        assert!(state.active_turn.is_none());
        state.handle(
            CodexHostEvent::Connected {
                name: Some("My room".into()),
            },
            &mut panel,
        );
        assert!(state.connected);
        assert_eq!(panel.messages.len(), 1);
    }

    #[test]
    fn test_late_acceptance_and_other_turn_completion_do_not_leave_busy_state() {
        let mut state = ExternalChatState::default();
        let mut panel = CoCreatorState::new();
        state.handle(
            CodexHostEvent::Finished {
                turn_id: "old".into(),
                status: "completed".into(),
            },
            &mut panel,
        );
        state.handle(CodexHostEvent::Accepted("old".into()), &mut panel);
        assert!(!panel.processing);
        state.handle(CodexHostEvent::Accepted("new".into()), &mut panel);
        state.handle(
            CodexHostEvent::Finished {
                turn_id: "old".into(),
                status: "completed".into(),
            },
            &mut panel,
        );
        assert!(panel.processing);
        state.handle(CodexHostEvent::Error("Steer rejected".into()), &mut panel);
        assert!(panel.processing);
        state.handle(
            CodexHostEvent::Finished {
                turn_id: "new".into(),
                status: "completed".into(),
            },
            &mut panel,
        );
        assert!(!panel.processing);
    }
}
