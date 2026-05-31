//! Widget interaction behavior helpers.
//!
//! This module provides shared interaction logic for widgets to reduce code duplication.

/// Configuration for click interaction behavior.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClickConfig {
    /// When `false`, uses `release_hovered` (raw input hover check) on release
    /// to bypass popup blocking. Use this for buttons and menu items that must
    /// respond even when a popup is consuming clicks.
    ///
    /// When `true`, uses the pre-computed `hovered` state which respects
    /// `active_id` checks.
    pub popup_aware: bool,
}

impl ClickConfig {
    /// Popup-aware click: release hover uses the pre-computed hovered state.
    /// Used by checkboxes, radio buttons, toggle buttons, image buttons.
    pub const POPUP_AWARE: Self = Self { popup_aware: true };
}

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
}
