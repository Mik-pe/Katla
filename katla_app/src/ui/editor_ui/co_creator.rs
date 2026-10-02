use katla_agent::MessageRole;
use katla_math::Color;
use katla_ui::declarative::DraggablePanelState;

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
    pub(crate) input_epoch: u64,
    pub(crate) host_name: Option<String>,
    /// Private socket of the existing conversation owner.
    pub host_socket: String,
    /// Existing conversation chosen explicitly by the user.
    pub host_thread: String,
    /// Chat message history for display.
    pub messages: Vec<DisplayMessage>,
    #[cfg(any(feature = "mcp", test))]
    last_host_item: Option<(String, String)>,
    /// Whether we're waiting for an agent response.
    pub processing: bool,
    /// Status message shown when idle.
    pub status_message: String,
}

impl CoCreatorState {
    pub fn new() -> Self {
        Self {
            panel: DraggablePanelState::default(),
            input_text: String::new(),
            input_epoch: 0,
            host_name: None,
            host_socket: String::new(),
            host_thread: String::new(),
            messages: Vec::new(),
            #[cfg(any(feature = "mcp", test))]
            last_host_item: None,
            processing: false,
            status_message: "Connect a conversation in Connection settings.".to_string(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.panel.is_visible()
    }

    pub fn open(&mut self) {
        self.panel.open();
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
        self.input_epoch = self.input_epoch.wrapping_add(1);
        self.processing = true;
    }

    /// Add a system message (errors, status).
    pub fn add_system_message(&mut self, text: &str) {
        self.messages.push(DisplayMessage {
            role: MessageRole::System,
            text: text.to_string(),
        });
        self.processing = false;
    }

    /// Mirror one host message without combining separate turns or message items.
    #[cfg(any(feature = "mcp", test))]
    pub(crate) fn append_host_text(&mut self, turn_id: &str, item_id: &str, delta: &str) {
        let key = (turn_id.to_owned(), item_id.to_owned());
        if self.last_host_item.as_ref() == Some(&key)
            && let Some(last) = self.messages.last_mut()
            && last.role == MessageRole::Assistant
        {
            last.text.push_str(delta);
        } else {
            self.messages.push(DisplayMessage {
                role: MessageRole::Assistant,
                text: delta.into(),
            });
            self.last_host_item = Some(key);
        }
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
    pub _panel_bg: Color,
    pub _panel_border: Color,
    pub _panel_header: Color,
    pub _background_light: Color,
    pub _text_primary: Color,
    pub text_muted: Color,
}

impl CoCreatorStyle {
    pub fn from_theme(theme: &ColorScheme) -> Self {
        Self {
            user_msg_color: theme.info,
            assistant_msg_color: theme.text_primary,
            system_msg_color: theme.text_muted,
            _panel_bg: theme.panel_bg,
            _panel_border: theme.panel_border,
            _panel_header: theme.panel_header,
            _background_light: theme.background_light,
            _text_primary: theme.text_primary,
            text_muted: theme.text_muted,
        }
    }
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
        assert_eq!(
            state.status_message,
            "Connect a conversation in Connection settings."
        );
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
    fn test_host_messages_remain_separate_across_turns_and_items() {
        let mut state = CoCreatorState::new();
        state.append_host_text("t1", "a", "Hello");
        state.append_host_text("t1", "a", " world");
        state.append_host_text("t1", "b", "Second item");
        state.append_host_text("t2", "a", "Next turn");
        assert_eq!(state.messages.len(), 3);
        assert_eq!(state.messages[0].text, "Hello world");
        assert_eq!(state.messages[1].text, "Second item");
        assert_eq!(state.messages[2].text, "Next turn");
        state.processing = true;
        state.finalize_streaming();
        assert!(!state.processing);
    }

    #[test]
    fn test_open_close() {
        let mut state = CoCreatorState::new();
        assert!(!state.is_open());

        state.open();
        assert!(state.is_open());

        state.panel.close();
        assert!(!state.is_open());
    }
}
