//! Widget state management.
//!
//! Persistent state storage for widgets like checkboxes, sliders, text inputs.

use std::collections::HashMap;

use katla_math::Vec2;

use super::WidgetId;

/// Persistent state for widgets.
#[derive(Debug, Clone)]
pub enum WidgetState {
    /// Checkbox state.
    Checkbox(bool),
    /// Slider value.
    Slider(f32),
    /// Text input content.
    TextInput(String),
    /// Window position.
    WindowPos(Vec2),
    /// Dropdown open state.
    DropdownOpen(bool),
    /// Context menu position.
    ContextMenuPos(Vec2),
}

/// Storage for widget states.
pub type WidgetStorage = HashMap<WidgetId, WidgetState>;

/// Helper trait for accessing widget state.
pub trait StateAccess {
    /// Get a checkbox state.
    fn get_checkbox(&self, id: WidgetId) -> Option<bool>;

    /// Set a checkbox state.
    fn set_checkbox(&mut self, id: WidgetId, checked: bool);

    /// Get a slider value.
    fn get_slider(&self, id: WidgetId) -> Option<f32>;

    /// Set a slider value.
    fn set_slider(&mut self, id: WidgetId, value: f32);

    /// Get dropdown open state.
    fn get_dropdown_open(&self, id: WidgetId) -> bool;

    /// Set dropdown open state.
    fn set_dropdown_open(&mut self, id: WidgetId, open: bool);

    /// Get context menu position.
    fn get_context_menu_pos(&self, id: WidgetId) -> Option<Vec2>;

    /// Set context menu position.
    fn set_context_menu_pos(&mut self, id: WidgetId, pos: Vec2);
}

impl StateAccess for WidgetStorage {
    fn get_checkbox(&self, id: WidgetId) -> Option<bool> {
        self.get(&id).and_then(|s| {
            if let WidgetState::Checkbox(checked) = s {
                Some(*checked)
            } else {
                None
            }
        })
    }

    fn set_checkbox(&mut self, id: WidgetId, checked: bool) {
        self.insert(id, WidgetState::Checkbox(checked));
    }

    fn get_slider(&self, id: WidgetId) -> Option<f32> {
        self.get(&id).and_then(|s| {
            if let WidgetState::Slider(value) = s {
                Some(*value)
            } else {
                None
            }
        })
    }

    fn set_slider(&mut self, id: WidgetId, value: f32) {
        self.insert(id, WidgetState::Slider(value));
    }

    fn get_dropdown_open(&self, id: WidgetId) -> bool {
        self.get(&id)
            .map(|s| matches!(s, WidgetState::DropdownOpen(true)))
            .unwrap_or(false)
    }

    fn set_dropdown_open(&mut self, id: WidgetId, open: bool) {
        self.insert(id, WidgetState::DropdownOpen(open));
    }

    fn get_context_menu_pos(&self, id: WidgetId) -> Option<Vec2> {
        self.get(&id).and_then(|s| {
            if let WidgetState::ContextMenuPos(pos) = s {
                Some(*pos)
            } else {
                None
            }
        })
    }

    fn set_context_menu_pos(&mut self, id: WidgetId, pos: Vec2) {
        self.insert(id, WidgetState::ContextMenuPos(pos));
    }
}
