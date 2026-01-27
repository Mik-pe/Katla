pub mod actions;
pub mod mouse;

pub use actions::Action;
use katla_math::Vec2;
pub use mouse::MouseButton;

pub enum ModifierKey {
    Shift,
    Control,
    Alt,
    Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

pub struct InputState {
    pub mouse_position: Vec2,
    pub mouse_delta: Vec2,
    pub mouse_wheel_delta: f32,
    pub mouse_buttons: [ButtonState; 5],
    pub keyboard_keys: [bool; Action::COUNT],
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            mouse_position: Vec2::ZERO,
            mouse_delta: Vec2::ZERO,
            mouse_wheel_delta: 0.0,
            mouse_buttons: [ButtonState::Released; 5],
            keyboard_keys: [false; Action::COUNT],
        }
    }

    pub fn set_action_state(&mut self, key: impl Into<Action>, pressed: bool) {
        self.keyboard_keys[key.into() as usize] = pressed;
    }

    pub fn is_action_pressed(&self, action: Action) -> bool {
        self.keyboard_keys[action as usize]
    }

    pub fn set_mouse_button_state(&mut self, button: MouseButton, state: ButtonState) {
        self.mouse_buttons[button as usize] = state;
    }

    pub fn set_mouse_position(&mut self, x: f32, y: f32) {
        self.mouse_delta = Vec2::new(x - self.mouse_position.x, y - self.mouse_position.y);
        self.mouse_position = Vec2::new(x, y);
    }
}
