use std::collections::HashMap;

use crate::input::Action;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;
use winit::keyboard::ModifiersState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: KeyCode,
    pub modifiers: ModifiersState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MouseCombo {
    pub button: MouseButton,
    pub modifiers: ModifiersState,
}

impl MouseCombo {
    /// Creates a MouseCombo with no modifiers.
    pub fn button(button: MouseButton) -> Self {
        Self {
            button,
            modifiers: ModifiersState::empty(),
        }
    }

    /// Creates a MouseCombo with the specified modifiers.
    pub fn with_modifiers(button: MouseButton, modifiers: ModifiersState) -> Self {
        Self { button, modifiers }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputBinding {
    Keyboard(KeyCombo),
    Mouse(MouseCombo),
}

impl KeyCombo {
    pub fn new(key: KeyCode, modifiers: ModifiersState) -> Self {
        Self { key, modifiers }
    }

    /// Creates a KeyCombo with no modifiers.
    pub fn key(key: KeyCode) -> Self {
        Self {
            key,
            modifiers: ModifiersState::empty(),
        }
    }

    /// Creates a KeyCombo with the specified modifiers.
    pub fn with_modifiers(key: KeyCode, modifiers: ModifiersState) -> Self {
        Self { key, modifiers }
    }
}

pub struct InputMapper {
    action_map: HashMap<InputBinding, Action>,
}

impl Default for InputMapper {
    fn default() -> Self {
        let mut action_map = HashMap::new();
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::Space)),
            Action::Jump,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyW)),
            Action::MoveForward,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyS)),
            Action::MoveBackward,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyA)),
            Action::MoveLeft,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyD)),
            Action::MoveRight,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyE)),
            Action::MoveUp,
        );

        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyQ)),
            Action::MoveDown,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyF)),
            Action::Interact,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyI)),
            Action::Inventory,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyP)),
            Action::Pause,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::Escape)),
            Action::Exit,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::KeyL)),
            Action::LookEnable,
        );

        action_map.insert(
            InputBinding::Mouse(MouseCombo::button(MouseButton::Right)),
            Action::LookEnable,
        );
        action_map.insert(
            InputBinding::Keyboard(KeyCombo::key(KeyCode::ShiftLeft)),
            Action::Sprint,
        );
        action_map.insert(
            InputBinding::Mouse(MouseCombo::button(MouseButton::Left)),
            Action::Interact,
        );
        action_map.insert(
            InputBinding::Mouse(MouseCombo::with_modifiers(
                MouseButton::Right,
                ModifiersState::CONTROL,
            )),
            Action::PanEnable,
        );

        Self { action_map }
    }
}

impl InputMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map_action(&mut self, binding: InputBinding, action: Action) {
        self.action_map.insert(binding, action);
    }

    pub fn unmap_action(&mut self, binding: InputBinding) -> Option<Action> {
        self.action_map.remove(&binding)
    }

    pub fn get_action(&self, binding: &InputBinding) -> Option<Action> {
        self.action_map.get(binding).copied()
    }

    pub fn reset_to_default(&mut self) {
        *self = Self::default();
    }

    pub fn get_bindings_for_action(&self, action: Action) -> Vec<InputBinding> {
        self.action_map
            .iter()
            .filter(|&(_, a)| *a == action)
            .map(|(binding, _)| *binding)
            .collect()
    }
}
