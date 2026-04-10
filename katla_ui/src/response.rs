use katla_math::{Rect2D, Vec2};

use crate::context::UiContext;
use crate::input::{UiInputState, mouse_button};

/// Response from a widget interaction.
///
/// Provides detailed information about widget state and interaction.
/// All interactive widgets return this type.
///
/// # Example
/// ```ignore
/// use katla_ui::widgets::Button;
///
/// let resp = ui.add(Button::new("Click").bounds(my_bounds));
/// if resp.clicked {
///     // Handle click
/// }
/// if resp.double_clicked() {
///     // Handle double-click
/// }
/// resp.on_hover_tooltip(ui, "Click this button");
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Response {
    /// Widget was clicked this frame (button released while hovered).
    pub clicked: bool,
    /// Widget is being hovered (mouse over, not blocked by popup).
    pub hovered: bool,
    /// Widget is active (mouse pressed on it).
    pub active: bool,
    /// Widget value changed (for sliders, text inputs, checkboxes).
    pub changed: bool,
    /// Enter was pressed while this text input was focused.
    pub enter_pressed: bool,
    /// Widget bounds.
    pub bounds: Rect2D,
    /// Mouse delta since last frame (for dragging).
    pub drag_delta: Vec2,
    /// Whether this was a double-click.
    pub double_clicked: bool,
}

impl Response {
    /// Create a new response with default values.
    pub fn new(bounds: Rect2D) -> Self {
        Self {
            clicked: false,
            hovered: false,
            active: false,
            changed: false,
            enter_pressed: false,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Create a response for a clicked widget.
    pub fn clicked(bounds: Rect2D) -> Self {
        Self {
            clicked: true,
            hovered: true,
            active: false,
            changed: true,
            enter_pressed: false,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Create a response for a hovered widget.
    pub fn hovered(bounds: Rect2D) -> Self {
        Self {
            clicked: false,
            hovered: true,
            active: false,
            changed: false,
            enter_pressed: false,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Create a response for an active (pressed) widget.
    pub fn active(bounds: Rect2D) -> Self {
        Self {
            clicked: false,
            hovered: true,
            active: true,
            changed: false,
            enter_pressed: false,
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Check if any interaction occurred.
    pub fn any(&self) -> bool {
        self.clicked || self.hovered || self.active || self.changed
    }

    /// Check if the widget is being dragged.
    pub fn is_dragging(&self) -> bool {
        self.active && (self.drag_delta.x().abs() > 0.0 || self.drag_delta.y().abs() > 0.0)
    }

    /// Show a tooltip when this widget is hovered.
    pub fn on_hover_tooltip(self, ui: &mut UiContext, text: &str) {
        if self.hovered {
            ui.tooltip(text);
        }
    }

    /// Combine two responses (union of interactions).
    pub fn union(self, other: Self) -> Self {
        Response {
            clicked: self.clicked || other.clicked,
            hovered: self.hovered || other.hovered,
            active: self.active || other.active,
            changed: self.changed || other.changed,
            enter_pressed: self.enter_pressed || other.enter_pressed,
            bounds: self.bounds.union(&other.bounds),
            drag_delta: self.drag_delta + other.drag_delta,
            double_clicked: self.double_clicked || other.double_clicked,
        }
    }

    /// Create a response for an interactive widget with automatic double-click and drag tracking.
    ///
    /// This is a convenience constructor that handles common widget response patterns.
    ///
    /// # Arguments
    /// * `clicked` - Whether the widget was clicked this frame
    /// * `hovered` - Whether the widget is being hovered
    /// * `active` - Whether the widget is active (pressed)
    /// * `bounds` - Widget bounds
    /// * `input` - Input state reference for double-click and drag detection
    pub(crate) fn interactive(
        clicked: bool,
        hovered: bool,
        active: bool,
        bounds: Rect2D,
        input: &UiInputState,
    ) -> Self {
        let double_clicked = clicked && input.mouse_double_clicked(mouse_button::LEFT);
        let drag_delta = if active {
            input.mouse_delta
        } else {
            Vec2::new(0.0, 0.0)
        };

        Self {
            clicked,
            hovered,
            active,
            changed: clicked,
            enter_pressed: false,
            bounds,
            drag_delta,
            double_clicked,
        }
    }
}

impl Default for Response {
    fn default() -> Self {
        Self::new(Rect2D::from_size(Vec2::new(0.0, 0.0)))
    }
}

impl std::ops::BitOr for Response {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        self.union(other)
    }
}

impl std::ops::BitOrAssign for Response {
    fn bitor_assign(&mut self, other: Self) {
        *self = self.union(other);
    }
}
