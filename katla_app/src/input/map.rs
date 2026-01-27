use std::collections::HashMap;

use katla_ecs::input::actions::Action;
use winit::keyboard::KeyCode;
use winit::keyboard::ModifiersState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: KeyCode,
    pub modifiers: ModifiersState,
}

impl KeyCombo {
    pub fn new(key: KeyCode, modifiers: ModifiersState) -> Self {
        Self { key, modifiers }
    }
}

pub struct InputMapper {
    action_map: HashMap<KeyCombo, Action>,
}

impl Default for InputMapper {
    fn default() -> Self {
        let mut action_map = HashMap::new();
        action_map.insert(
            KeyCombo::new(KeyCode::Space, ModifiersState::empty()),
            Action::Jump,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::KeyW, ModifiersState::empty()),
            Action::MoveForward,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::KeyS, ModifiersState::empty()),
            Action::MoveBackward,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::KeyA, ModifiersState::empty()),
            Action::MoveLeft,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::KeyD, ModifiersState::empty()),
            Action::MoveRight,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::KeyE, ModifiersState::empty()),
            Action::MoveUp,
        );

        action_map.insert(
            KeyCombo::new(KeyCode::KeyQ, ModifiersState::empty()),
            Action::MoveDown,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::KeyF, ModifiersState::empty()),
            Action::Interact,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::KeyI, ModifiersState::empty()),
            Action::Inventory,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::KeyP, ModifiersState::empty()),
            Action::Pause,
        );
        action_map.insert(
            KeyCombo::new(KeyCode::Escape, ModifiersState::empty()),
            Action::Exit,
        );
        Self { action_map }
    }
}

pub struct KeyboardMapping(pub KeyCode);

impl KeyboardMapping {
    /// Returns the mapped ECS action for this key, if any.
    ///
    /// Note: Prefer `InputMapper` + `KeyCombo` for remappable bindings.
    pub fn to_action(self) -> Option<Action> {
        match self.0 {
            KeyCode::Escape => Some(Action::Exit),
            KeyCode::KeyW => Some(Action::MoveForward),
            KeyCode::KeyA => Some(Action::MoveLeft),
            KeyCode::KeyS => Some(Action::MoveBackward),
            KeyCode::KeyD => Some(Action::MoveRight),
            KeyCode::Space => Some(Action::Jump),
            KeyCode::KeyE => Some(Action::Interact),
            KeyCode::KeyI => Some(Action::Inventory),
            KeyCode::KeyP => Some(Action::Pause),
            _ => None,
        }
    }
}
