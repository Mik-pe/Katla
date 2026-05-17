use katla_math::Vec2;

use super::UiContext;

impl UiContext {
    /// Get the current screen size.
    #[inline]
    pub fn screen_size(&self) -> Vec2 {
        self.screen_size
    }

    /// Get the current DPI scale factor.
    #[inline]
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Set the mouse cursor type.
    #[inline]
    pub fn set_mouse_cursor(&mut self, cursor: crate::input::MouseCursor) {
        self.input.set_cursor(cursor);
    }

    /// Check if a mouse button was clicked this frame.
    #[inline]
    pub fn mouse_clicked(&self, button: usize) -> bool {
        self.input.mouse_clicked(button)
    }

    /// Check if a mouse button is currently down.
    #[inline]
    pub fn mouse_down(&self, button: usize) -> bool {
        self.input.mouse_down[button]
    }

    /// Get the current mouse position.
    #[inline]
    pub fn mouse_pos(&self) -> Vec2 {
        self.input.mouse_pos
    }

    /// Check if a key was pressed this frame.
    #[inline]
    pub fn key_pressed(&self, key: crate::input::KeyCode) -> bool {
        self.input.key_pressed(key)
    }

    /// Check if a key is currently being held down.
    #[inline]
    pub fn key_down(&self, key: crate::input::KeyCode) -> bool {
        self.input.is_key_down(key)
    }

    /// Check if a mouse button was released this frame.
    #[inline]
    pub fn mouse_released(&self, button: usize) -> bool {
        self.input.mouse_released[button]
    }

    /// Check if a mouse button was double-clicked this frame.
    #[inline]
    pub fn mouse_double_clicked(&self, button: usize) -> bool {
        self.input.mouse_double_clicked(button)
    }

    /// Request that the UI capture keyboard input, preventing the application from processing it.
    #[inline]
    pub fn capture_keyboard(&mut self) {
        self.input.want_capture_keyboard = true;
    }

    /// Get the highest z-index that the mouse is currently over.
    ///
    /// Returns `z_index::DEFAULT` when no panel/popup covers the mouse.
    /// Used by the application layer to decide whether to forward input
    /// (e.g. scroll) to non-UI systems like the orbit camera.
    #[inline]
    pub fn hover_z_index(&self) -> u32 {
        self.hover_z_index
    }
}
