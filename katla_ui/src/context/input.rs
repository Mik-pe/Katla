use katla_math::Vec2;

use super::UiContext;

impl UiContext {
    /// Get the current screen size.
    #[inline]
    pub fn screen_size(&self) -> Vec2 {
        self.screen_size
    }

    /// Set the mouse cursor type.
    #[inline]
    pub(crate) fn set_mouse_cursor(&mut self, cursor: crate::input::MouseCursor) {
        self.input.set_cursor(cursor);
    }

    /// Check if a mouse button was clicked this frame.
    #[inline]
    pub fn mouse_clicked(&self, button: usize) -> bool {
        self.input.mouse_clicked(button)
    }

    /// Get the current mouse position.
    #[inline]
    pub fn mouse_pos(&self) -> Vec2 {
        self.input.mouse_pos
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

    /// Returns the highest z-index that covered the mouse in the previous frame.
    /// Useful for blocking scene picking when a panel/popup was visible last frame.
    #[inline]
    pub fn prev_hover_z_index(&self) -> u32 {
        self.prev_hover_z_index
    }
}
