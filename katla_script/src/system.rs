use std::any::TypeId;
use std::rc::Rc;

use katla_ecs::events::{ComponentEvent, EntityEvent};
use katla_ecs::{EntityId, System, World};
use katla_math::Transform;
use log::{debug, error};

use crate::bindings::script_world::{InputSnapshot, ScriptWorldProxy, SharedWorldData};
use crate::bindings::world::ScriptCommand;
use crate::component::{ScriptComponent, ScriptInstanceHandle};
use crate::engine::ScriptEngine;

const MAX_SCRIPT_ERRORS: u32 = 10;

type TransformProvider = Box<dyn FnMut(&World) -> Vec<(EntityId, Transform)>>;
type CommandConsumer = Box<dyn FnMut(&mut World, &[ScriptCommand])>;
type InputProvider = Box<dyn FnMut(&World) -> InputSnapshot>;

/// Resource that controls whether scripts execute their `on_update` hooks.
/// Insert into the ECS World to signal play mode. Defaults to `false` (suspended).
#[derive(Debug, Clone, Copy)]
pub struct ScriptsActive(pub bool);

pub struct ScriptSystem {
    pub(crate) engine: ScriptEngine,
    transform_provider: Option<TransformProvider>,
    command_consumer: Option<CommandConsumer>,
    input_provider: Option<InputProvider>,
}

impl Default for ScriptSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptSystem {
    pub fn new() -> Self {
        Self {
            engine: ScriptEngine::new().expect("failed to create script engine"),
            transform_provider: None,
            command_consumer: None,
            input_provider: None,
        }
    }

    /// Set the base directory for resolving bare script names.
    ///
    /// Called by the app bridge to configure where scripts live on disk.
    pub fn with_scripts_dir(mut self, dir: impl Into<String>) -> Self {
        self.engine.set_scripts_dir(dir);
        self
    }

    /// Set the transform snapshot provider. Called by the app bridge.
    ///
    /// The closure is invoked each frame to gather `(EntityId, Transform)` pairs
    /// from the world, which scripts can then read via `world:get_transform()`.
    pub fn with_transform_provider<F>(mut self, f: F) -> Self
    where
        F: FnMut(&World) -> Vec<(EntityId, Transform)> + 'static,
    {
        self.transform_provider = Some(Box::new(f));
        self
    }

    /// Set the command consumer. Called by the app bridge after scripts run.
    ///
    /// The closure receives mutable world access and the script commands,
    /// allowing it to apply `SetTransform`/`SetPosition` back to ECS components.
    pub fn with_command_consumer<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut World, &[ScriptCommand]) + 'static,
    {
        self.command_consumer = Some(Box::new(f));
        self
    }

    /// Set the input snapshot provider. Called each frame before scripts run.
    ///
    /// The closure gathers input state from the world so scripts can query
    /// actions via `world:is_action_pressed()`, mouse delta, and mouse wheel.
    pub fn with_input_provider<F>(mut self, f: F) -> Self
    where
        F: FnMut(&World) -> InputSnapshot + 'static,
    {
        self.input_provider = Some(Box::new(f));
        self
    }

    fn process_spawns(&mut self, world: &mut World) {
        let spawned: Vec<EntityId> = world
            .entity_events()
            .iter()
            .filter_map(|event| match event {
                EntityEvent::Spawned(id) => Some(*id),
                _ => None,
            })
            .collect();

        let script_type_id = TypeId::of::<ScriptComponent>();
        let component_added: Vec<EntityId> = world
            .component_events()
            .iter()
            .filter_map(|event| match event {
                ComponentEvent::Added(id, tid) if *tid == script_type_id => Some(*id),
                _ => None,
            })
            .collect();

        let all_new: std::collections::HashSet<EntityId> =
            spawned.into_iter().chain(component_added).collect();

        for entity_id in all_new {
            let path = match world.get_component::<ScriptComponent>(entity_id) {
                Some(script) if script.instance_handle.is_none() => script.script_path.clone(),
                _ => continue,
            };

            match self.engine.create_instance(entity_id, &path) {
                Ok(handle) => {
                    if let Some(comp) = world.get_component_mut::<ScriptComponent>(entity_id) {
                        comp.instance_handle = Some(handle);
                    }
                    debug!("Created script instance for entity {entity_id}: {path}");
                }
                Err(e) => {
                    error!("Failed to create script instance for entity {entity_id}: {e}");
                }
            }
        }
    }

    fn build_shared_data(&mut self, world: &World) -> SharedWorldData {
        let transforms = match self.transform_provider.as_mut() {
            Some(provider) => provider(world),
            None => Vec::new(),
        };

        let input = match self.input_provider.as_mut() {
            Some(provider) => provider(world),
            None => InputSnapshot::default(),
        };

        let live_entities = world.entity_ids().collect();

        SharedWorldData {
            transforms: transforms.into_iter().collect(),
            live_entities,
            input_state: input,
        }
    }

    fn apply_commands(&mut self, commands: Vec<ScriptCommand>, world: &mut World) {
        if let Some(consumer) = self.command_consumer.as_mut() {
            consumer(world, &commands);
            // Also handle spawn/destroy that the consumer doesn't process
            for cmd in &commands {
                match cmd {
                    ScriptCommand::SpawnEntity { return_index: _ } => {
                        let id = world.create_entity();
                        debug!("Script spawned entity {id}");
                    }
                    ScriptCommand::DestroyEntity(entity) => {
                        debug!("Script destroying entity {entity}");
                        world.destroy_entity(*entity);
                    }
                    _ => {}
                }
            }
        } else {
            for cmd in commands {
                match cmd {
                    ScriptCommand::SetTransform(entity, _transform) => {
                        debug!("Script set_transform for entity {entity} (no consumer)");
                    }
                    ScriptCommand::SetPosition(entity, _position) => {
                        debug!("Script set_position for entity {entity} (no consumer)");
                    }
                    ScriptCommand::SpawnEntity { return_index: _ } => {
                        let id = world.create_entity();
                        debug!("Script spawned entity {id}");
                    }
                    ScriptCommand::DestroyEntity(entity) => {
                        debug!("Script destroying entity {entity}");
                        world.destroy_entity(entity);
                    }
                }
            }
        }
    }

    fn process_destroyed(&mut self, world: &World) {
        let destroyed: Vec<EntityId> = world
            .entity_events()
            .iter()
            .filter_map(|event| match event {
                EntityEvent::Destroyed(id) => Some(*id),
                _ => None,
            })
            .collect();

        for id in destroyed {
            let handle = match world.get_component::<ScriptComponent>(id) {
                Some(script) => script.instance_handle,
                _ => continue,
            };
            if let Some(handle) = handle {
                self.engine.call_on_destroy(handle, id).ok();
                self.engine.remove_instance(handle);
                debug!("Destroyed script instance for entity {id}");
            }
        }
    }
}

impl System for ScriptSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        self.process_spawns(world);

        let active = world
            .get_resource::<ScriptsActive>()
            .map(|s| s.0)
            .unwrap_or(false);

        if !active {
            self.process_destroyed(world);
            return;
        }

        let shared = Rc::new(self.build_shared_data(world));

        let active: Vec<(ScriptInstanceHandle, EntityId)> = self
            .engine
            .instances
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| {
                opt.as_ref().map(|inst| {
                    (
                        ScriptInstanceHandle {
                            index: i as u32,
                            generation: inst.generation,
                        },
                        inst.entity,
                    )
                })
            })
            .collect();

        let mut all_commands = Vec::new();
        for (handle, entity) in active {
            let proxy = ScriptWorldProxy::from_shared(Rc::clone(&shared));
            match self
                .engine
                .execute_on_update(handle, entity, proxy, delta_time)
            {
                Ok(commands) => all_commands.extend(commands),
                Err(e) => {
                    error!("Script on_update error for entity {entity}: {e}");
                    if let Some(Some(inst)) = self.engine.instances.get(handle.index as usize)
                        && inst.generation == handle.generation
                        && inst.error_count >= MAX_SCRIPT_ERRORS
                    {
                        log::warn!(
                            "Disabling script for entity {entity} after {MAX_SCRIPT_ERRORS} errors"
                        );
                        self.engine.remove_instance(handle);
                        continue;
                    }
                }
            }
        }

        self.apply_commands(all_commands, world);

        self.process_destroyed(world);
    }
}
