use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use katla_ecs::EntityId;
use mlua::{Lua, RegistryKey, UserData, UserDataMethods};

use crate::bindings::entity::LuaEntityId;
use crate::bindings::math::{LuaTransform, LuaVec3};
use crate::bindings::world::ScriptCommand;

/// Result of a raycast operation.
///
/// Returned by `world:get_raycast_result(index)` after calling `world:raycast()`.
#[derive(Clone, Debug)]
pub struct RaycastResult {
    /// The entity that was hit, if any (raw entity ID).
    pub entity: Option<u64>,
    /// The point in world space where the ray hit.
    pub point: katla_math::Vec3,
    /// The normal vector at the hit point.
    pub normal: katla_math::Vec3,
    /// The distance from the ray origin to the hit point.
    pub distance: f32,
}

/// Snapshot of input state for a frame.
///
/// This is captured at the beginning of each frame and passed to all scripts.
#[derive(Clone, Default)]
pub struct InputSnapshot {
    /// Set of action names that are currently pressed.
    /// Populated by the input system from keyboard/mouse/gamepad bindings.
    pub pressed_actions: HashSet<String>,
    /// Mouse movement delta since last frame (x, y).
    pub mouse_delta: (f32, f32),
    /// Mouse wheel scroll amount since last frame.
    pub mouse_wheel: f32,
}

/// Shared world data passed to all script instances each frame.
///
/// Contains immutable snapshots of world state that scripts can query.
#[derive(Clone)]
pub(crate) struct SharedWorldData {
    /// Current transforms for all entities with Transform components.
    pub transforms: HashMap<EntityId, katla_math::Transform>,
    /// All currently alive entities.
    pub live_entities: Vec<EntityId>,
    /// Map from component type names to lists of entities that have that component.
    pub component_entities: HashMap<String, Vec<EntityId>>,
    /// Current input state.
    pub input_state: InputSnapshot,
    /// Results of raycasts from the previous frame.
    pub raycast_results: HashMap<usize, RaycastResult>,
}

/// Shared event bus wrapper that allows scripts to emit events and register handlers.
///
/// This is used to collect events and subscriptions from Lua across the Lua/C boundary.
/// At the end of each frame, the `ScriptSystem` flushes these into the main `EventBus`.
#[derive(Default)]
pub struct SharedEventBus {
    /// Events emitted by scripts during the current frame.
    pub pending_emits: Vec<(String, mlua::Value)>,
    /// Handlers registered by scripts via on_event.
    pub pending_subscriptions: Vec<(String, RegistryKey)>,
}

/// Proxy object passed to Lua scripts as the `world` parameter.
///
/// This provides the script API for interacting with the game world.
/// All mutating operations queue commands that are processed after script execution.
///
/// # Lua API
///
/// The following methods are available to Lua scripts:
///
/// ## Entity Management
/// - `world:get_transform(entity)` - Get an entity's transform
/// - `world:set_transform(entity, transform)` - Set an entity's transform
/// - `world:set_position(entity, position)` - Set an entity's position
/// - `world:spawn_entity()` - Spawn a new entity, returns index
/// - `world:destroy_entity(entity)` - Destroy an entity
/// - `world:entity_exists(entity)` - Check if entity exists
///
/// ## Querying
/// - `world:get_all_with("ComponentName")` - Get all entities with a component
///
/// ## Input
/// - `world:is_action_pressed("action")` - Check if action is pressed
/// - `world:get_mouse_delta()` - Get mouse movement delta
/// - `world:get_mouse_wheel()` - Get mouse wheel delta
///
/// ## Audio
/// - `world:play_sound(path, volume, looping)` - Play a sound
/// - `world:play_sound_at(path, position, volume, looping)` - Play sound at position
/// - `world:play_sound_cue(cue_name)` - Play a sound cue
///
/// ## Physics
/// - `world:raycast(origin, direction, max_distance)` - Perform raycast
/// - `world:get_raycast_result(index)` - Get raycast result
///
/// ## Events
/// - `world:emit(name, data)` - Emit an event
/// - `world:on_event(name, callback)` - Subscribe to an event
pub struct ScriptWorldProxy {
    pub(crate) commands: Vec<ScriptCommand>,
    pub(crate) shared: Rc<SharedWorldData>,
    pub(crate) event_bus: Rc<RefCell<SharedEventBus>>,
    pub(crate) vm: Option<*const Lua>,
}

// SAFETY: The vm pointer is only read to create registry values during method calls,
// which happen within a single Lua thread. It is never sent across threads.
unsafe impl Send for ScriptWorldProxy {}
unsafe impl Sync for ScriptWorldProxy {}

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
            event_bus: Rc::new(RefCell::new(SharedEventBus::default())),
            vm: None,
        }
    }

    pub(crate) fn with_event_bus(&mut self, event_bus: Rc<RefCell<SharedEventBus>>, vm: &Lua) {
        self.event_bus = event_bus;
        self.vm = Some(vm as *const Lua);
    }

    pub fn with_transforms(transforms: Vec<(EntityId, katla_math::Transform)>) -> Self {
        Self {
            commands: Vec::new(),
            shared: Rc::new(SharedWorldData {
                transforms: transforms.into_iter().collect(),
                live_entities: Vec::new(),
                component_entities: HashMap::new(),
                input_state: InputSnapshot::default(),
                raycast_results: HashMap::new(),
            }),
            event_bus: Rc::new(RefCell::new(SharedEventBus::default())),
            vm: None,
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

    pub fn get_all_with(&self, component_name: &str) -> Vec<EntityId> {
        self.shared
            .component_entities
            .get(component_name)
            .cloned()
            .unwrap_or_default()
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

    /// Emit a named event with arbitrary Lua data.
    pub fn emit_event(&self, name: String, data: mlua::Value) {
        self.event_bus.borrow_mut().pending_emits.push((name, data));
    }

    /// Get the result of a previously issued raycast command by its return index.
    pub fn get_raycast_result(&self, index: usize) -> Option<RaycastResult> {
        self.shared.raycast_results.get(&index).cloned()
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

        methods.add_method("get_all_with", |lua, this, name: String| {
            let entities = this.get_all_with(&name);
            let table = lua.create_table()?;
            for (i, id) in entities.into_iter().enumerate() {
                table.set(i + 1, LuaEntityId(id))?;
            }
            Ok(table)
        });

        methods.add_method("is_action_pressed", |_, this, action: String| {
            Ok(this.is_action_pressed(&action))
        });

        methods.add_method("get_mouse_delta", |_, this, ()| {
            let (x, y) = this.get_mouse_delta();
            Ok((x, y))
        });

        methods.add_method("get_mouse_wheel", |_, this, ()| Ok(this.get_mouse_wheel()));

        methods.add_method_mut("emit", |_, this, (name, data): (String, mlua::Value)| {
            this.emit_event(name, data);
            Ok(())
        });

        methods.add_method_mut(
            "on_event",
            |lua, this, (name, callback): (String, mlua::Function)| {
                let key = lua.create_registry_value(callback)?;
                this.event_bus
                    .borrow_mut()
                    .pending_subscriptions
                    .push((name, key));
                Ok(())
            },
        );

        methods.add_method_mut(
            "play_sound",
            |_, this, (path, volume_or_nil, looping_or_nil): (String, Option<f32>, Option<bool>)| {
                let volume = volume_or_nil.unwrap_or(1.0);
                let looping = looping_or_nil.unwrap_or(false);
                this.commands.push(ScriptCommand::PlaySound {
                    path,
                    volume,
                    looping,
                });
                Ok(())
            },
        );

        methods.add_method_mut(
            "play_sound_at",
            |_,
             this,
             (path, position, volume_or_nil, looping_or_nil): (
                String,
                LuaVec3,
                Option<f32>,
                Option<bool>,
            )| {
                let volume = volume_or_nil.unwrap_or(1.0);
                let looping = looping_or_nil.unwrap_or(false);
                this.commands.push(ScriptCommand::PlaySoundAt {
                    path,
                    position: position.0,
                    volume,
                    looping,
                });
                Ok(())
            },
        );

        methods.add_method_mut("play_sound_cue", |_, this, cue_name: String| {
            this.commands.push(ScriptCommand::PlaySoundCue { cue_name });
            Ok(())
        });

        methods.add_method_mut(
            "raycast",
            |_, this, (origin, direction, max_distance): (LuaVec3, LuaVec3, f32)| {
                let return_index = this.commands.len();
                this.commands.push(ScriptCommand::Raycast {
                    origin: origin.0,
                    direction: direction.0,
                    max_distance,
                    return_index,
                });
                Ok(return_index)
            },
        );

        methods.add_method("get_raycast_result", |lua, this, index: usize| {
            match this.get_raycast_result(index) {
                Some(result) => {
                    let table = lua.create_table()?;
                    if let Some(entity_id) = result.entity {
                        table.set("entity", LuaEntityId(EntityId::from_raw(entity_id)))?;
                    }
                    table.set("point", LuaVec3(result.point))?;
                    table.set("normal", LuaVec3(result.normal))?;
                    table.set("distance", result.distance)?;
                    Ok(Some(table))
                }
                None => Ok(None),
            }
        });
    }
}

pub fn register_script_world_type(lua: &Lua) -> Result<(), mlua::Error> {
    lua.register_userdata_type::<ScriptWorldProxy>(|reg| {
        ScriptWorldProxy::add_methods(reg);
    })?;
    Ok(())
}
