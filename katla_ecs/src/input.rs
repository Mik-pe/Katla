pub mod actions;
pub mod mouse;

pub use actions::Action;
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
    pub mouse_position: (f32, f32),
    pub mouse_delta: (f32, f32),
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
            mouse_position: (0.0, 0.0),
            mouse_delta: (0.0, 0.0),
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
        self.mouse_delta = (x - self.mouse_position.0, y - self.mouse_position.1);
        self.mouse_position = (x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(state.mouse_position, (100.0, 200.0));
        assert_eq!(state.mouse_delta, (100.0, 200.0));

        state.set_mouse_position(150.0, 250.0);
        assert_eq!(state.mouse_position, (150.0, 250.0));
        assert_eq!(state.mouse_delta, (50.0, 50.0));
    }

    #[test]
    fn test_mouse_delta_calculation() {
        let mut state = InputState::new();

        state.set_mouse_position(10.0, 20.0);
        assert_eq!(state.mouse_delta, (10.0, 20.0));

        state.set_mouse_position(15.0, 30.0);
        assert_eq!(state.mouse_delta, (5.0, 10.0));

        state.set_mouse_position(15.0, 30.0);
        assert_eq!(state.mouse_delta, (0.0, 0.0));
    }

    #[test]
    fn test_consecutive_mouse_positions() {
        let mut state = InputState::new();

        let positions = [(0.0, 0.0), (10.0, 20.0), (25.0, 45.0), (100.0, 200.0)];

        for (i, (x, y)) in positions.iter().enumerate() {
            state.set_mouse_position(*x, *y);
            assert_eq!(state.mouse_position, (*x, *y));

            if i > 0 {
                let (prev_x, prev_y) = positions[i - 1];
                assert_eq!(state.mouse_delta, (*x - prev_x, *y - prev_y));
            }
        }
    }
}
