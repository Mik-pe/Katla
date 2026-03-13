use katla_math::{Rect2D, Vec2};

use crate::input::{mouse_button, UiInputState};
use crate::UiContext;

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
/// if resp.hovered {
///     ui.tooltip("Click this button");
/// }
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
            bounds,
            drag_delta: Vec2::new(0.0, 0.0),
            double_clicked: false,
        }
    }

    /// Check if any interaction occurred.
    pub fn any(&self) -> bool {
        self.clicked || self.hovered || self.active || self.changed
    }

    /// Was this a double-click?
    pub fn double_clicked(&self) -> bool {
        self.double_clicked
    }

    /// Get the drag delta for this frame.
    /// Returns non-zero only when the widget is being dragged.
    pub fn drag_delta(&self) -> Vec2 {
        self.drag_delta
    }

    /// Check if the widget is being dragged.
    pub fn is_dragging(&self) -> bool {
        self.active && (self.drag_delta.x().abs() > 0.0 || self.drag_delta.y().abs() > 0.0)
    }

    /// Show tooltip on hover (chainable).
    pub fn on_hover_text(self, ui: &mut UiContext, text: &str) -> Self {
        if self.hovered && !self.active {
            ui.tooltip(text);
        }
        self
    }

    /// Combine two responses (union of interactions).
    pub fn union(self, other: Self) -> Self {
        Response {
            clicked: self.clicked || other.clicked,
            hovered: self.hovered || other.hovered,
            active: self.active || other.active,
            changed: self.changed || other.changed,
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

/// Response from a container widget that returns a value.
///
/// Used by closure-based containers like `ui.horizontal()` that need to
/// return both a value from the closure and interaction info for the container.
///
/// # Example
///
/// ```ignore
/// let result = ui.horizontal(|ui| {
///     ui.add(Button::new("One"));
///     ui.add(Button::new("Two"));
///     "computed value"  // Return value from closure
/// });
///
/// // result.inner == "computed value"
/// // result.response == Response for the horizontal area
/// if result.response.clicked {
///     println!("Container was clicked!");
/// }
/// ```
#[derive(Debug, Clone)]
pub struct InnerResponse<R> {
    /// The return value from the closure.
    pub inner: R,
    /// The interaction response for the whole container.
    pub response: Response,
}

impl<R> InnerResponse<R> {
    /// Create a new InnerResponse.
    pub fn new(inner: R, response: Response) -> Self {
        Self { inner, response }
    }

    /// Map the inner value to a new type.
    pub fn map<T>(self, f: impl FnOnce(R) -> T) -> InnerResponse<T> {
        InnerResponse {
            inner: f(self.inner),
            response: self.response,
        }
    }
}
