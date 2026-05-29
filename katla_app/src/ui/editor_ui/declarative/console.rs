use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use katla_math::{Color, Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::input::{KeyCode, mouse_button};
use katla_ui::widgets::{Button, TextInput};
use katla_ui::{FontSize, ScrollArea, ScrollAreaState, UiContext};

use crate::ui::console::LogBuffer;
use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::EditorAction;

thread_local! {
    static CONSOLE_CTX: RefCell<Option<ConsoleDrawCtx>> = const { RefCell::new(None) };
}

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

pub(crate) fn set_console_ctx(ctx: ConsoleDrawCtx) {
    CONSOLE_CTX.with(|c| *c.borrow_mut() = Some(ctx));
}

pub(crate) fn take_console_ctx() -> Option<ConsoleDrawCtx> {
    CONSOLE_CTX.with(|c| c.borrow_mut().take())
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
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        if CONSOLE_CTX.with(|c| c.borrow().is_some()) {
            ViewDescriptor::Custom(draw_console)
        } else {
            ViewDescriptor::Empty
        }
    }
}

const LEVEL_LABELS: [&str; 5] = ["Error", "Warn", "Info", "Debug", "Trace"];
const LEVEL_BADGES: [&str; 5] = ["E", "W", "I", "D", "T"];

fn level_color(level: log::Level, theme: &ColorScheme) -> Color {
    match level {
        log::Level::Error => theme.error,
        log::Level::Warn => theme.warning,
        log::Level::Info => theme.success,
        log::Level::Debug => theme.text_muted,
        log::Level::Trace => Color::new(0.4, 0.4, 0.4, 1.0),
    }
}

fn level_index(level: log::Level) -> usize {
    match level {
        log::Level::Error => 0,
        log::Level::Warn => 1,
        log::Level::Info => 2,
        log::Level::Debug => 3,
        log::Level::Trace => 4,
    }
}

fn draw_console(ui: &mut UiContext, _bounds: Rect2D) {
    let mut ctx = match take_console_ctx() {
        Some(ctx) => ctx,
        None => return,
    };

    let content_bounds = ctx.bounds;
    ui.draw_rect(content_bounds, ctx.theme.panel_bg);

    let theme = &ctx.theme;
    let padding = ui.style().panel_padding;
    let spacing = ui.style().item_inner_spacing;
    let toolbar_h = 28.0;
    let toolbar_y = content_bounds.min.y();
    let chip_h = 20.0;
    let chip_radius = 4.0;
    let chip_padding_h = 6.0;
    let chip_gap = 4.0;
    let dot_radius = 3.5;

    let font_size = ui.scaled_font_size(FontSize::Small);

    let mut x = content_bounds.min.x() + padding;

    let levels = [
        log::Level::Error,
        log::Level::Warn,
        log::Level::Info,
        log::Level::Debug,
        log::Level::Trace,
    ];

    let toolbar_center_y = toolbar_y + toolbar_h * 0.5;

    for (i, &level) in levels.iter().enumerate() {
        let label = LEVEL_LABELS[i];
        let label_size = ui.measure_text(label, font_size);
        let chip_w = dot_radius * 2.0 + 4.0 + label_size.x() + chip_padding_h * 2.0;
        let chip_y = toolbar_center_y - chip_h * 0.5;

        let chip_bounds = Rect2D::from_origin_size(Vec2::new(x, chip_y), Vec2::new(chip_w, chip_h));

        let active = ctx.filter_levels[i];
        let hovered = ui.is_hovered(chip_bounds);
        let fg = level_color(level, theme);

        let bg = if active {
            Color::new(fg.r, fg.g, fg.b, 0.15)
        } else if hovered {
            theme.background_dark
        } else {
            Color::TRANSPARENT
        };

        ui.draw_rounded_rect(chip_bounds, bg, chip_radius);

        let dot_center = Vec2::new(
            chip_bounds.min.x() + chip_padding_h + dot_radius,
            chip_center_y(chip_bounds),
        );
        let dot_color = if active {
            fg
        } else {
            Color::new(fg.r, fg.g, fg.b, 0.5)
        };
        ui.draw_circle(dot_center, dot_radius, dot_color);

        let text_color = if active {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let text_x = dot_center.x() + dot_radius + 4.0;
        let text_y = chip_bounds.min.y() + (chip_h - label_size.y()) * 0.5;
        ui.draw_text(label, Vec2::new(text_x, text_y), text_color, font_size);

        if hovered && ui.mouse_clicked(mouse_button::LEFT) {
            ctx.pending_actions
                .push(EditorAction::ToggleConsoleFilterLevel { level_index: i });
        }

        x += chip_w + chip_gap;
    }

    let btn_h = 20.0;
    let search_width = 150.0;
    let search_y = toolbar_center_y - btn_h * 0.5;
    let search_bounds = Rect2D::from_origin_size(
        Vec2::new(x + spacing, search_y),
        Vec2::new(search_width, btn_h),
    );
    ui.add(
        TextInput::new("console_search", &mut ctx.search_filter)
            .bounds(search_bounds)
            .placeholder("Filter..."),
    );

    let clear_x = content_bounds.max.x() - padding - 50.0;
    let clear_y = toolbar_center_y - btn_h * 0.5;
    let clear_bounds =
        Rect2D::from_origin_size(Vec2::new(clear_x, clear_y), Vec2::new(50.0, btn_h));
    if ui
        .add(
            Button::new("Clear")
                .bounds(clear_bounds)
                .fill_color(theme.button_bg)
                .hover_color(theme.button_hover)
                .border(theme.border),
        )
        .clicked
        && let Ok(mut buf) = ctx.log_buffer.lock()
    {
        buf.clear();
    }

    let log_area_y = toolbar_y + toolbar_h;
    let log_bounds = Rect2D::from_origin_size(
        Vec2::new(content_bounds.min.x(), log_area_y),
        Vec2::new(
            content_bounds.width(),
            (content_bounds.max.y() - log_area_y).max(0.0),
        ),
    );

    if log_bounds.height() <= 0.0 {
        set_console_ctx(ctx);
        return;
    }

    struct EntrySnapshot {
        level: log::Level,
        message: String,
        target: String,
    }

    let filtered_entries: Vec<EntrySnapshot> = {
        let log_buffer = ctx.log_buffer.clone();
        let buf = match log_buffer.lock() {
            Ok(b) => b,
            Err(_) => {
                set_console_ctx(ctx);
                return;
            }
        };
        buf.entries()
            .filter(|e| {
                if !ctx.filter_levels[level_index(e.level)] {
                    return false;
                }
                if ctx.search_filter.is_empty() {
                    return true;
                }
                let f = ctx.search_filter.to_lowercase();
                e.message.to_lowercase().contains(&f) || e.target.to_lowercase().contains(&f)
            })
            .map(|e| EntrySnapshot {
                level: e.level,
                message: e.message.clone(),
                target: e.target.clone(),
            })
            .collect()
    };

    let row_height = 18.0;
    let badge_w = 14.0;

    let sel_range = selection_range(ctx.selection_anchor, ctx.selection_cursor);
    let selection_color = theme.selection;

    ctx.scroll_state = ui.scroll_area(
        ScrollArea::new("console_scroll")
            .max_height(log_bounds.height())
            .stick_to_bottom(ctx.auto_scroll),
        ctx.scroll_state,
        log_bounds,
        |ui| {
            let scroll_offset = ui.scroll_offset();
            let mut y = 0.0;

            for (idx, entry) in filtered_entries.iter().enumerate() {
                let draw_y = log_bounds.min.y() + y - scroll_offset;

                if draw_y + row_height < log_bounds.min.y() || draw_y > log_bounds.max.y() {
                    y += row_height;
                    continue;
                }

                let row_bounds = Rect2D::from_origin_size(
                    Vec2::new(log_bounds.min.x(), draw_y),
                    Vec2::new(log_bounds.width(), row_height),
                );

                let is_selected = sel_range.is_some_and(|(a, b)| idx >= a && idx <= b);

                if is_selected {
                    ui.draw_rect(row_bounds, selection_color);
                }

                let hovered = ui.is_hovered(row_bounds);
                if hovered {
                    if ui.mouse_clicked(mouse_button::LEFT) {
                        let shift = ui.key_down(KeyCode::Shift);
                        if shift {
                            if ctx.selection_anchor.is_none() {
                                ctx.selection_anchor = Some(idx);
                            }
                            ctx.selection_cursor = Some(idx);
                        } else {
                            ctx.selection_anchor = Some(idx);
                            ctx.selection_cursor = Some(idx);
                        }
                        ctx.auto_scroll = false;
                    } else if ui.mouse_down(mouse_button::LEFT) && !ui.key_down(KeyCode::Shift) {
                        ctx.selection_cursor = Some(idx);
                    }
                }

                let fg = level_color(entry.level, theme);
                let fg_bg = Color::new(fg.r, fg.g, fg.b, 0.15);

                let badge_bounds = Rect2D::from_origin_size(
                    Vec2::new(log_bounds.min.x() + padding, draw_y),
                    Vec2::new(badge_w, row_height),
                );
                ui.draw_rect(badge_bounds, fg_bg);

                let badge_label = LEVEL_BADGES[level_index(entry.level)];
                let bc_size = ui.measure_text(badge_label, font_size);
                ui.draw_text(
                    badge_label,
                    Vec2::new(
                        badge_bounds.min.x() + (badge_w - bc_size.x()) * 0.5,
                        badge_bounds.min.y() + (row_height - bc_size.y()) * 0.5,
                    ),
                    fg,
                    font_size,
                );

                let msg_x = log_bounds.min.x() + padding + badge_w + spacing;
                let msg_max = log_bounds.width() - (msg_x - log_bounds.min.x()) - padding - 100.0;
                let display_msg = if msg_max > 0.0 {
                    ui.truncate_text(&entry.message, msg_max, font_size)
                } else {
                    entry.message.clone()
                };
                ui.draw_text(
                    &display_msg,
                    Vec2::new(msg_x, draw_y + 2.0),
                    theme.text_primary,
                    font_size,
                );

                let msg_w = ui.measure_text(&display_msg, font_size).x();
                let target_x = msg_x + msg_w + spacing * 2.0;
                let target_max = log_bounds.max.x() - target_x - padding;
                if target_max > 20.0 {
                    let display_target = ui.truncate_text(&entry.target, target_max, font_size);
                    ui.draw_text(
                        &display_target,
                        Vec2::new(target_x, draw_y + 2.0),
                        theme.text_muted,
                        font_size,
                    );
                }

                y += row_height;
            }

            y
        },
    );

    let cmd_held = ui.key_down(KeyCode::Control) || ui.key_down(KeyCode::Super);
    if cmd_held
        && ui.key_pressed(KeyCode::C)
        && let Some((start, end)) = selection_range(ctx.selection_anchor, ctx.selection_cursor)
    {
        let lines: Vec<String> = (start..=end)
            .filter_map(|i| filtered_entries.get(i))
            .map(|e| format!("[{}] {} — {}", e.level, e.message, e.target))
            .collect();
        let text = lines.join("\n");
        ui.copy_to_clipboard(&text);
    }

    set_console_ctx(ctx);
}

fn chip_center_y(bounds: Rect2D) -> f32 {
    bounds.min.y() + bounds.height() * 0.5
}
