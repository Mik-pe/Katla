use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use katla_ecs::EntityId;
use mlua::{Lua, UserData, UserDataMethods};

use crate::bindings::entity::LuaEntityId;
use crate::bindings::math::{LuaTransform, LuaVec3};
use crate::bindings::world::ScriptCommand;

#[derive(Clone, Default)]
pub struct InputSnapshot {
    pub pressed_actions: HashSet<String>,
    pub mouse_delta: (f32, f32),
    pub mouse_wheel: f32,
}

#[derive(Clone)]
pub(crate) struct SharedWorldData {
    pub transforms: HashMap<EntityId, katla_math::Transform>,
    pub live_entities: Vec<EntityId>,
    pub input_state: InputSnapshot,
}

pub struct ScriptWorldProxy {
    pub(crate) commands: Vec<ScriptCommand>,
    pub(crate) shared: Rc<SharedWorldData>,
}

impl Default for ScriptWorldProxy {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptWorldProxy {
    pub fn new() -> Self {
        Self::with_transforms(Vec::new())
    }

    pub(crate) fn from_shared(shared: Rc<SharedWorldData>) -> Self {
        Self {
            commands: Vec::new(),
            shared,
        }
    }

    pub fn with_transforms(transforms: Vec<(EntityId, katla_math::Transform)>) -> Self {
        Self {
            commands: Vec::new(),
            shared: Rc::new(SharedWorldData {
                transforms: transforms.into_iter().collect(),
                live_entities: Vec::new(),
                input_state: InputSnapshot::default(),
            }),
        }
    }

    pub fn with_input(mut self, input: InputSnapshot) -> Self {
        Rc::get_mut(&mut self.shared).unwrap().input_state = input;
        self
    }

    pub fn push_live_entity(&mut self, id: EntityId) {
        if let Some(shared) = Rc::get_mut(&mut self.shared) {
            shared.live_entities.push(id);
        }
    }

    pub fn get_transform(&self, entity: EntityId) -> Option<katla_math::Transform> {
        self.shared.transforms.get(&entity).copied()
    }

    pub fn entity_exists(&self, entity: EntityId) -> bool {
        self.shared.live_entities.contains(&entity)
    }

    pub fn is_action_pressed(&self, action: &str) -> bool {
        self.shared.input_state.pressed_actions.contains(action)
    }

    pub fn get_mouse_delta(&self) -> (f32, f32) {
        self.shared.input_state.mouse_delta
    }

    pub fn get_mouse_wheel(&self) -> f32 {
        self.shared.input_state.mouse_wheel
    }
}

impl fmt::Display for ScriptWorldProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "World")
    }
}

impl UserData for ScriptWorldProxy {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get_transform", |_, this, entity: LuaEntityId| {
            match this.get_transform(entity.0) {
                Some(transform) => Ok(Some(LuaTransform(transform))),
                None => Ok(None),
            }
        });

        methods.add_method_mut(
            "set_transform",
            |_, this, (entity, transform): (LuaEntityId, LuaTransform)| {
                this.commands
                    .push(ScriptCommand::SetTransform(entity.0, transform.0));
                Ok(())
            },
        );

        methods.add_method_mut("spawn_entity", |_, this, ()| {
            let return_index = this.commands.len();
            this.commands
                .push(ScriptCommand::SpawnEntity { return_index });
            Ok(return_index)
        });

        methods.add_method_mut("destroy_entity", |_, this, entity: LuaEntityId| {
            this.commands.push(ScriptCommand::DestroyEntity(entity.0));
            Ok(())
        });

        methods.add_method_mut(
            "set_position",
            |_, this, (entity, position): (LuaEntityId, LuaVec3)| {
                this.commands
                    .push(ScriptCommand::SetPosition(entity.0, position.0));
                Ok(())
            },
        );

        methods.add_method("entity_exists", |_, this, entity: LuaEntityId| {
            Ok(this.entity_exists(entity.0))
        });

        methods.add_method("is_action_pressed", |_, this, action: String| {
            Ok(this.is_action_pressed(&action))
        });

        methods.add_method("get_mouse_delta", |_, this, ()| {
            let (x, y) = this.get_mouse_delta();
            Ok((x, y))
        });

        methods.add_method("get_mouse_wheel", |_, this, ()| Ok(this.get_mouse_wheel()));
    }
}

pub fn register_script_world_type(lua: &Lua) -> Result<(), mlua::Error> {
    lua.register_userdata_type::<ScriptWorldProxy>(|reg| {
        ScriptWorldProxy::add_methods(reg);
    })?;
    Ok(())
}
