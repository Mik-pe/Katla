//! Immediate-mode builder widgets — internal rendering primitives.
//!
//! These widgets provide an immediate-mode builder pattern on top of `UiContext`.
//! They are used internally by the declarative draw pipeline.
//!
//! **For new UI code, prefer the declarative system** (`crate::declarative`) which
//! provides `ViewDescriptor` variants with automatic layout, diffing, input handling,
//! and animation support. Use these builders only when:
//!
//! - A declarative equivalent does not yet exist (TreeView, DraggablePanel, MenuBar,
//!   StatusBar, DockArea)

use crate::UiContext;
use katla_math::{Rect2D, Vec2};

mod color_picker;
pub use color_picker::{ColorPickerButton, ColorPickerState, hsv_to_rgb, rgb_to_hsv};

mod draggable_panel;
pub use draggable_panel::{
    DraggablePanel, DraggablePanelConfig, DraggablePanelFrame, DraggablePanelState, PanelState,
};

mod list_view;
pub use list_view::ListView;

mod tree;
pub use tree::{RenderItemFn, RowInfo, TreeItem, TreeState, TreeView};

mod dock;
pub use dock::{
    DockArea, DockAreaResponse, DockDragState, DockLayout, DockNode, DockPanelId, DockTabBar,
    DockTabBarResponse, DockZone, FloatingDockWindow, SplitDirection,
};

// =============================================================================
// ResizeHandle Widget
// =============================================================================

/// Direction of resize for a [`ResizeHandle`].
pub enum ResizeDirection {
    Horizontal,
    Vertical,
}

/// A thin invisible hit-region that drives panel-edge resizing.
///
/// Returns the new clamped dimension after each frame. Cursor changes and
/// drag tracking are handled internally so callers only need to feed the
/// returned value back into their layout.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::ResizeHandle;
///
/// let new_width = ResizeHandle::horizontal(edge_bounds, panel_width)
///     .min_value(120.0)
///     .max_value(400.0)
///     .show(ui);
/// ```
pub struct ResizeHandle {
    bounds: Rect2D,
    direction: ResizeDirection,
    current_value: f32,
    min_value: f32,
    max_value: f32,
    inverted: bool,
}

impl ResizeHandle {
    /// Create a horizontal resize handle (left/right drag changes width).
    pub fn horizontal(bounds: Rect2D, current_value: f32) -> Self {
        Self {
            bounds,
            direction: ResizeDirection::Horizontal,
            current_value,
            min_value: 0.0,
            max_value: f32::MAX,
            inverted: false,
        }
    }

    /// Create a vertical resize handle (up/down drag changes height).
    pub fn vertical(bounds: Rect2D, current_value: f32) -> Self {
        Self {
            bounds,
            direction: ResizeDirection::Vertical,
            current_value,
            min_value: 0.0,
            max_value: f32::MAX,
            inverted: false,
        }
    }

    /// Set the minimum allowed value.
    pub fn min_value(mut self, min: f32) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum allowed value.
    pub fn max_value(mut self, max: f32) -> Self {
        self.max_value = max;
        self
    }

    /// Negate the drag delta. Use for bottom or right edges where
    /// dragging against the axis should increase the dimension.
    pub fn inverted(mut self) -> Self {
        self.inverted = true;
        self
    }

    /// Process the resize interaction and return the new clamped dimension.
    pub fn show(self, ui: &mut UiContext) -> f32 {
        let id = ui.generate_id("resize_handle");
        let hovered = ui.input.is_hovered(self.bounds);

        if hovered {
            match self.direction {
                ResizeDirection::Horizontal => {
                    ui.set_mouse_cursor(crate::input::MouseCursor::ResizeHorizontal)
                }
                ResizeDirection::Vertical => {
                    ui.set_mouse_cursor(crate::input::MouseCursor::ResizeVertical)
                }
            }
        }

        let is_active = ui.active_id == Some(id);

        if hovered && ui.input.mouse_pressed[crate::input::mouse_button::LEFT] && !is_active {
            ui.active_id = Some(id);
        }

        if is_active {
            let raw_delta = match self.direction {
                ResizeDirection::Horizontal => ui.input.mouse_delta.x(),
                ResizeDirection::Vertical => ui.input.mouse_delta.y(),
            };
            let delta = if self.inverted { -raw_delta } else { raw_delta };
            let new_value = (self.current_value + delta).clamp(self.min_value, self.max_value);

            if !ui.input.mouse_down[crate::input::mouse_button::LEFT] {
                ui.active_id = None;
            }

            new_value
        } else {
            self.current_value
        }
    }
}

// =============================================================================
// MenuBar Widget
// =============================================================================

/// A horizontal menu bar widget drawn at the top of the screen.
///
/// Uses a show/end pattern instead of the `Widget` trait so that callers
/// can add left-aligned menu items, right-aligned content (title, status),
/// and then close the row layout.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::MenuBar;
///
/// let bar = MenuBar::new(screen_size.x(), 32.0);
/// bar.show(ui);
///
/// // Left-side menus
/// let file_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(50.0, 32.0));
/// ui.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |ui, open| {
///     if ui.menu_item_clicked("New") { *open = false; }
/// });
///
/// // Right-side content
/// bar.right_side(ui);
/// ui.draw_text("Katla Engine", ui.cursor(), text_color, font_size);
///
/// bar.end(ui);
/// ```
pub struct MenuBar {
    bounds: Rect2D,
}

impl MenuBar {
    /// Create a new menu bar spanning `width` with the given `height` at the top of the screen.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            bounds: Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(width, height)),
        }
    }

    /// Override the bounds entirely.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set the y-position while keeping width and height.
    pub fn y_position(mut self, y: f32) -> Self {
        self.bounds = Rect2D::from_origin_size(
            Vec2::new(self.bounds.min.x(), y),
            Vec2::new(self.bounds.width(), self.bounds.height()),
        );
        self
    }

    /// Override the height while keeping position and width.
    pub fn height(mut self, height: f32) -> Self {
        self.bounds =
            Rect2D::from_origin_size(self.bounds.min, Vec2::new(self.bounds.width(), height));
        self
    }

    /// Return the menu bar bounds.
    pub fn bounds_val(&self) -> Rect2D {
        self.bounds
    }

    /// Draw the menu bar background, border, begin a row layout, and position the cursor.
    ///
    /// After calling this, add left-aligned menu items via `menu_bar_dropdown()`.
    /// When ready for right-aligned content, call `right_side()`.
    /// When finished, call `end()`.
    pub fn show(&self, ui: &mut UiContext) {
        ui.draw_rect(self.bounds, ui.style.menu_bg);

        ui.draw_line(
            Vec2::new(self.bounds.min.x(), self.bounds.max.y()),
            Vec2::new(self.bounds.max.x(), self.bounds.max.y()),
            ui.style.separator,
            1.0,
        );

        ui.set_cursor(self.bounds.min);
        ui.begin_row();
    }

    /// Move the cursor to the right side of the menu bar for right-aligned content.
    ///
    /// Call this after adding left-side menu items. Subsequent draw calls will
    /// position content near the right edge of the bar. The `padding` parameter
    /// controls how far from the right edge the cursor is placed.
    pub fn right_side(&self, ui: &mut UiContext, padding: f32) {
        let right_x = (self.bounds.max.x() - padding).max(self.bounds.min.x());
        ui.set_cursor(Vec2::new(right_x, self.bounds.min.y()));
    }

    /// End the menu bar row layout.
    ///
    /// Must be called after `show()` when all menu items and right-side content
    /// have been drawn.
    pub fn end(&self, ui: &mut UiContext) {
        ui.end_row();
    }
}

// =============================================================================
// StatusBar Widget
// =============================================================================

/// A status bar widget drawn at the bottom (or top) of the screen.
///
/// Draws a background rect with a top border line and positions the cursor
/// for subsequent `status_label` / `status_separator` calls.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::StatusBar;
///
/// let bar = StatusBar::new(screen_size.x(), 24.0, screen_size.y() - 24.0);
/// bar.show(ui);
/// ui.status_label("FPS: 60", fps_color);
/// ui.status_separator();
/// ui.status_label("Frame: 1234", text_color);
/// ```
pub struct StatusBar {
    bounds: Rect2D,
}

impl StatusBar {
    /// Create a new status bar spanning `width` with the given `height` at `y_position`.
    pub fn new(width: f32, height: f32, y_position: f32) -> Self {
        Self {
            bounds: Rect2D::from_origin_size(Vec2::new(0.0, y_position), Vec2::new(width, height)),
        }
    }

    /// Override the bounds entirely.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Override the height while keeping position and width.
    pub fn height(mut self, height: f32) -> Self {
        self.bounds =
            Rect2D::from_origin_size(self.bounds.min, Vec2::new(self.bounds.width(), height));
        self
    }

    /// Draw the status bar background and top border, then position the cursor
    /// for left-aligned content items.
    pub fn show(&self, ui: &mut UiContext) {
        ui.draw_line(
            Vec2::new(self.bounds.min.x(), self.bounds.min.y()),
            Vec2::new(self.bounds.max.x(), self.bounds.min.y()),
            ui.style.separator,
            1.0,
        );
        ui.draw_rect(self.bounds, ui.style.window_bg);

        let padding = ui.style.window_padding;
        ui.set_cursor(Vec2::new(
            self.bounds.min.x() + padding,
            self.bounds.min.y() + (self.bounds.height() - ui.style.font_size) * 0.5,
        ));
    }

    /// Return the status bar bounds.
    pub fn bounds_val(&self) -> Rect2D {
        self.bounds
    }
}
