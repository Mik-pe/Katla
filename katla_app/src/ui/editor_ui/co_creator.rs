use katla_agent::MessageRole;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::markdown::{MarkdownColors, draw_markdown_segments, parse_markdown_line, wrap_lines};
use katla_ui::widgets::{Button, ImageButton, TextInput};
use katla_ui::widgets::{DraggablePanelConfig, DraggablePanelState};
use katla_ui::{FontSize, ScrollArea, ScrollAreaState, UiContext};

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
    ///
    /// If the last message is an assistant message and we're processing,
    /// appends to it. Otherwise creates a new assistant message.
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
    ///
    /// Sets processing to false and removes empty assistant messages.
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

/// Render the co-creator chat panel.
/// Returns response indicating if a message was submitted.
pub fn draw_co_creator_panel(
    ui: &mut UiContext,
    state: &mut CoCreatorState,
    style: &CoCreatorStyle,
    screen_size: Vec2,
    agent_undo_count: usize,
) -> CoCreatorResponse {
    if !state.is_open() {
        state.panel.mark_shown();
        return CoCreatorResponse {
            submitted: false,
            submitted_text: None,
            undo_clicked: false,
        };
    }

    let status_message = state.status_message.clone();
    let messages: Vec<(MessageRole, String)> = state
        .messages
        .iter()
        .map(|m| (m.role.clone(), m.text.clone()))
        .collect();
    let processing = state.processing;
    let mut input_text = state.input_text.clone();
    let mut scroll_state = state.scroll_state;
    let mut send_clicked = false;
    let mut enter_pressed = false;
    let mut undo_clicked = false;

    let md_colors = MarkdownColors::from_style(ui.style());

    katla_ui::widgets::DraggablePanel::show(
        ui,
        &mut state.panel,
        DraggablePanelConfig::new("co_creator", "AI Co-Creator")
            .size(400.0, 500.0)
            .screen_size(screen_size)
            .close_on_outside_click(false),
        |ui, frame| {
            let content_x = frame.panel_bounds.min.x() + 8.0;
            let content_width = frame.panel_bounds.width() - 16.0;
            let header_height = 32.0;
            let bottom_padding = 8.0;

            let line_count = input_text.lines().count().max(1);
            let input_height = (line_count.min(5) as f32) * 28.0;

            let msg_area_top = frame.panel_bounds.min.y() + header_height + 8.0;
            let msg_area_bottom = frame.panel_bounds.max.y() - input_height - bottom_padding;

            let font_size = ui.scaled_font_size(FontSize::Small);

            if agent_undo_count > 0 {
                let undo_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        frame.panel_bounds.max.x() - 40.0,
                        frame.panel_bounds.min.y() + 2.0,
                    ),
                    Vec2::new(28.0, 28.0),
                );
                let response = ui.add(
                    ImageButton::new(katla_ui::ForkAwesome::UNDO)
                        .bounds(undo_bounds)
                        .id("co_creator_undo"),
                );
                if response.clicked {
                    undo_clicked = true;
                }
            }

            // Message area with scroll
            let msg_area_bounds = Rect2D::from_origin_size(
                Vec2::new(content_x, msg_area_top),
                Vec2::new(content_width, msg_area_bottom - msg_area_top),
            );

            let scroll_config = ScrollArea::new("co_creator_msgs")
                .max_height(msg_area_bounds.height())
                .stick_to_bottom(true);

            scroll_state = ui.scroll_area(scroll_config, scroll_state, msg_area_bounds, |ui| {
                let scroll_off = ui.scroll_offset();
                let mut y = msg_area_top - scroll_off;

                if messages.is_empty() {
                    ui.draw_text(
                        &status_message,
                        Vec2::new(content_x, y),
                        style.text_muted,
                        font_size,
                    );
                    y += font_size + 2.0;
                } else {
                    for (role, text) in &messages {
                        let color = match role {
                            MessageRole::User => style.user_msg_color,
                            MessageRole::Assistant => style.assistant_msg_color,
                            MessageRole::System => style.system_msg_color,
                            MessageRole::Tool => style.system_msg_color,
                        };

                        let prefix = match role {
                            MessageRole::User => "You: ",
                            MessageRole::Assistant => "AI: ",
                            MessageRole::System => "> ",
                            MessageRole::Tool => "> ",
                        };

                        let full_text = format!("{prefix}{text}");

                        for line in wrap_lines(&full_text, content_width, font_size, ui) {
                            let segments = parse_markdown_line(&line);
                            draw_markdown_segments(
                                ui,
                                &segments,
                                Vec2::new(content_x, y),
                                color,
                                font_size,
                                &md_colors,
                            );
                            y += font_size + 2.0;
                        }
                        y += 4.0;
                    }
                }

                if processing {
                    ui.draw_text(
                        "Processing...",
                        Vec2::new(content_x, y),
                        style.text_muted,
                        font_size,
                    );
                    y += font_size + 2.0;
                }

                y - msg_area_top + scroll_off
            });

            // Input area
            let input_y = msg_area_bottom + 4.0;
            let input_bounds = Rect2D::from_origin_size(
                Vec2::new(content_x, input_y),
                Vec2::new(content_width - 70.0, input_height),
            );

            let send_x = content_x + content_width - 60.0;
            let send_bounds =
                Rect2D::from_origin_size(Vec2::new(send_x, input_y), Vec2::new(60.0, 28.0));

            let input_response = ui.add(
                TextInput::new("co_creator_input", &mut input_text)
                    .bounds(input_bounds)
                    .placeholder("Ask the AI...")
                    .multiline(true)
                    .id("co_creator_input"),
            );
            enter_pressed = input_response.enter_pressed;

            let response = ui.add(
                Button::new("Send")
                    .bounds(send_bounds)
                    .id("co_creator_send"),
            );
            send_clicked = response.clicked;
        },
    );

    // Sync input text back from the closure's local copy
    state.input_text = input_text;
    state.scroll_state = scroll_state;

    // Process send after the closure returns (panel state borrow released)
    let mut submitted_text: Option<String> = None;
    if send_clicked || enter_pressed {
        let text = state.input_text.clone();
        state.submit_message(&text);
        if !text.trim().is_empty() {
            submitted_text = Some(text);
        }
    }

    state.panel.mark_shown();

    CoCreatorResponse {
        submitted: submitted_text.is_some(),
        submitted_text,
        undo_clicked,
    }
}

/// Word-wrap text preserving explicit newlines.
///
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
