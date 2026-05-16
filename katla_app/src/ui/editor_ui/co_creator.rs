use katla_agent::MessageRole;
use katla_math::Color;
use katla_ui::ScrollAreaState;
use katla_ui::widgets::DraggablePanelState;

use super::ColorScheme;

/// A display-oriented chat message for the co-creator panel.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: MessageRole,
    pub text: String,
}

/// State for the co-creator chat panel.
pub struct CoCreatorState {
    /// Draggable panel state (position, visibility, drag).
    pub panel: DraggablePanelState,
    /// Current text in the input field.
    pub input_text: String,
    /// Chat message history for display.
    pub messages: Vec<DisplayMessage>,
    /// Whether we're waiting for an agent response.
    pub processing: bool,
    /// Status message shown when idle.
    pub status_message: String,
    /// Scroll state for the message area.
    pub scroll_state: ScrollAreaState,
}

impl CoCreatorState {
    pub fn new() -> Self {
        Self {
            panel: DraggablePanelState::default(),
            input_text: String::new(),
            messages: Vec::new(),
            processing: false,
            status_message: "Type a request below.".to_string(),
            scroll_state: ScrollAreaState::default(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.panel.is_visible()
    }

    pub fn open(&mut self) {
        self.panel.open();
    }

    pub fn close(&mut self) {
        self.panel.close();
    }

    /// Add a user message and queue it for processing.
    pub fn submit_message(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.messages.push(DisplayMessage {
            role: MessageRole::User,
            text: text.to_string(),
        });
        self.input_text.clear();
        self.processing = true;
    }

    /// Add an assistant response.
    pub fn add_assistant_message(&mut self, text: &str) {
        self.messages.push(DisplayMessage {
            role: MessageRole::Assistant,
            text: text.to_string(),
        });
        self.processing = false;
    }

    /// Add a system message (errors, status).
    pub fn add_system_message(&mut self, text: &str) {
        self.messages.push(DisplayMessage {
            role: MessageRole::System,
            text: text.to_string(),
        });
        self.processing = false;
    }

    /// Append a streaming text delta to the last assistant message.
    pub fn append_streaming_text(&mut self, delta: &str) {
        if self.processing
            && let Some(last) = self.messages.last_mut()
            && last.role == MessageRole::Assistant
        {
            last.text.push_str(delta);
            return;
        }
        self.messages.push(DisplayMessage {
            role: MessageRole::Assistant,
            text: delta.to_string(),
        });
        self.processing = true;
    }

    /// Finalize the streaming response.
    pub fn finalize_streaming(&mut self) {
        self.processing = false;
        if let Some(last) = self.messages.last()
            && last.role == MessageRole::Assistant
            && last.text.trim().is_empty()
        {
            self.messages.pop();
        }
    }
}

impl Default for CoCreatorState {
    fn default() -> Self {
        Self::new()
    }
}

/// Style colors for the co-creator panel.
pub struct CoCreatorStyle {
    pub user_msg_color: Color,
    pub assistant_msg_color: Color,
    pub system_msg_color: Color,
    pub panel_bg: Color,
    pub panel_border: Color,
    pub panel_header: Color,
    pub background_light: Color,
    pub text_primary: Color,
    pub text_muted: Color,
}

impl CoCreatorStyle {
    pub fn from_theme(theme: &ColorScheme) -> Self {
        Self {
            user_msg_color: theme.info,
            assistant_msg_color: theme.text_primary,
            system_msg_color: theme.text_muted,
            panel_bg: theme.panel_bg,
            panel_border: theme.panel_border,
            panel_header: theme.panel_header,
            background_light: theme.background_light,
            text_primary: theme.text_primary,
            text_muted: theme.text_muted,
        }
    }
}

/// Whether the co-creator panel submitted a message this frame.
pub struct CoCreatorResponse {
    /// True if the user submitted a message this frame.
    pub submitted: bool,
    /// The submitted text, if any.
    pub submitted_text: Option<String>,
    /// True if the user clicked the undo button this frame.
    pub undo_clicked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_co_creator_state_new() {
        let state = CoCreatorState::new();
        assert!(!state.is_open());
        assert!(state.input_text.is_empty());
        assert!(state.messages.is_empty());
        assert!(!state.processing);
        assert_eq!(state.status_message, "Type a request below.");
    }

    #[test]
    fn test_submit_message() {
        let mut state = CoCreatorState::new();
        state.submit_message("spawn a cube");

        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, MessageRole::User);
        assert_eq!(state.messages[0].text, "spawn a cube");
        assert!(state.input_text.is_empty());
        assert!(state.processing);
    }

    #[test]
    fn test_add_assistant_message() {
        let mut state = CoCreatorState::new();
        state.processing = true;
        state.add_assistant_message("Done! Spawned a cube at origin.");

        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, MessageRole::Assistant);
        assert_eq!(state.messages[0].text, "Done! Spawned a cube at origin.");
        assert!(!state.processing);
    }

    #[test]
    fn test_add_system_message() {
        let mut state = CoCreatorState::new();
        state.processing = true;
        state.add_system_message("Error: could not process request.");

        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].role, MessageRole::System);
        assert!(!state.processing);
    }

    #[test]
    fn test_empty_submit_ignored() {
        let mut state = CoCreatorState::new();
        state.submit_message("");
        assert!(state.messages.is_empty());
        assert!(!state.processing);

        state.submit_message("   ");
        assert!(state.messages.is_empty());
        assert!(!state.processing);
    }

    #[test]
    fn test_open_close() {
        let mut state = CoCreatorState::new();
        assert!(!state.is_open());

        state.open();
        assert!(state.is_open());

        state.close();
        assert!(!state.is_open());
    }
}
