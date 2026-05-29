use std::sync::{Arc, Mutex};

use katla_math::{Color, Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, Padding, StateId, ViewDescriptor};
use katla_ui::input::KeyCode;
use katla_ui::widgets::Button;
use katla_ui::{FontSize, ScrollArea, ScrollAreaState};

use crate::ui::console::LogBuffer;
use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::EditorAction;

/// Environment data injected before each frame for the console panel.
#[derive(Clone)]
pub(crate) struct ConsoleDrawCtx {
    pub bounds: Rect2D,
    pub theme: ColorScheme,
    pub scroll_state: ScrollAreaState,
    pub filter_levels: [bool; 5],
    pub search_filter: String,
    pub log_buffer: Arc<Mutex<LogBuffer>>,
    pub pending_actions: Vec<EditorAction>,
    pub auto_scroll: bool,
    pub selection_anchor: Option<usize>,
    pub selection_cursor: Option<usize>,
}

/// Actions emitted by the console panel to sync state back to the application.
#[derive(Clone, Debug)]
pub(crate) struct ConsoleSync {
    pub scroll_state: ScrollAreaState,
    pub filter_levels: [bool; 5],
    pub search_filter: String,
    pub auto_scroll: bool,
    pub selection_anchor: Option<usize>,
    pub selection_cursor: Option<usize>,
    pub pending_actions: Vec<EditorAction>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConsoleState {
    pub scroll_state: ScrollAreaState,
    pub filter_levels: [bool; 5],
    pub search_filter: String,
    pub auto_scroll: bool,
    pub selection_anchor: Option<usize>,
    pub selection_cursor: Option<usize>,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self {
            scroll_state: ScrollAreaState::default(),
            filter_levels: [true; 5],
            search_filter: String::new(),
            auto_scroll: true,
            selection_anchor: None,
            selection_cursor: None,
        }
    }
}

fn selection_range(anchor: Option<usize>, cursor: Option<usize>) -> Option<(usize, usize)> {
    match (anchor, cursor) {
        (Some(a), Some(c)) => Some((a.min(c), a.max(c))),
        _ => None,
    }
}

pub(crate) struct ConsoleView;

impl Build for ConsoleView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        use katla_ui::declarative::{button, hstack, panel, scroll, text, textfield, vstack};

        let draw_ctx = ctx.env::<ConsoleDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
        };

        let search_id: StateId = ctx.state(draw_ctx.search_filter.clone());
        let scroll_id: StateId = ctx.state(0.0f32);

        // Build filter level toggles
        const LEVEL_LABELS: [&str; 5] = ["Error", "Warn", "Info", "Debug", "Trace"];
        let mut filter_toggles = Vec::new();
        for (i, label) in LEVEL_LABELS.iter().enumerate() {
            let is_active = draw_ctx.filter_levels[i];
            let toggle = button(*label)
                .fill(Color::new(
                    0.0,
                    0.0,
                    0.0,
                    if is_active { 0.15 } else { 0.0 },
                ))
                .border(draw_ctx.theme.border);
            filter_toggles.push(toggle);
        }

        // Search field
        let search_field = textfield("Filter...", search_id);

        // Clear button
        let clear_button = button("Clear")
            .fill(draw_ctx.theme.button_bg)
            .border(draw_ctx.theme.border);

        let toolbar = hstack([
            hstack(filter_toggles).spacing(4.0),
            search_field,
            clear_button,
        ])
        .spacing(8.0)
        .padding(Padding::all(4.0));

        // Build log entries
        let log_entries = {
            let mut entries = Vec::new();
            if let Ok(buf) = draw_ctx.log_buffer.lock() {
                for entry in buf.entries() {
                    let level_color = match entry.level {
                        log::Level::Error => draw_ctx.theme.error,
                        log::Level::Warn => draw_ctx.theme.warning,
                        log::Level::Info => draw_ctx.theme.success,
                        log::Level::Debug => draw_ctx.theme.text_muted,
                        log::Level::Trace => Color::new(0.4, 0.4, 0.4, 1.0),
                    };
                    let level_badge = text(format!("{} ", log_level_badge(entry.level)))
                        .color(level_color)
                        .font_size(FontSize::XSmall);
                    let message = text(&entry.message).color(draw_ctx.theme.text_primary);
                    entries.push(hstack([level_badge, message]).spacing(8.0).padding_all(2.0));
                }
            }
            entries
        };

        let log_content = if log_entries.is_empty() {
            text("No log entries").color(draw_ctx.theme.text_muted)
        } else {
            vstack(log_entries)
        };

        let content = vstack([toolbar, scroll(log_content, scroll_id)])
            .spacing(4.0)
            .padding(Padding::all(4.0));

        panel("Console".to_string(), content).header_height(24.0)
    }
}

fn log_level_badge(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "E",
        log::Level::Warn => "W",
        log::Level::Info => "I",
        log::Level::Debug => "D",
        log::Level::Trace => "T",
    }
}
