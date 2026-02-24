//! Widget state management.
//!
//! Persistent state storage for widgets like checkboxes, sliders, text inputs.

use katla_math::Vec2;

/// Persistent state for widgets.
#[derive(Debug, Clone)]
pub enum WidgetState {
    /// Dropdown open state.
    DropdownOpen(bool),
    /// Context menu position.
    ContextMenuPos(Vec2),
}
