//! Widget interaction behavior helpers.
//!
//! This module provides shared interaction logic for widgets to reduce code duplication.

// WidgetId is defined in context module and re-exported from crate root

/// Result of click behavior processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClickResult {
    /// No click occurred.
    None,
    /// Button was pressed (mouse down while hovering).
    Pressed,
    /// Button was released (mouse up, but not hovering).
    Released,
    /// Button was clicked (mouse up while hovering).
    Clicked,
}

impl ClickResult {
    /// Whether this result represents a click.
    pub fn is_clicked(&self) -> bool {
        matches!(self, Self::Clicked)
    }

    /// Whether this result represents a press.
    pub fn is_pressed(&self) -> bool {
        matches!(self, Self::Pressed)
    }

    /// Whether the button is currently active (pressed or clicked).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pressed | Self::Clicked)
    }

    /// Get the clicked state as a bool.
    pub fn as_clicked_bool(&self) -> bool {
        matches!(self, Self::Clicked)
    }
}

/// Widget interaction state.
///
/// Tracks the interaction state of a widget for rendering purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InteractionState {
    /// Whether the widget is being hovered.
    pub hovered: bool,
    /// Whether the widget is active (pressed).
    pub active: bool,
}

impl InteractionState {
    /// Create a new interaction state.
    pub fn new(hovered: bool, active: bool) -> Self {
        Self { hovered, active }
    }

    /// Get the appropriate background color for a button based on state.
    pub fn button_color(
        &self,
        normal: katla_math::Color,
        hovered: katla_math::Color,
        active: katla_math::Color,
    ) -> katla_math::Color {
        if self.active {
            active
        } else if self.hovered {
            hovered
        } else {
            normal
        }
    }
}
