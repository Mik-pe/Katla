//! UI input state management.
//!
//! This module handles mouse, keyboard, and other input for UI interactions.
//! The input state should be updated each frame before building the UI.

use katla_math::Vec2;

/// Index constants for mouse buttons.
pub mod mouse_button {
    /// Left mouse button.
    pub const LEFT: usize = 0;
    /// Right mouse button.
    pub const RIGHT: usize = 1;
    /// Middle mouse button (scroll wheel click).
    pub const MIDDLE: usize = 2;
    /// Forward mouse button (side button).
    pub const FORWARD: usize = 3;
    /// Back mouse button (side button).
    pub const BACK: usize = 4;
}

/// Mouse cursor types for UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseCursor {
    /// Default arrow cursor.
    #[default]
    Arrow,
    /// Text input cursor (I-beam).
    Text,
    /// Resize horizontal (left-right).
    ResizeHorizontal,
    /// Resize vertical (up-down).
    ResizeVertical,
    /// Resize diagonal (top-left to bottom-right).
    ResizeDiagonal,
    /// Resize diagonal (top-right to bottom-left).
    ResizeDiagonal2,
    /// Hand cursor (for clickable elements).
    Hand,
    /// Crosshair cursor.
    Crosshair,
    /// Not allowed / forbidden.
    NotAllowed,
}

/// Maximum time between clicks for a double-click (in seconds).
pub const DOUBLE_CLICK_TIME: f64 = 0.5;

/// Maximum distance between clicks for a double-click (in pixels).
pub const DOUBLE_CLICK_MAX_DISTANCE: f32 = 5.0;

/// Input state for the UI system.
///
/// This should be updated each frame with the current mouse position,
/// button states, and keyboard input before calling `UiContext::begin()`.
#[derive(Debug, Clone)]
pub struct UiInputState {
    // Mouse state
    /// Current mouse position in screen coordinates.
    pub mouse_pos: Vec2,
    /// Mouse position from the previous frame.
    pub mouse_pos_prev: Vec2,
    /// Mouse movement delta this frame.
    pub mouse_delta: Vec2,
    /// Current mouse button states (true = down).
    pub mouse_down: [bool; 5],
    /// Mouse buttons that were pressed this frame.
    pub mouse_pressed: [bool; 5],
    /// Mouse buttons that were released this frame.
    pub mouse_released: [bool; 5],
    /// Mouse scroll wheel delta this frame.
    pub scroll_delta: Vec2,

    // Double-click detection
    /// Time of last click for each button (seconds since some epoch).
    pub last_click_time: [f64; 5],
    /// Position of last click for each button.
    pub last_click_pos: [Vec2; 5],
    /// Whether a double-click occurred this frame for each button.
    mouse_double_clicked: [bool; 5],

    // Keyboard state
    /// Characters typed this frame (for text input).
    pub characters: Vec<char>,
    /// Keys that were pressed this frame.
    pub keys_pressed: Vec<KeyCode>,
    /// Keys that were released this frame.
    pub keys_released: Vec<KeyCode>,
    /// Keys currently being held down.
    pub held_keys: std::collections::HashSet<KeyCode>,
    /// Whether any key is currently held down.
    pub any_key_down: bool,

    // UI state
    /// Whether the UI wants to capture mouse input.
    pub want_capture_mouse: bool,
    /// Whether the UI wants to capture keyboard input.
    pub want_capture_keyboard: bool,
    /// Requested mouse cursor type.
    pub cursor: MouseCursor,
    /// Whether scroll wheel input was consumed this frame.
    pub(crate) scroll_consumed: bool,
    /// The widget that was active (pressed/dragged) in the previous frame.
    /// Set by UiContext at the start of each frame.
    pub(crate) prev_active_id: Option<u64>,
}

impl UiInputState {
    /// Create a new input state with default values.
    pub fn new() -> Self {
        Self {
            mouse_pos: Vec2::new(0.0, 0.0),
            mouse_pos_prev: Vec2::new(0.0, 0.0),
            mouse_delta: Vec2::new(0.0, 0.0),
            mouse_down: [false; 5],
            mouse_pressed: [false; 5],
            mouse_released: [false; 5],
            scroll_delta: Vec2::new(0.0, 0.0),
            last_click_time: [0.0; 5],
            last_click_pos: [Vec2::new(0.0, 0.0); 5],
            mouse_double_clicked: [false; 5],
            characters: Vec::new(),
            keys_pressed: Vec::new(),
            keys_released: Vec::new(),
            held_keys: std::collections::HashSet::new(),
            any_key_down: false,
            want_capture_mouse: false,
            want_capture_keyboard: false,
            cursor: MouseCursor::Arrow,
            scroll_consumed: false,
            prev_active_id: None,
        }
    }

    /// Update mouse position.
    ///
    /// Call this each frame before `begin()`.
    pub fn set_mouse_pos(&mut self, pos: Vec2) {
        self.mouse_pos_prev = self.mouse_pos;
        self.mouse_pos = pos;
        self.mouse_delta = pos - self.mouse_pos_prev;
    }

    /// Update mouse button state.
    ///
    /// Call this when a mouse button is pressed or released.
    /// Note: For double-click detection, use `set_mouse_button_with_time` instead.
    pub fn set_mouse_button(&mut self, button: usize, down: bool) {
        if button >= 5 {
            return;
        }

        let was_down = self.mouse_down[button];
        self.mouse_down[button] = down;

        if down && !was_down {
            self.mouse_pressed[button] = true;
        } else if !down && was_down {
            self.mouse_released[button] = true;
        }
    }

    /// Update mouse button state with timestamp for double-click detection.
    ///
    /// Call this when a mouse button is pressed or released.
    /// `time` should be in seconds (e.g., from `instant.elapsed().as_secs_f64()`).
    pub fn set_mouse_button_with_time(&mut self, button: usize, down: bool, time: f64) {
        if button >= 5 {
            return;
        }

        let was_down = self.mouse_down[button];
        self.mouse_down[button] = down;

        if down && !was_down {
            self.mouse_pressed[button] = true;

            // Check for double-click
            let time_since_last = time - self.last_click_time[button];
            let distance = (self.mouse_pos - self.last_click_pos[button]).length();

            if time_since_last < DOUBLE_CLICK_TIME && distance < DOUBLE_CLICK_MAX_DISTANCE {
                self.mouse_double_clicked[button] = true;
            }

            // Update last click info
            self.last_click_time[button] = time;
            self.last_click_pos[button] = self.mouse_pos;
        } else if !down && was_down {
            self.mouse_released[button] = true;
        }
    }

    /// Add a typed character.
    pub fn add_char(&mut self, c: char) {
        self.characters.push(c);
    }

    /// Add a key press event.
    pub fn add_key_press(&mut self, key: KeyCode) {
        self.keys_pressed.push(key);
        self.held_keys.insert(key);
        self.any_key_down = true;
    }

    /// Add a key release event.
    pub fn add_key_release(&mut self, key: KeyCode) {
        self.keys_released.push(key);
        self.held_keys.remove(&key);
        self.any_key_down = !self.held_keys.is_empty();
    }

    /// Clear per-frame state.
    ///
    /// This should be called after `end()` to prepare for the next frame.
    pub fn clear_frame_state(&mut self) {
        self.mouse_delta = Vec2::new(0.0, 0.0);
        self.mouse_pressed = [false; 5];
        self.mouse_released = [false; 5];
        self.mouse_double_clicked = [false; 5];
        self.scroll_delta = Vec2::new(0.0, 0.0);
        self.characters.clear();
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.want_capture_mouse = false;
        self.want_capture_keyboard = false;
        self.any_key_down = false;
        self.cursor = MouseCursor::Arrow;
        self.scroll_consumed = false;
    }

    /// Set the mouse cursor type.
    pub fn set_cursor(&mut self, cursor: MouseCursor) {
        self.cursor = cursor;
    }

    /// Check if a mouse button was clicked this frame.
    #[inline]
    pub fn mouse_clicked(&self, button: usize) -> bool {
        button < 5 && self.mouse_pressed[button] && !self.mouse_released[button]
    }

    /// Check if a mouse button was double-clicked this frame.
    ///
    /// A double-click is detected when the same button is pressed twice
    /// within `DOUBLE_CLICK_TIME` seconds and within `DOUBLE_CLICK_MAX_DISTANCE` pixels.
    #[inline]
    pub fn mouse_double_clicked(&self, button: usize) -> bool {
        button < 5 && self.mouse_double_clicked[button]
    }

    /// Check if a mouse button is currently down.
    #[inline]
    pub fn is_mouse_down(&self, button: usize) -> bool {
        button < 5 && self.mouse_down[button]
    }

    /// Check if a key was pressed this frame.
    #[inline]
    pub fn key_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    /// Check if a key was released this frame.
    #[inline]
    pub fn key_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
    }

    /// Check if a key is currently being held down.
    #[inline]
    pub fn is_key_down(&self, key: KeyCode) -> bool {
        self.held_keys.contains(&key)
    }

    /// Check if mouse is hovering over a rectangle.
    #[inline]
    pub fn is_hovered(&self, rect: katla_math::Rect2D) -> bool {
        rect.contains(self.mouse_pos)
    }
}

impl Default for UiInputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Virtual key codes for keyboard input.
///
/// This is a simplified subset of key codes needed for UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    // Modifiers
    Shift,
    Control,
    Alt,
    Super,

    // Navigation
    Tab,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,

    // Editing
    Enter,
    Escape,
    Backspace,
    Delete,
    Insert,

    // Text selection
    A, // Select all (Ctrl+A)

    // Clipboard shortcuts (checked with Ctrl held)
    C,
    X,
    V,

    // Other common keys
    Space,
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Rect2D;

    #[test]
    fn test_mouse_button_press() {
        let mut state = UiInputState::new();

        state.set_mouse_button(mouse_button::LEFT, true);
        assert!(state.mouse_down[mouse_button::LEFT]);
        assert!(state.mouse_pressed[mouse_button::LEFT]);
        assert!(state.mouse_clicked(mouse_button::LEFT));

        state.clear_frame_state();
        assert!(!state.mouse_pressed[mouse_button::LEFT]);
        assert!(state.mouse_down[mouse_button::LEFT]); // Still down
    }

    #[test]
    fn test_hover_detection() {
        let mut state = UiInputState::new();
        state.set_mouse_pos(Vec2::new(50.0, 50.0));

        let rect = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        assert!(state.is_hovered(rect));

        let outside_rect =
            Rect2D::from_origin_size(Vec2::new(200.0, 200.0), Vec2::new(100.0, 100.0));
        assert!(!state.is_hovered(outside_rect));
    }

    #[test]
    fn test_double_click_detection_with_time() {
        let mut state = UiInputState::new();

        // First click at time 0.0
        state.set_mouse_pos(Vec2::new(50.0, 50.0));
        state.set_mouse_button_with_time(mouse_button::LEFT, true, 0.0);
        assert!(
            !state.mouse_double_clicked(mouse_button::LEFT),
            "First click should not be a double-click"
        );
        state.set_mouse_button_with_time(mouse_button::LEFT, false, 0.1);
        state.clear_frame_state();

        // Second click at time 0.2 (within DOUBLE_CLICK_TIME of 0.5s)
        state.set_mouse_pos(Vec2::new(50.0, 50.0)); // Same position
        state.set_mouse_button_with_time(mouse_button::LEFT, true, 0.2);
        assert!(
            state.mouse_double_clicked(mouse_button::LEFT),
            "Second quick click at same position should be a double-click"
        );
    }

    #[test]
    fn test_double_click_too_slow() {
        let mut state = UiInputState::new();

        // First click at time 0.0
        state.set_mouse_pos(Vec2::new(50.0, 50.0));
        state.set_mouse_button_with_time(mouse_button::LEFT, true, 0.0);
        state.set_mouse_button_with_time(mouse_button::LEFT, false, 0.1);
        state.clear_frame_state();

        // Second click at time 1.0 (too slow - beyond DOUBLE_CLICK_TIME of 0.5s)
        state.set_mouse_pos(Vec2::new(50.0, 50.0));
        state.set_mouse_button_with_time(mouse_button::LEFT, true, 1.0);
        assert!(
            !state.mouse_double_clicked(mouse_button::LEFT),
            "Click after 1 second should NOT be a double-click"
        );
    }

    #[test]
    fn test_double_click_too_far() {
        let mut state = UiInputState::new();

        // First click at position (50, 50)
        state.set_mouse_pos(Vec2::new(50.0, 50.0));
        state.set_mouse_button_with_time(mouse_button::LEFT, true, 0.0);
        state.set_mouse_button_with_time(mouse_button::LEFT, false, 0.1);
        state.clear_frame_state();

        // Second click at position (60, 60) - too far (14px away, max is 5px)
        state.set_mouse_pos(Vec2::new(60.0, 60.0));
        state.set_mouse_button_with_time(mouse_button::LEFT, true, 0.2);
        assert!(
            !state.mouse_double_clicked(mouse_button::LEFT),
            "Click too far away should NOT be a double-click"
        );
    }

    #[test]
    fn test_double_click_requires_with_time_method() {
        // This test documents that set_mouse_button (without time) does NOT
        // enable double-click detection - you must use set_mouse_button_with_time
        let mut state = UiInputState::new();

        // First click using set_mouse_button (without time)
        state.set_mouse_pos(Vec2::new(50.0, 50.0));
        state.set_mouse_button(mouse_button::LEFT, true);
        state.set_mouse_button(mouse_button::LEFT, false);
        state.clear_frame_state();

        // Second click using set_mouse_button (without time)
        state.set_mouse_pos(Vec2::new(50.0, 50.0));
        state.set_mouse_button(mouse_button::LEFT, true);
        assert!(
            !state.mouse_double_clicked(mouse_button::LEFT),
            "set_mouse_button without time should NOT enable double-click detection"
        );
    }

    #[test]
    fn test_hover_detection_on_exact_boundary() {
        let mut state = UiInputState::new();

        // Rect from (10, 10) to (110, 60) — width=100, height=50
        let rect = Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(100.0, 50.0));

        // Test all four edges (inclusive boundary check)
        // Top-left corner
        state.set_mouse_pos(Vec2::new(10.0, 10.0));
        assert!(
            state.is_hovered(rect),
            "Top-left corner should be hovered (inclusive)"
        );

        // Top-right corner
        state.set_mouse_pos(Vec2::new(110.0, 10.0));
        assert!(
            state.is_hovered(rect),
            "Top-right corner should be hovered (inclusive)"
        );

        // Bottom-left corner
        state.set_mouse_pos(Vec2::new(10.0, 60.0));
        assert!(
            state.is_hovered(rect),
            "Bottom-left corner should be hovered (inclusive)"
        );

        // Bottom-right corner
        state.set_mouse_pos(Vec2::new(110.0, 60.0));
        assert!(
            state.is_hovered(rect),
            "Bottom-right corner should be hovered (inclusive)"
        );

        // Just outside each edge
        state.set_mouse_pos(Vec2::new(9.999, 30.0));
        assert!(
            !state.is_hovered(rect),
            "Just left of left edge should not be hovered"
        );

        state.set_mouse_pos(Vec2::new(110.001, 30.0));
        assert!(
            !state.is_hovered(rect),
            "Just right of right edge should not be hovered"
        );

        state.set_mouse_pos(Vec2::new(60.0, 9.999));
        assert!(
            !state.is_hovered(rect),
            "Just above top edge should not be hovered"
        );

        state.set_mouse_pos(Vec2::new(60.0, 60.001));
        assert!(
            !state.is_hovered(rect),
            "Just below bottom edge should not be hovered"
        );
    }
}
