use std::sync::{Arc, Mutex};

use katla_math::{Color, Rect2D};
use katla_ui::declarative::{Alignment, Build, BuildContext, Padding, StateId, Widget, WidgetBox};
use katla_ui::{FontSize, ScrollAreaState};

use crate::ui::console::LogBuffer;
use crate::ui::editor_ui::ColorScheme;

/// Environment data injected before each frame for the console panel.
#[derive(Clone)]
pub(crate) struct ConsoleDrawCtx {
    pub bounds: Rect2D,
    pub theme: ColorScheme,
    pub filter_levels: [bool; 5],
    pub search_filter: String,
    pub log_buffer: Arc<Mutex<LogBuffer>>,
}

#[derive(Clone, Debug)]
pub(crate) enum ConsoleAction {
    ToggleLevel(usize),
    Clear,
}

#[derive(Debug, Clone)]
pub(crate) struct ConsoleState {
    #[expect(dead_code)]
    pub scroll_state: ScrollAreaState,
    pub filter_levels: [bool; 5],
    pub search_filter: String,
    #[expect(dead_code)]
    pub auto_scroll: bool,
    #[expect(dead_code)]
    pub selection_anchor: Option<usize>,
    #[expect(dead_code)]
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

pub(crate) struct ConsoleView;

impl Build for ConsoleView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        use katla_ui::declarative::{
            button, empty, hstack, panel, scroll, text, textfield, vstack,
        };

        let draw_ctx = ctx.env::<ConsoleDrawCtx>().cloned();
        let initial_search = draw_ctx
            .as_ref()
            .map(|draw_ctx| draw_ctx.search_filter.clone())
            .unwrap_or_default();

        // Always reserve state slots in the same order regardless of whether
        // the env is set, so that subsequent sibling views don't get their
        // StateId slots shifted when this view becomes active/inactive.
        let search_id: StateId = ctx.state(initial_search);
        let scroll_id: StateId = ctx.state(0.0f32);

        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };
        let search_filter: String = ctx
            .get_state(search_id)
            .unwrap_or_else(|| draw_ctx.search_filter.clone());

        const LEVEL_LABELS: [&str; 5] = ["Error", "Warn", "Info", "Debug", "Trace"];
        let mut filter_toggles = Vec::new();
        for (i, label) in LEVEL_LABELS.iter().enumerate() {
            let is_active = draw_ctx.filter_levels[i];
            let toggle = button(*label)
                .fill(if is_active {
                    draw_ctx.theme.selection
                } else {
                    Color::TRANSPARENT
                })
                .border(Color::TRANSPARENT)
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(ConsoleAction::ToggleLevel(i));
                }))
                .boxed();
            filter_toggles.push(toggle);
        }

        let search_field = textfield("Filter...", search_id).boxed();

        let clear_button = button("Clear")
            .fill(draw_ctx.theme.button_bg)
            .border(Color::TRANSPARENT)
            .on_click(ctx.on_click(|actions| {
                actions.emit(ConsoleAction::Clear);
            }))
            .boxed();

        let toolbar = hstack([
            hstack(filter_toggles)
                .spacing(4.0)
                .align(Alignment::Middle)
                .boxed(),
            search_field,
            clear_button,
        ])
        .spacing(8.0)
        .padding(Padding::all(4.0))
        .align(Alignment::Middle)
        .boxed();

        let search_lower = search_filter.to_lowercase();
        let level_index = |level: log::Level| -> usize {
            match level {
                log::Level::Error => 0,
                log::Level::Warn => 1,
                log::Level::Info => 2,
                log::Level::Debug => 3,
                log::Level::Trace => 4,
            }
        };

        let log_entries = {
            let mut entries = Vec::new();
            if let Ok(buf) = draw_ctx.log_buffer.lock() {
                for entry in buf
                    .entries()
                    .filter(|e| draw_ctx.filter_levels[level_index(e.level)])
                    .filter(|e| {
                        search_lower.is_empty() || e.message.to_lowercase().contains(&search_lower)
                    })
                {
                    let level_color = match entry.level {
                        log::Level::Error => draw_ctx.theme.error,
                        log::Level::Warn => draw_ctx.theme.warning,
                        log::Level::Info => draw_ctx.theme.success,
                        log::Level::Debug => draw_ctx.theme.text_muted,
                        log::Level::Trace => draw_ctx.theme.text_muted,
                    };
                    let level_badge = text(format!("{} ", log_level_badge(entry.level)))
                        .color(level_color)
                        .font_size(FontSize::XSmall)
                        .boxed();
                    let message = text(&entry.message)
                        .color(draw_ctx.theme.text_primary)
                        .boxed();
                    entries.push(
                        hstack([level_badge, message])
                            .spacing(8.0)
                            .padding_all(2.0)
                            .boxed(),
                    );
                }
            }
            entries
        };

        let log_content = if log_entries.is_empty() {
            text("No log entries")
                .color(draw_ctx.theme.text_muted)
                .boxed()
        } else {
            vstack(log_entries).boxed()
        };

        let content = vstack([
            toolbar,
            scroll(log_content, scroll_id).flex_grow(1.0).boxed(),
        ])
        .spacing(4.0)
        .padding(Padding::all(4.0))
        .flex_grow(1.0)
        .boxed();

        panel("Console".to_string(), content)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
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
