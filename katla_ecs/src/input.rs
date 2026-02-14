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
            mouse_position: Vec2::zero(),
            mouse_delta: Vec2::zero(),
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
        self.mouse_delta = Vec2::new(x - self.mouse_position.x(), y - self.mouse_position.y());
        self.mouse_position = Vec2::new(x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::actions::ActionState;

    #[test]
    fn test_input_state_new() {
        let state = InputState::new();

        assert_eq!(state.mouse_position, Vec2::zero());
        assert_eq!(state.mouse_delta, Vec2::zero());
        assert_eq!(state.mouse_wheel_delta, 0.0);

        // All mouse buttons should be released
        for button in 0..5 {
            assert_eq!(state.mouse_buttons[button], ButtonState::Released);
        }

        // All keyboard keys should be false
        for key in 0..Action::COUNT {
            assert!(!state.keyboard_keys[key]);
        }
    }

    #[test]
    fn test_input_state_default() {
        let state = InputState::default();
        assert_eq!(state.mouse_position, Vec2::zero());
    }

    #[test]
    fn test_set_action_state() {
        let mut state = InputState::new();

        state.set_action_state(Action::MoveForward, true);
        assert!(state.is_action_pressed(Action::MoveForward));
        assert!(state.keyboard_keys[Action::MoveForward as usize]);

        state.set_action_state(Action::Jump, false);
        assert!(!state.is_action_pressed(Action::Jump));
        assert!(!state.keyboard_keys[Action::Jump as usize]);
    }

    #[test]
    fn test_is_action_pressed() {
        let mut state = InputState::new();

        assert!(!state.is_action_pressed(Action::MoveLeft));

        state.set_action_state(Action::MoveLeft, true);
        assert!(state.is_action_pressed(Action::MoveLeft));

        state.set_action_state(Action::MoveLeft, false);
        assert!(!state.is_action_pressed(Action::MoveLeft));
    }

    #[test]
    fn test_multiple_action_states() {
        let mut state = InputState::new();

        state.set_action_state(Action::MoveForward, true);
        state.set_action_state(Action::MoveLeft, true);
        state.set_action_state(Action::Jump, false);

        assert!(state.is_action_pressed(Action::MoveForward));
        assert!(state.is_action_pressed(Action::MoveLeft));
        assert!(!state.is_action_pressed(Action::Jump));
    }

    #[test]
    fn test_all_actions_can_be_set() {
        let mut state = InputState::new();
        let actions = [
            Action::MoveForward,
            Action::MoveBackward,
            Action::MoveLeft,
            Action::MoveRight,
            Action::MoveUp,
            Action::MoveDown,
            Action::Jump,
            Action::Interact,
            Action::Inventory,
            Action::Pause,
            Action::Exit,
            Action::LookEnable,
            Action::Sprint,
        ];

        for action in actions {
            state.set_action_state(action, true);
            assert!(state.is_action_pressed(action));
        }
    }

    #[test]
    fn test_set_mouse_button_state() {
        let mut state = InputState::new();

        state.set_mouse_button_state(MouseButton::Left, ButtonState::Pressed);
        assert_eq!(
            state.mouse_buttons[MouseButton::Left as usize],
            ButtonState::Pressed
        );

        state.set_mouse_button_state(MouseButton::Right, ButtonState::Pressed);
        assert_eq!(
            state.mouse_buttons[MouseButton::Right as usize],
            ButtonState::Pressed
        );
    }

    #[test]
    fn test_set_mouse_position() {
        let mut state = InputState::new();

        state.set_mouse_position(100.0, 200.0);
        assert_eq!(state.mouse_position, Vec2::new(100.0, 200.0));
        assert_eq!(state.mouse_delta, Vec2::new(100.0, 200.0));

        state.set_mouse_position(150.0, 250.0);
        assert_eq!(state.mouse_position, Vec2::new(150.0, 250.0));
        assert_eq!(state.mouse_delta, Vec2::new(50.0, 50.0));
    }

    #[test]
    fn test_mouse_delta_calculation() {
        let mut state = InputState::new();

        state.set_mouse_position(10.0, 20.0);
        assert_eq!(state.mouse_delta, Vec2::new(10.0, 20.0));

        state.set_mouse_position(15.0, 30.0);
        assert_eq!(state.mouse_delta, Vec2::new(5.0, 10.0));

        state.set_mouse_position(15.0, 30.0);
        assert_eq!(state.mouse_delta, Vec2::new(0.0, 0.0));
    }

    #[test]
    fn test_all_mouse_buttons() {
        let mut state = InputState::new();

        state.set_mouse_button_state(MouseButton::Left, ButtonState::Pressed);
        state.set_mouse_button_state(MouseButton::Right, ButtonState::Pressed);
        state.set_mouse_button_state(MouseButton::Middle, ButtonState::Pressed);
        state.set_mouse_button_state(MouseButton::Forward, ButtonState::Pressed);
        state.set_mouse_button_state(MouseButton::Backward, ButtonState::Pressed);

        assert_eq!(
            state.mouse_buttons[MouseButton::Left as usize],
            ButtonState::Pressed
        );
        assert_eq!(
            state.mouse_buttons[MouseButton::Right as usize],
            ButtonState::Pressed
        );
        assert_eq!(
            state.mouse_buttons[MouseButton::Middle as usize],
            ButtonState::Pressed
        );
        assert_eq!(
            state.mouse_buttons[MouseButton::Forward as usize],
            ButtonState::Pressed
        );
        assert_eq!(
            state.mouse_buttons[MouseButton::Backward as usize],
            ButtonState::Pressed
        );
    }

    #[test]
    fn test_input_state_independence() {
        let mut state = InputState::new();

        // Set keyboard action
        state.set_action_state(Action::Jump, true);

        // Mouse buttons should still be released
        assert_eq!(
            state.mouse_buttons[MouseButton::Left as usize],
            ButtonState::Released
        );

        // Set mouse button
        state.set_mouse_button_state(MouseButton::Left, ButtonState::Pressed);

        // Keyboard action should still be pressed
        assert!(state.is_action_pressed(Action::Jump));
    }

    #[test]
    fn test_consecutive_mouse_positions() {
        let mut state = InputState::new();

        let positions = [
            (0.0, 0.0),
            (10.0, 20.0),
            (25.0, 45.0),
            (100.0, 200.0),
        ];

        for (i, (x, y)) in positions.iter().enumerate() {
            state.set_mouse_position(*x, *y);
            assert_eq!(state.mouse_position, Vec2::new(*x, *y));

            if i > 0 {
                let (prev_x, prev_y) = positions[i - 1];
                assert_eq!(
                    state.mouse_delta,
                    Vec2::new(*x - prev_x, *y - prev_y)
                );
            }
        }
    }

    #[test]
    fn test_toggle_action_state() {
        let mut state = InputState::new();

        // Toggle on
        state.set_action_state(Action::Sprint, true);
        assert!(state.is_action_pressed(Action::Sprint));

        // Toggle off
        state.set_action_state(Action::Sprint, false);
        assert!(!state.is_action_pressed(Action::Sprint));

        // Toggle on again
        state.set_action_state(Action::Sprint, true);
        assert!(state.is_action_pressed(Action::Sprint));
    }

    #[test]
    fn test_mouse_button_toggle() {
        let mut state = InputState::new();

        // Press
        state.set_mouse_button_state(MouseButton::Middle, ButtonState::Pressed);
        assert_eq!(
            state.mouse_buttons[MouseButton::Middle as usize],
            ButtonState::Pressed
        );

        // Release
        state.set_mouse_button_state(MouseButton::Middle, ButtonState::Released);
        assert_eq!(
            state.mouse_buttons[MouseButton::Middle as usize],
            ButtonState::Released
        );
    }
}
