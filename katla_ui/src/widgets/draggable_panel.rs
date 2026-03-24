use katla_math::{Color, Rect2D, Vec2};

use crate::input::mouse_button;
use crate::widgets::Button;
use crate::{FontSize, UiContext};

/// Visibility state for a floating panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelState {
    #[default]
    Hidden,
    JustOpened,
    Visible,
}

impl PanelState {
    pub fn is_visible(&self) -> bool {
        *self != PanelState::Hidden
    }

    pub fn is_just_opened(&self) -> bool {
        *self == PanelState::JustOpened
    }

    pub fn open(&mut self) {
        *self = PanelState::JustOpened;
    }

    pub fn close(&mut self) {
        *self = PanelState::Hidden;
    }

    pub fn mark_shown(&mut self) {
        if *self == PanelState::JustOpened {
            *self = PanelState::Visible;
        }
    }
}

/// Shared state for a draggable floating panel (title bar drag, position, visibility).
#[derive(Debug, Clone, Default)]
pub struct DraggablePanelState {
    pub visibility: PanelState,
    pub position: Option<Vec2>,
    pub dragging: bool,
    pub drag_offset: Vec2,
}

impl DraggablePanelState {
    pub fn is_visible(&self) -> bool {
        self.visibility.is_visible()
    }

    pub fn open(&mut self) {
        self.visibility.open();
    }

    pub fn close(&mut self) {
        self.visibility.close();
        self.position = None;
    }

    pub fn mark_shown(&mut self) {
        self.visibility.mark_shown();
    }

    pub fn is_just_opened(&self) -> bool {
        self.visibility.is_just_opened()
    }
}

/// Colors needed by [`DraggablePanel`] to draw the panel chrome.
#[derive(Debug, Clone)]
pub struct DraggablePanelStyle {
    pub panel_bg: Color,
    pub panel_border: Color,
    pub panel_header: Color,
    pub background_light: Color,
    pub text_primary: Color,
    pub text_muted: Color,
}

/// Result of [`DraggablePanel::begin`] — contains layout info for the panel body.
pub struct DraggablePanelFrame {
    pub panel_bounds: Rect2D,
    pub close_clicked: bool,
    pub clicked_outside: bool,
}

/// Builds the chrome for a draggable, closeable floating panel.
///
/// Usage:
/// ```ignore
/// let frame = DraggablePanel::begin(ui, "my_panel", "Title", 450.0, 500.0, screen_size, &mut state, &style);
/// // ... draw panel content starting at frame.panel_bounds.min.y() + DraggablePanel::title_bar_height() ...
/// DraggablePanel::end(&mut state, &frame);
/// ```
pub struct DraggablePanel;

impl DraggablePanel {
    const TITLE_BAR_HEIGHT: f32 = 32.0;

    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        ui: &mut UiContext,
        id: &str,
        title: &str,
        panel_width: f32,
        panel_height: f32,
        screen_size: Vec2,
        state: &mut DraggablePanelState,
        style: &DraggablePanelStyle,
    ) -> DraggablePanelFrame {
        let title_bar_height = Self::TITLE_BAR_HEIGHT;

        let default_pos = Vec2::new(
            screen_size.x() * 0.5 - panel_width * 0.5,
            screen_size.y() * 0.5 - panel_height * 0.5,
        );
        let panel_pos = state.position.unwrap_or(default_pos);

        let title_bounds =
            Rect2D::from_origin_size(panel_pos, Vec2::new(panel_width, title_bar_height));

        let close_btn_area = Rect2D::from_origin_size(
            Vec2::new(panel_pos.x() + panel_width - 30.0, panel_pos.y()),
            Vec2::new(30.0, title_bar_height),
        );
        let can_drag = ui.is_hovered(title_bounds) && !ui.is_hovered(close_btn_area);

        if ui.mouse_clicked(mouse_button::LEFT) && can_drag {
            state.dragging = true;
            let mouse_pos = ui.mouse_pos();
            state.drag_offset =
                Vec2::new(mouse_pos.x() - panel_pos.x(), mouse_pos.y() - panel_pos.y());
        }

        if state.dragging {
            if ui.mouse_down(mouse_button::LEFT) {
                let mouse_pos = ui.mouse_pos();
                let new_pos = Vec2::new(
                    mouse_pos.x() - state.drag_offset.x(),
                    mouse_pos.y() - state.drag_offset.y(),
                );
                let clamped_x = new_pos
                    .x()
                    .clamp(0.0, (screen_size.x() - panel_width).max(0.0))
                    .round();
                let clamped_y = new_pos
                    .y()
                    .clamp(0.0, (screen_size.y() - panel_height).max(0.0))
                    .round();
                state.position = Some(Vec2::new(clamped_x, clamped_y));
            } else {
                state.dragging = false;
            }
        }

        let panel_pos = state.position.unwrap_or(default_pos);
        let panel_bounds =
            Rect2D::from_origin_size(panel_pos, Vec2::new(panel_width, panel_height));

        // Shadow
        let shadow_offset = Vec2::new(6.0, 6.0);
        let shadow_bounds = Rect2D::new(
            panel_bounds.min + shadow_offset,
            panel_bounds.max + shadow_offset,
        );
        ui.draw_rect(shadow_bounds, Color::new(0.0, 0.0, 0.0, 0.6));

        // Panel body
        ui.draw_rect(panel_bounds, style.panel_bg);
        ui.draw_rect_border(panel_bounds, style.panel_bg, style.panel_border, 1.0);

        // Title bar
        let title_bounds =
            Rect2D::from_origin_size(panel_bounds.min, Vec2::new(panel_width, title_bar_height));
        let title_color = if state.dragging || can_drag {
            style.background_light
        } else {
            style.panel_header
        };
        ui.draw_rect(title_bounds, title_color);

        // Drag handle
        let handle_x = panel_bounds.min.x() + panel_width * 0.5 - 20.0;
        let handle_y = panel_bounds.min.y() + 6.0;
        for i in 0..3 {
            let line_y = handle_y + i as f32 * 3.0;
            ui.draw_line(
                Vec2::new(handle_x, line_y),
                Vec2::new(handle_x + 40.0, line_y),
                style.text_muted,
                1.0,
            );
        }

        // Title text
        let title_pos = Vec2::new(
            panel_bounds.min.x() + ui.scaled_font_size(FontSize::Medium),
            panel_bounds.min.y() + ui.scaled_font_size(FontSize::Large),
        );
        ui.draw_text(
            title,
            title_pos,
            style.text_primary,
            ui.scaled_font_size(FontSize::Large),
        );

        // Close button
        let close_size = 24.0;
        let close_bounds = Rect2D::from_origin_size(
            Vec2::new(
                panel_bounds.max.x() - close_size - 6.0,
                panel_bounds.min.y() + 4.0,
            ),
            Vec2::new(close_size, close_size),
        );
        let close_clicked = ui
            .add(
                Button::new("\u{00d7}")
                    .bounds(close_bounds)
                    .id(&format!("close_{}", id)),
            )
            .clicked;

        // Click-outside detection
        let mouse_in_panel = panel_bounds.contains(ui.mouse_pos());
        let mouse_clicked_outside = !state.dragging
            && !state.visibility.is_just_opened()
            && ui.mouse_clicked(mouse_button::LEFT)
            && !mouse_in_panel;

        DraggablePanelFrame {
            panel_bounds,
            close_clicked,
            clicked_outside: mouse_clicked_outside,
        }
    }

    pub fn end(state: &mut DraggablePanelState, frame: &DraggablePanelFrame) {
        if frame.close_clicked || frame.clicked_outside {
            state.close();
        }
        state.mark_shown();
    }

    pub fn title_bar_height() -> f32 {
        Self::TITLE_BAR_HEIGHT
    }
}
