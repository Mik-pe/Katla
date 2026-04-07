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

/// Layout info passed to the content closure.
pub struct DraggablePanelFrame {
    pub panel_bounds: Rect2D,
}

/// Configuration for [`DraggablePanel::show`], built via the builder pattern.
///
/// # Example
/// ```ignore
/// DraggablePanel::show(
///     ui,
///     &mut state,
///     &style,
///     DraggablePanelConfig::new("my_panel", "Title")
///         .size(450.0, 500.0)
///         .screen_size(screen_size),
///     |ui, frame| {
///         // draw content at PANEL z-index
///     },
/// );
/// ```
pub struct DraggablePanelConfig<'a> {
    id: &'a str,
    title: &'a str,
    panel_width: f32,
    panel_height: f32,
    screen_size: Vec2,
}

impl<'a> DraggablePanelConfig<'a> {
    /// Create a new config with the given panel ID and title.
    pub fn new(id: &'a str, title: &'a str) -> Self {
        Self {
            id,
            title,
            panel_width: 300.0,
            panel_height: 300.0,
            screen_size: Vec2::new(800.0, 600.0),
        }
    }

    /// Set the panel dimensions (width and height).
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.panel_width = width;
        self.panel_height = height;
        self
    }

    /// Set the screen/viewport size used for centering and clamping.
    pub fn screen_size(mut self, screen_size: Vec2) -> Self {
        self.screen_size = screen_size;
        self
    }
}

/// Builds the chrome for a draggable, closeable floating panel.
///
/// Z-index is managed internally -- content runs at `z_index::PANEL`.
/// Scroll and hover are blocked for widgets underneath the panel.
pub struct DraggablePanel;

impl DraggablePanel {
    const TITLE_BAR_HEIGHT: f32 = 32.0;

    pub fn show<F>(
        ui: &mut UiContext,
        state: &mut DraggablePanelState,
        style: &DraggablePanelStyle,
        config: DraggablePanelConfig<'_>,
        content: F,
    ) where
        F: FnOnce(&mut UiContext, &DraggablePanelFrame),
    {
        let DraggablePanelConfig {
            id: _id,
            title,
            panel_width,
            panel_height,
            screen_size,
        } = config;

        if !state.is_visible() {
            return;
        }

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

        ui.push_z_index(crate::z_index::PANEL);

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
            .add(Button::new("\u{00d7}").bounds(close_bounds).id("\x00close"))
            .clicked;

        // Click-outside detection
        let mouse_in_panel = panel_bounds.contains(ui.mouse_pos());
        let mouse_clicked_outside = !state.dragging
            && !state.visibility.is_just_opened()
            && ui.mouse_clicked(mouse_button::LEFT)
            && !mouse_in_panel;

        let frame = DraggablePanelFrame { panel_bounds };

        content(ui, &frame);

        ui.pop_z_index();

        if close_clicked || mouse_clicked_outside {
            state.close();
        }
        state.mark_shown();
    }

    pub fn title_bar_height() -> f32 {
        Self::TITLE_BAR_HEIGHT
    }
}
