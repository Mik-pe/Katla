use std::any::TypeId;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use katla_ecs::events::{ComponentEvent, EntityEvent};
use katla_ecs::{EntityId, System, World};
use katla_math::Transform;
use log::{debug, error, info};

use crate::ScriptError;
use crate::bindings::script_world::{InputSnapshot, ScriptWorldProxy, SharedWorldData};
use crate::bindings::world::ScriptCommand;
use crate::component::{ScriptComponent, ScriptInstanceHandle};
use crate::engine::ScriptEngine;
use crate::event_bus::EventBus;
use crate::watcher::ScriptWatcher;

/// Maximum number of consecutive errors before a script instance is disabled.
/// Prevents spamming the log with errors from broken scripts.
const MAX_SCRIPT_ERRORS: u32 = 10;

type TransformProvider = Box<dyn FnMut(&World) -> Vec<(EntityId, Transform)>>;
type VelocityProvider = Box<dyn FnMut(&World) -> Vec<(EntityId, katla_math::Vec3)>>;
type CommandConsumer = Box<dyn FnMut(&mut World, &[ScriptCommand])>;
type InputProvider = Box<dyn FnMut(&World) -> InputSnapshot>;
type ComponentEntitiesProvider =
    Box<dyn FnMut(&World) -> std::collections::HashMap<String, Vec<EntityId>>>;

/// Resource that controls whether scripts execute their `on_update` hooks.
/// Insert into the ECS World to signal play mode. Defaults to `false` (suspended).
///
/// When `ScriptsActive(false)` (default), scripts only process spawn/destroy events.
/// When `ScriptsActive(true)` is inserted, scripts run their `on_update` hooks each frame.
#[derive(Debug, Clone, Copy)]
pub struct ScriptsActive(pub bool);

/// Resource holding audio commands queued by scripts during the last ECS update.
/// `katla_app` drains this after `world.update()` and forwards to `AudioSystem`.
///
/// Contains commands like `PlaySound`, `PlaySoundAt`, `PlaySoundCue`.
#[derive(Default)]
pub struct PendingAudioCommands(pub Vec<ScriptCommand>);

/// Resource holding raycast results from the previous frame.
/// Scripts call `world:raycast()` to queue a command, then `world:get_raycast_result()`
/// on the next frame to retrieve the result.
///
/// The results are indexed by the return value from `world:raycast()`.
#[derive(Default)]
pub struct PendingRaycastResults(
    pub std::collections::HashMap<usize, crate::bindings::script_world::RaycastResult>,
);

/// Resource holding raycast commands queued by scripts during the last ECS update.
/// `katla_app` drains this after `world.update()`, executes raycasts against
/// `PhysicsWorld`, and stores results in `PendingRaycastResults`.
#[derive(Default)]
pub struct PendingRaycastCommands(pub Vec<crate::bindings::world::ScriptCommand>);

/// Resource holding physics force/impulse commands queued by scripts during the last ECS update.
/// `katla_app` drains this after `world.update()` and applies them to `PhysicsWorld`.
#[derive(Default)]
pub struct PendingPhysicsForceCommands(pub Vec<crate::bindings::world::ScriptCommand>);

/// Resource holding velocity commands queued by scripts during the last ECS update.
/// `katla_app` drains this after `world.update()` and applies them to `PhysicsWorld`.
#[derive(Default)]
pub struct PendingVelocityCommands(pub Vec<crate::bindings::world::ScriptCommand>);

/// Resource holding trigger overlap query results from the previous frame.
/// Scripts call `world:query_trigger_overlaps()` to queue a query, then
/// `world:get_trigger_overlaps()` on the next frame to retrieve the result.
#[derive(Default)]
pub struct PendingTriggerQueryResults(
    pub std::collections::HashMap<usize, crate::bindings::script_world::TriggerOverlapResult>,
);

/// Resource holding trigger overlap query commands queued by scripts during the last ECS update.
/// `katla_app` drains this after `world.update()`, queries `PhysicsWorld` for current
/// trigger overlaps, and stores results in `PendingTriggerQueryResults`.
#[derive(Default)]
pub struct PendingTriggerQueryCommands(pub Vec<crate::bindings::world::ScriptCommand>);

/// A collision event from the physics system.
///
/// Dispatched to scripts as `"collision_enter"` or `"collision_exit"` events.
#[derive(Debug, Clone)]
pub struct PhysicsCollisionEvent {
    /// Type of collision event.
    pub event_type: PhysicsCollisionEventType,
    /// First entity involved in the collision (raw entity ID).
    pub entity_a: u64,
    /// Second entity involved in the collision (raw entity ID).
    pub entity_b: u64,
}

/// Type of physics collision event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsCollisionEventType {
    /// Two entities have started colliding.
    CollisionEnter,
    /// Two entities have stopped colliding.
    CollisionExit,
}

/// Resource holding collision events produced by the physics system each frame.
/// `ScriptSystem` drains these and dispatches them as script events
/// (`"collision_enter"`, `"collision_exit"`).
#[derive(Default)]
pub struct PendingPhysicsEvents(pub Vec<PhysicsCollisionEvent>);

/// A single entity's script variable snapshot for the inspector.
/// Contains entity ID, script path, and variable name-value pairs.
type ScriptVarEntry = (
    katla_ecs::EntityId,
    String,
    Vec<(String, crate::engine::ScriptVarValue)>,
);

/// Resource holding script variable snapshots for the editor inspector.
/// `ScriptSystem` populates this each frame during editing mode.
///
/// The inspector UI can read this to display and edit script variables.
#[derive(Default)]
pub struct ScriptInspectorData {
    /// Per-entity script variable snapshots: (entity_id, script_path, vars)
    /// Each var is a (name, value) pair where value is Number, Boolean, or String.
    pub entries: Vec<ScriptVarEntry>,
}

/// Resource telling the script system whether to populate inspector data.
///
/// Set to `PopulateScriptInspector(true)` to enable inspector data collection.
#[derive(Default)]
pub struct PopulateScriptInspector(pub bool);

/// Resource holding pending script variable edits from the editor inspector.
/// `katla_app` pushes edits, `ScriptSystem` applies them each frame.
///
/// Each edit is a tuple of (entity_id, variable_name, new_value).
#[derive(Default)]
pub struct PendingScriptVarEdits(
    pub Vec<(katla_ecs::EntityId, String, crate::engine::ScriptVarValue)>,
);

/// The ECS system that manages script execution.
///
/// This system:
/// - Creates script instances for entities with `ScriptComponent`
/// - Executes script `on_update` hooks each frame (when `ScriptsActive(true)`)
/// - Processes script commands (transforms, spawning, audio, etc.)
/// - Handles hot-reloading of changed scripts
/// - Dispatches events between scripts
/// - Manages physics collision events
///
/// # Setup
///
/// Use the builder methods to configure the system before adding to the ECS world:
///
/// ```ignore
/// let script_system = ScriptSystem::new()
///     .with_scripts_dir("resources/scripts")
///     .with_transform_provider(|world| { ... })
///     .with_command_consumer(|world, commands| { ... });
/// ```
///
/// # Thread Safety
///
/// **Warning:** `ScriptSystem` is NOT thread-safe (`!Send + !Sync`).
/// The ECS should run this system on a single thread only.
pub struct ScriptSystem {
    engine: ScriptEngine,
    event_bus: EventBus,
    watcher: Option<ScriptWatcher>,
    transform_provider: Option<TransformProvider>,
    velocity_provider: Option<VelocityProvider>,
    command_consumer: Option<CommandConsumer>,
    input_provider: Option<InputProvider>,
    component_entities_provider: Option<ComponentEntitiesProvider>,
    /// Reusable event bus shared with script proxies across frames.
    shared_event_bus: Rc<RefCell<crate::bindings::script_world::SharedEventBus>>,
}

impl ScriptSystem {
    pub fn new() -> Result<Self, ScriptError> {
        Ok(Self {
            engine: ScriptEngine::new()?,
            event_bus: EventBus::new(),
            watcher: None,
            transform_provider: None,
            velocity_provider: None,
            command_consumer: None,
            input_provider: None,
            component_entities_provider: None,
            shared_event_bus: Rc::new(RefCell::new(
                crate::bindings::script_world::SharedEventBus::default(),
            )),
        })
    }

    /// Set the base directory for resolving bare script names.
    ///
    /// Called by the app bridge to configure where scripts live on disk.
    pub fn with_scripts_dir(mut self, dir: impl Into<String>) -> Self {
        let dir_str = dir.into();
        self.engine.set_scripts_dir(&dir_str);

        // Also start the file watcher for hot reload
        match ScriptWatcher::new(&dir_str) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(e) => error!("Failed to start script watcher: {e}"),
        }

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

    /// Set the velocity snapshot provider. Called by the app bridge.
    ///
    /// The closure is invoked each frame to gather `(EntityId, Vec3)` pairs
    /// from the world, which scripts can then read via `world:get_velocity()`.
    pub fn with_velocity_provider<F>(mut self, f: F) -> Self
    where
        F: FnMut(&World) -> Vec<(EntityId, katla_math::Vec3)> + 'static,
    {
        self.velocity_provider = Some(Box::new(f));
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

    pub fn with_component_entities_provider<F>(mut self, f: F) -> Self
    where
        F: FnMut(&World) -> std::collections::HashMap<String, Vec<EntityId>> + 'static,
    {
        self.component_entities_provider = Some(Box::new(f));
        self
    }

    /// Set the per-call script execution timeout.
    ///
    /// Scripts that exceed this duration are aborted.
    pub fn with_timeout(self, timeout: Duration) -> Self {
        self.engine.set_timeout(timeout);
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

        let velocities = match self.velocity_provider.as_mut() {
            Some(provider) => provider(world),
            None => Vec::new(),
        };

        let input = match self.input_provider.as_mut() {
            Some(provider) => provider(world),
            None => InputSnapshot::default(),
        };

        let component_entities = match self.component_entities_provider.as_mut() {
            Some(provider) => provider(world),
            None => std::collections::HashMap::new(),
        };

        let live_entities = world.entity_ids().collect();

        let raycast_results = world
            .get_resource::<PendingRaycastResults>()
            .map(|r| r.0.clone())
            .unwrap_or_default();

        let trigger_overlap_results = world
            .get_resource::<PendingTriggerQueryResults>()
            .map(|r| r.0.clone())
            .unwrap_or_default();

        SharedWorldData {
            transforms: transforms.into_iter().collect(),
            velocities: velocities.into_iter().collect(),
            live_entities,
            component_entities,
            input_state: input,
            raycast_results,
            trigger_overlap_results,
        }
    }

    fn apply_commands(&mut self, commands: Vec<ScriptCommand>, world: &mut World) {
        let mut audio_cmds = Vec::new();
        let mut raycast_cmds = Vec::new();
        let mut force_cmds = Vec::new();
        let mut velocity_cmds = Vec::new();
        let mut trigger_query_cmds = Vec::new();
        let mut core_cmds = Vec::new();
        for cmd in commands {
            match &cmd {
                ScriptCommand::PlaySound { .. }
                | ScriptCommand::PlaySoundAt { .. }
                | ScriptCommand::PlaySoundCue { .. } => {
                    audio_cmds.push(cmd);
                }
                ScriptCommand::Raycast { .. } => {
                    raycast_cmds.push(cmd);
                }
                ScriptCommand::ApplyForce { .. } | ScriptCommand::ApplyImpulse { .. } => {
                    force_cmds.push(cmd);
                }
                ScriptCommand::SetVelocity { .. } => {
                    velocity_cmds.push(cmd);
                }
                ScriptCommand::QueryTriggerOverlaps { .. } => {
                    trigger_query_cmds.push(cmd);
                }
                _ => core_cmds.push(cmd),
            }
        }

        if !audio_cmds.is_empty()
            && let Some(pending) = world.get_resource_mut::<PendingAudioCommands>()
        {
            pending.0.extend(audio_cmds);
        }

        if !raycast_cmds.is_empty()
            && let Some(pending) = world.get_resource_mut::<PendingRaycastCommands>()
        {
            pending.0.extend(raycast_cmds);
        }

        if !force_cmds.is_empty()
            && let Some(pending) = world.get_resource_mut::<PendingPhysicsForceCommands>()
        {
            pending.0.extend(force_cmds);
        }

        if !velocity_cmds.is_empty()
            && let Some(pending) = world.get_resource_mut::<PendingVelocityCommands>()
        {
            pending.0.extend(velocity_cmds);
        }

        if !trigger_query_cmds.is_empty()
            && let Some(pending) = world.get_resource_mut::<PendingTriggerQueryCommands>()
        {
            pending.0.extend(trigger_query_cmds);
        }

        if let Some(consumer) = self.command_consumer.as_mut() {
            consumer(world, &core_cmds);
            for cmd in &core_cmds {
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
            for cmd in core_cmds {
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
                    ScriptCommand::PlaySound { .. }
                    | ScriptCommand::PlaySoundAt { .. }
                    | ScriptCommand::PlaySoundCue { .. }
                    | ScriptCommand::Raycast { .. }
                    | ScriptCommand::ApplyForce { .. }
                    | ScriptCommand::ApplyImpulse { .. }
                    | ScriptCommand::SetVelocity { .. }
                    | ScriptCommand::QueryTriggerOverlaps { .. } => {}
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

    /// Poll the file watcher and hot-reload any changed scripts.
    fn process_hot_reload(&mut self, world: &mut World) {
        let changed_scripts = match self.watcher.as_mut() {
            Some(watcher) => watcher.poll_changes(),
            None => return,
        };

        for script_path in changed_scripts {
            info!("Hot reloading script: {script_path}");

            if let Err(e) = self.engine.reload_script(&script_path) {
                error!("Failed to reload script '{script_path}': {e}");
                continue;
            }

            let reloaded_handles = self.engine.hot_reload_instances(&script_path);

            // Update ScriptComponent handles in the ECS world since
            // hot_reload_instances removes old instances and creates new ones
            for handle in &reloaded_handles {
                let entity = self
                    .engine
                    .instances
                    .get(handle.index as usize)
                    .and_then(|opt| opt.as_ref())
                    .map(|inst| inst.entity);
                if let Some(entity) = entity
                    && let Some(comp) = world.get_component_mut::<ScriptComponent>(entity)
                {
                    comp.instance_handle = Some(*handle);
                }
            }
        }
    }

    /// Drain pending events from the event bus and dispatch to registered script handlers.
    fn process_events(&mut self) {
        let events = self.event_bus.drain_pending();
        if events.is_empty() {
            return;
        }

        for event in events {
            let handler_count = self.event_bus.handlers(&event.name).len();
            let handler_indices: Vec<usize> = (0..handler_count).collect();
            for idx in handler_indices {
                let handlers = self.event_bus.handlers(&event.name);
                let handler_key = match handlers.get(idx) {
                    Some(k) => k,
                    None => continue,
                };
                let func: Result<mlua::Function, _> = self.engine.vm.registry_value(handler_key);
                let func = match func {
                    Ok(f) => f,
                    Err(_) => continue,
                };

                self.engine.reset_instruction_counter();
                if let Err(e) = func.call::<()>((event.name.clone(), event.data.clone())) {
                    error!("Event handler for '{}' failed: {e}", event.name);
                }
            }
        }
    }

    /// Flush pending emits and subscriptions from a SharedEventBus into the real EventBus.
    fn flush_script_events(
        &mut self,
        shared_bus: &Rc<RefCell<crate::bindings::script_world::SharedEventBus>>,
    ) {
        let mut bus = shared_bus.borrow_mut();

        // Flush subscriptions first so handlers are registered before events arrive
        for (name, key) in bus.pending_subscriptions.drain(..) {
            self.event_bus.subscribe(name, key);
        }

        // Flush emitted events into the real event bus
        for (name, data) in bus.pending_emits.drain(..) {
            self.event_bus.emit(name, data);
        }
    }

    /// Drain pending physics collision events and emit them as script events.
    fn dispatch_physics_events(&mut self, world: &mut World) {
        let events: Vec<PhysicsCollisionEvent> =
            match world.get_resource_mut::<PendingPhysicsEvents>() {
                Some(r) => std::mem::take(&mut r.0),
                None => return,
            };

        if events.is_empty() {
            return;
        }

        let has_collision_enter = !self.event_bus.handlers("collision_enter").is_empty();
        let has_collision_exit = !self.event_bus.handlers("collision_exit").is_empty();

        if !has_collision_enter && !has_collision_exit {
            return;
        }

        for event in events {
            let event_name = match event.event_type {
                PhysicsCollisionEventType::CollisionEnter => "collision_enter",
                PhysicsCollisionEventType::CollisionExit => "collision_exit",
            };

            // Skip if no handlers registered for this event type
            if self.event_bus.handlers(event_name).is_empty() {
                continue;
            }

            let table = match self.engine.vm.create_table() {
                Ok(t) => t,
                Err(e) => {
                    error!("Failed to create physics event table: {e}");
                    continue;
                }
            };

            if let Err(e) = table.set("entity_a", event.entity_a) {
                error!("Failed to set entity_a: {e}");
                continue;
            }
            if let Err(e) = table.set("entity_b", event.entity_b) {
                error!("Failed to set entity_b: {e}");
                continue;
            }

            self.event_bus
                .emit(event_name.to_string(), mlua::Value::Table(table));
        }
    }
}

impl System for ScriptSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        self.process_hot_reload(world);
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

        // Clear the reusable shared event bus from previous frame
        {
            let mut bus = self.shared_event_bus.borrow_mut();
            bus.pending_emits.clear();
            bus.pending_subscriptions.clear();
        }

        // Collect active handles in a single pass
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

        let mut all_commands = Vec::with_capacity(active.len());

        for (handle, entity) in active {
            let mut proxy = ScriptWorldProxy::from_shared(Rc::clone(&shared));
            proxy.with_event_bus(Rc::clone(&self.shared_event_bus), &self.engine.vm);
            match self
                .engine
                .execute_on_update(handle, entity, proxy, delta_time)
            {
                Ok(commands) => {
                    all_commands.extend(commands);
                }
                Err(e) => {
                    error!("Entity {entity}: {e}");
                    if let Some(Some(inst)) = self.engine.instances.get(handle.index as usize)
                        && inst.generation == handle.generation
                        && inst.error_count >= MAX_SCRIPT_ERRORS
                    {
                        log::warn!(
                            "Disabling script for entity {entity} after {MAX_SCRIPT_ERRORS} errors"
                        );
                        self.engine.remove_instance(handle);
                    }
                }
            }
        }

        // Flush all pending events from scripts in a single batch
        let event_bus = Rc::clone(&self.shared_event_bus);
        self.flush_script_events(&event_bus);

        self.apply_commands(all_commands, world);

        self.dispatch_physics_events(world);
        self.process_events();
        self.process_destroyed(world);
        self.apply_script_var_edits(world);
        self.populate_inspector_data(world);
    }
}

impl ScriptSystem {
    /// Apply pending script variable edits from the editor inspector.
    fn apply_script_var_edits(&mut self, world: &mut World) {
        let edits: Vec<(katla_ecs::EntityId, String, crate::engine::ScriptVarValue)> = world
            .get_resource_mut::<PendingScriptVarEdits>()
            .map(|r| std::mem::take(&mut r.0))
            .unwrap_or_default();

        for (entity, var_name, value) in edits {
            let handle = match world.get_component::<ScriptComponent>(entity) {
                Some(comp) => comp.instance_handle,
                None => continue,
            };
            let Some(handle) = handle else { continue };
            if let Err(e) = self.engine.set_instance_var(handle, &var_name, value) {
                debug!("Failed to set script var '{var_name}' on entity {entity}: {e}");
            }
        }
    }

    /// Populate ScriptInspectorData for the editor if requested.
    fn populate_inspector_data(&mut self, world: &mut World) {
        let should_populate = world
            .get_resource::<PopulateScriptInspector>()
            .map(|p| p.0)
            .unwrap_or(false);
        if !should_populate {
            return;
        }

        let mut entries = Vec::new();
        for (idx, instance) in self.engine.instances.iter().enumerate() {
            let Some(inst) = instance else { continue };
            let handle = ScriptInstanceHandle {
                index: idx as u32,
                generation: inst.generation,
            };
            if let Ok(vars) = self.engine.inspect_instance_vars(handle) {
                entries.push((inst.entity, inst.script_path.clone(), vars));
            }
        }

        if let Some(inspector) = world.get_resource_mut::<ScriptInspectorData>() {
            inspector.entries = entries;
        }
    }
}
