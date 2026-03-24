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

    /// Whether the button is currently active (pressed or clicked).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pressed | Self::Clicked)
    }

    /// Get the clicked state as a bool.
    pub fn as_clicked_bool(&self) -> bool {
        matches!(self, Self::Clicked)
    }
}
