use katla_math::Rect2D;

/// IME-related information a TextField sends to the application.
///
/// When a TextField node has focus, the ViewTree reports its IME requirements
/// via this struct. The application forwards these to the windowing system.
#[derive(Clone, Debug)]
pub struct ImeRequest {
    /// Screen-space rectangle for the IME candidate window.
    pub cursor_rect: Rect2D,
    /// Whether IME is currently active.
    pub active: bool,
}

impl ImeRequest {
    /// Create an inactive IME request (no text field focused).
    pub fn inactive() -> Self {
        Self {
            cursor_rect: Rect2D::default(),
            active: false,
        }
    }

    /// Create an active IME request at the given cursor position.
    pub fn at_cursor(cursor_rect: Rect2D) -> Self {
        Self {
            cursor_rect,
            active: true,
        }
    }
}

impl Default for ImeRequest {
    fn default() -> Self {
        Self::inactive()
    }
}
