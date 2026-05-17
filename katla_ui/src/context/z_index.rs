/// Z-index constants for UI layers.
pub const DEFAULT: u32 = 0;
/// Layer for floating panels/windows.
pub const PANEL: u32 = 100;
/// Layer for dropdowns and popups.
pub const POPUP: u32 = 200;
/// Layer for tooltips (always on top).
pub const TOOLTIP: u32 = 300;

use super::UiContext;

impl UiContext {
    /// Set the current Z-index for rendering.
    ///
    /// Higher Z values are rendered on top of lower Z values.
    /// Use the constants in `z_index` module for common layers.
    pub fn set_z_index(&mut self, z: u32) {
        self.z_index = z;
        self.draw_list.set_z_index(z);
    }

    /// Get the current Z-index.
    pub fn z_index(&self) -> u32 {
        self.z_index
    }

    /// Push a new Z-index onto the stack and set it as current.
    pub fn push_z_index(&mut self, z: u32) {
        self.z_stack.push(self.z_index);
        self.set_z_index(z);
    }

    /// Pop a Z-index from the stack and restore the previous value.
    pub fn pop_z_index(&mut self) {
        if let Some(prev_z) = self.z_stack.pop() {
            self.set_z_index(prev_z);
        }
    }

    /// Execute a closure with a temporary Z-index, automatically restoring afterward.
    ///
    /// This is the preferred way to use Z-index for drawing as it avoids borrow checker issues.
    ///
    /// # Example
    /// ```ignore
    /// ui.with_z_index(z_index::POPUP, |ui| {
    ///     ui.draw_rect(bounds, color);
    ///     ui.tooltip("Hello");
    /// }); // Auto-pops z-index
    /// ```
    pub fn with_z_index<F, R>(&mut self, z: u32, f: F) -> R
    where
        F: FnOnce(&mut UiContext) -> R,
    {
        self.push_z_index(z);
        let result = f(self);
        self.pop_z_index();
        result
    }
}
