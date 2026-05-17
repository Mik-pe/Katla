# Katla Script Architecture: Luau Scripting via mlua

## Executive Summary

This document proposes a `katla_script` crate that embeds a Luau VM (via `mlua` with the `luau` feature flag) into the Katla engine. Scripts attach to entities via a `ScriptComponent`, receive lifecycle hooks (`on_spawn`, `on_update`, `on_destroy`), and access ECS components through a sandboxed API bridge.

---

## 1. mlua + Luau Viability

### Status: Production-Ready

- **mlua v0.11.x** (latest: 0.11.6 as of Jan 2026) supports Luau via the `luau` feature flag.
- The Luau VM is compiled from C++ and statically linked by `mlua-sys`. No external runtime dependency.
- `mlua` supports: `UserData` trait for custom Rust types, async/await, Luau's require-by-string system, and sandboxed environments.
- The `mluau` crate exists as a community fork focused on Luau-first ergonomics, but `mlua` is more mature and battle-tested. **Recommendation: use `mlua`.**

### Cargo.toml entry:
```toml
[dependencies]
mlua = { version = "0.11", features = ["luau", "vendored", "serialize"] }
katla_ecs = { path = "../katla_ecs" }
katla_math = { path = "../katla_math" }
```

- `vendored`: Compiles Luau from source (CMake + C++). Required since Luau isn't a system library.
- `serialize`: Enables `serde` interop (Lua tables ↔ Rust structs via `mlua::SerializeVec`).
- No `async` feature needed -- Katla's scripting is synchronous per-frame.

### Known Gotchas

1. **Build time**: Luau is C++ and takes ~30-60s to compile from source. Only affects first build.
2. **`unsafe` requirement**: `mlua` internally uses unsafe. The API is designed to be safe from the caller's perspective, but `UserData` callbacks receive `&Lua` and `&T`/`&mut T` -- you must not leak references out of callbacks.
3. **Luau `require`**: Luau has a built-in module system via `require`. Must configure `Lua::new()` with `mlua::StdLib::ALL_SAFE` and set up a sandboxed require loader.
4. **No `__gc`**: Luau does not support Lua's `__gc` metamethod. Instead it uses tag-based destructors only available to the host. This is fine -- we handle cleanup in Rust when entities are destroyed.

---

## 2. Architecture from Other Engines

### Fyrox
- Uses **Rust itself** as the scripting language via dynamic library hot-reloading (libreload).
- Has a `ScriptTrait` with lifecycle methods: `on_init`, `on_start`, `on_update`, `on_destroy`, `on_collision`, etc.
- Scripts are scene node attachments -- each node can have one script instance.
- Strongly typed access to node properties via a `Visit` trait (reflection/serialization).

### bevy_mod_scripting
- Attaches scripts as ECS components (`ScriptComponent` with a list of script handles).
- Uses an **event-driven** model: script lifecycle hooks fire as Bevy events dispatched at different stages.
- Supports Lua (via mlua), Rhai, and Rune.
- Provides a `ReflectReference` system for typed access to ECS components from scripts.
- Scripts communicate through a shared world context passed to each hook.

### Katla's Approach (Proposed)
- **Script component** pattern (like both Fyrox and bevy_mod_scripting).
- **Direct ECS access** through a `ScriptWorld` proxy object exposed to Luau.
- **Lifecycle hooks** as well-known Luau function names in each script file.
- **Single VM** shared across all script instances (for memory efficiency), with per-script environments for isolation.

---

## 3. ECS + Scripting Patterns

### Script Component

```rust
// In katla_script/src/component.rs

#[derive(Component)]
pub struct ScriptComponent {
    /// Path to the .luau script file (e.g., "scripts/player_controller")
    pub script_path: String,
    /// Per-entity script state (Lua table stored as a reference)
    pub(crate) instance_handle: Option<ScriptInstanceHandle>,
}

/// Opaque handle to a script instance within the VM.
/// Stores the index into ScriptEngine::instances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptInstanceHandle(pub(crate) u32);
```

### Script Lifecycle Hooks

Scripts implement hooks as top-level functions in the Luau file:

```lua
-- scripts/player_controller.luau
local speed: number = 5.0

function on_spawn(entity: Entity, world: World)
    -- Called when the entity is first spawned with this script
    world:set_tag(entity, "Player")
end

function on_update(entity: Entity, world: World, dt: number)
    local transform = world:get_transform(entity)
    if world:is_action_pressed("move_forward") then
        transform.position.z = transform.position.z - speed * dt
    end
    world:set_transform(entity, transform)
end

function on_destroy(entity: Entity, world: World)
    -- Called when the entity is being destroyed
end
```

### Script Engine (VM Manager)

```rust
// In katla_script/src/engine.rs

use mlua::{Lua, LuaOptions, StdLib};
use std::collections::HashMap;

use crate::component::ScriptInstanceHandle;

pub struct ScriptEngine {
    /// The single Luau VM instance.
    vm: Lua,
    /// Loaded script modules: path -> compiled chunk.
    loaded_scripts: HashMap<String, mlua::RegistryKey>,
    /// Per-entity script instances: handle -> (script_path, entity_id, env_table_key).
    instances: Vec<Option<ScriptInstance>>,
    /// Filesystem watch state for hot reload.
    script_timestamps: HashMap<String, std::time::Instant>,
}

struct ScriptInstance {
    pub script_path: String,
    pub entity: katla_ecs::EntityId,
    /// Registry key for the script's environment table (holds state).
    pub env_key: mlua::RegistryKey,
    /// Registry keys for the hook functions.
    pub hooks: ScriptHooks,
}

struct ScriptHooks {
    pub on_spawn: Option<mlua::RegistryKey>,
    pub on_update: Option<mlua::RegistryKey>,
    pub on_destroy: Option<mlua::RegistryKey>,
}
```

### Script System (ECS System)

```rust
// In katla_script/src/system.rs

pub struct ScriptSystem {
    engine: ScriptEngine,
}

impl System for ScriptSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        // 1. Process newly spawned entities with ScriptComponent -> call on_spawn
        // 2. Call on_update for all active script instances
        // 3. Process destroyed entities with ScriptComponent -> call on_destroy
        // 4. Check for hot reload changes
        self.engine.tick(world, delta_time);
    }
}
```

---

## 4. Type Registration

### Registering katla_math types as Luau UserData

```rust
// In katla_script/src/bindings/math.rs

use mlua::{UserData, UserDataFields, UserDataMethods, MetaMethod};
use katla_math::{Vec3, Transform, Quat, Color};

impl UserData for Vec3 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("x", |_, this| Ok(this.x()));
        fields.add_field_method_get("y", |_, this| Ok(this.y()));
        fields.add_field_method_get("z", |_, this| Ok(this.z()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Add, |_, this, other: Vec3| {
            Ok(*this + other)
        });
        methods.add_meta_method(MetaMethod::Sub, |_, this, other: Vec3| {
            Ok(*this - other)
        });
        methods.add_meta_method(MetaMethod::Mul, |_, this, scalar: f32| {
            Ok(*this * scalar)
        });
        methods.add_method("length", |_, this, ()| Ok(this.length()));
        methods.add_method("normalize", |_, this, ()| Ok(this.normalize()));
        methods.add_method("dot", |_, this, other: Vec3| Ok(this.dot(other)));
        methods.add_method("cross", |_, this, other: Vec3| Ok(this.cross(other)));
    }
}

impl UserData for Transform {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("position", |_, this| Ok(this.position));
        fields.add_field_method_set("position", |_, this, val: Vec3| {
            this.position = val;
            Ok(())
        });
        fields.add_field_method_get("scale", |_, this| Ok(this.scale));
        fields.add_field_method_set("scale", |_, this, val: Vec3| {
            this.scale = val;
            Ok(())
        });
        fields.add_field_method_get("rotation", |_, this| Ok(this.rotation));
        fields.add_field_method_set("rotation", |_, this, val: Quat| {
            this.rotation = val;
            Ok(())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("forward", |_, this, ()| Ok(this.forward()));
        methods.add_method("up", |_, this, ()| Ok(this.up()));
        methods.add_method("right", |_, this, ()| Ok(this.right()));
        methods.add_method("look_at", |_, this, target: Vec3| {
            Ok(this.look_at(target, Vec3::new(0.0, 1.0, 0.0)))
        });
    }
}
```

### EntityId as UserData

```rust
// In katla_script/src/bindings/entity.rs

use katla_ecs::EntityId;

impl UserData for EntityId {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, ()| Ok(this.id()));
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("{}", this))
        });
    }
}
```

### ScriptWorld Proxy (Bridge from Luau to ECS)

```rust
// In katla_script/src/bindings/world.rs

/// A proxy object passed to script hooks.
/// Provides safe, sandboxed access to the ECS world.
pub struct ScriptWorld<'a> {
    world: &'a mut katla_ecs::World,
}

impl UserData for ScriptWorld<'_> {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get_transform", |_, this, entity: EntityId| {
            // Read the TransformComponent from the ECS world
            let transform = this.world.get_component::<TransformComponent>(entity)
                .map(|c| c.transform);
            Ok(transform)
        });

        methods.add_method_mut("set_transform", |_, this, (entity, transform): (EntityId, Transform)| {
            if let Some(mut comp) = this.world.get_component_mut::<TransformComponent>(entity) {
                comp.transform = transform;
            }
            Ok(())
        });

        methods.add_method_mut("spawn_entity", |_, this, ()| {
            Ok(this.world.create_entity())
        });

        methods.add_method_mut("destroy_entity", |_, this, entity: EntityId| {
            this.world.destroy_entity(entity);
            Ok(())
        });

        methods.add_method("entity_exists", |_, this, entity: EntityId| {
            Ok(this.world.entity_exists(entity))
        });

        // ... more methods for adding/removing components, etc.
    }
}
```

**Important design note**: `ScriptWorld` takes `&mut World` and is NOT stored in the Lua registry. It is created fresh each frame and passed as a parameter to hook functions. This avoids lifetime issues and ensures scripts cannot hold stale references to the world.

---

## 5. Hot Reload Architecture

### Approach: Environment Swapping

1. **Monitor script files**: Use filesystem timestamps or a file watcher (e.g., `notify` crate) to detect changes.
2. **Recompile the chunk**: Load the new `.luau` file and compile it.
3. **Create a new environment table**: For each entity running the changed script:
   - Create a new environment table for the script.
   - Copy **only serializable state** from the old environment (numbers, strings, booleans, simple tables).
   - Functions and userdata references are NOT carried over (they reference old code).
4. **Swap the instance**: Replace the old instance's env table with the new one.
5. **Call `on_spawn`** on the new instance (optional, configurable).

### Simplified Alternative (MVP)

For the initial implementation, skip state preservation:
- On file change, destroy all instances of the old script and recreate them.
- Call `on_spawn` on each new instance.
- This is how most game engines handle hot reload in development.

### Implementation sketch:

```rust
impl ScriptEngine {
    pub fn hot_reload_script(&mut self, path: &str) -> Result<(), ScriptError> {
        // 1. Load and compile the new script
        let new_chunk = self.vm.load(fs::read_to_string(path)?)
            .set_name(path)?
            .into_function()?;

        // 2. Update the loaded script cache
        let new_key = self.vm.create_registry_value(new_chunk)?;
        self.loaded_scripts.insert(path.to_string(), new_key);

        // 3. For each instance using this script, create a new environment
        for instance in &mut self.instances {
            if let Some(inst) = instance.as_mut() {
                if inst.script_path == path {
                    let old_env_key = &inst.env_key;
                    // Preserve scalar state from old env
                    let preserved_state = self.preserve_state(old_env_key)?;
                    // Create new env, apply preserved state, extract new hooks
                    self.recreate_instance(inst, preserved_state)?;
                }
            }
        }
        Ok(())
    }
}
```

---

## 6. Performance

### Overhead Estimates

Based on the `script-bench-rs` benchmark (by the mlua author, khvzak):

| Operation | Approximate Cost |
|-----------|-----------------|
| Call a Lua function (no args) | ~50-100ns |
| Call a Lua function (3 args) | ~80-150ns |
| Create a Lua table | ~30-50ns |
| Access UserData field | ~20-40ns |
| Luau compiled chunk execution | ~10-30ns overhead |

### 1000 entities with `on_update(dt)`:

- Each call: ~100ns for the function call + script body execution
- **Call overhead alone**: ~100μs (0.1ms) for 1000 entities
- With a simple script body (~200ns each): ~300μs (0.3ms) total
- **At 60fps (16.6ms budget)**: This is <2% of the frame budget. Very acceptable.

### Mitigation Strategies

1. **Only call scripts that have `on_update`**: Check for hook existence before calling.
2. **Batch processing**: Group scripts by type to improve cache locality.
3. **Yield-based scheduling**: For expensive scripts, use Luau coroutines to spread work across frames.
4. **Entity culling**: Skip scripts on entities outside the camera frustum.

### Luau-Specific Performance Features

- **Bytecode compilation**: Luau compiles to bytecode ahead of execution (fast).
- **Inline caching**: Luau uses inline caches for table access and method calls.
- **No `__gc` overhead**: Luau's tag-based destructors are cheaper than Lua's finalizers.
- **Optimized interpreter**: Luau's interpreter uses computed goto (where available) for ~20% faster dispatch vs standard Lua.

---

## 7. Security/Sandboxing

### Luau's Built-in Sandboxing

Luau is designed for sandboxing (originally built for Roblox):
- No `io`, `os`, `debug` libraries exposed by default
- No `dofile`, `loadfile`, or `require` to the filesystem
- Read-only standard libraries (`math`, `string`, `table`, `coroutine`)
- No `__gc` metamethod (prevents finalizer-based attacks)
- VM-level interrupt mechanism for runaway script detection

### What Katla Should Expose

**Expose:**
- `math` (Luau built-in)
- `string` (Luau built-in)
- `table` (Luau built-in)
- `coroutine` (Luau built-in, for async patterns)
- `Entity` type (userdata)
- `World` proxy (with controlled methods)
- `Vec3`, `Transform`, `Quat`, `Color` types
- `print` → `log::info!` bridge (debug builds only)
- `warn` → `log::warn!` bridge

**DO NOT Expose:**
- `io` (filesystem access)
- `os` (system calls, environment variables)
- `debug` (internal VM inspection)
- `require` (unless implementing a sandboxed module loader)
- `getfenv`/`setfenv` (environment manipulation)
- `rawget`/`rawset` (bypass metamethods)
- Direct Rust panic/unwrap pathways

### VM Initialization

```rust
fn create_sandboxed_vm() -> Lua {
    // Luau mode: no unsafe stdlib
    let lua = Lua::new_with(
        StdLib::ALL_SAFE,  // Excludes io, os, debug, etc.
        LuaOptions::default(),
    ).expect("Failed to create Luau VM");

    // Register our custom types
    lua.register_type::<Vec3>()?;
    lua.register_type::<Transform>()?;
    lua.register_type::<Quat>()?;
    lua.register_type::<EntityId>()?;
    // ScriptWorld is NOT registered as a type -- it's passed as a parameter

    lua
}
```

### Interrupt Watchdog

```rust
// Set a script execution timeout
lua.set_interrupt(Some(Box::new(|_| {
    // Called periodically during script execution
    // Return Err to abort the script
    static INSTRUCTION_COUNT: AtomicU64 = AtomicU64::new(0);
    let count = INSTRUCTION_COUNT.fetch_add(1, Ordering::Relaxed);
    if count > 10_000_000 {
        Err(mlua::Error::runtime("Script execution timeout"))
    } else {
        Ok(())
    }
})));
```

---

## 8. Concrete Module Structure

```
katla_script/
├── Cargo.toml
├── AGENTS.md
└── src/
    ├── lib.rs              -- Public API, re-exports
    ├── component.rs        -- ScriptComponent, ScriptInstanceHandle
    ├── engine.rs           -- ScriptEngine (VM management, instance lifecycle)
    ├── system.rs           -- ScriptSystem (ECS System impl)
    ├── error.rs            -- ScriptError enum
    ├── hot_reload.rs       -- File watching, script reloading
    └── bindings/
        ├── mod.rs          -- register_all() function
        ├── entity.rs       -- EntityId UserData impl
        ├── math.rs         -- Vec3, Quat, Transform, Color UserData impls
        ├── world.rs        -- ScriptWorld proxy (read/write components, spawn/destroy)
        └── input.rs        -- Input query bindings (is_action_pressed, etc.)
```

### Cargo.toml

```toml
[package]
name = "katla_script"
version.workspace = true
edition = "2024"

[dependencies]
mlua = { version = "0.11", features = ["luau", "vendored", "serialize"] }
katla_ecs = { path = "../katla_ecs" }
katla_math = { path = "../katla_math" }
log = { workspace = true }
katla_derive = { path = "../katla_derive" }
```

### Public API (lib.rs)

```rust
pub mod bindings;
pub mod component;
pub mod engine;
pub mod error;
pub mod system;

// Re-exports
pub use component::{ScriptComponent, ScriptInstanceHandle};
pub use engine::ScriptEngine;
pub use error::ScriptError;
pub use system::ScriptSystem;
```

---

## 9. Integration with katla_app

### Where katla_script Fits in the Dependency Graph

```
katla_math  ←  katla_script  ←  katla_app
katla_ecs   ←  katla_script
```

This respects the existing dependency rules:
- `katla_script` depends on `katla_ecs` and `katla_math` (both allowed)
- `katla_script` does NOT depend on `katla_gfx`, `katla_ui`, or `katla_app`
- `katla_app` adds `katla_script` as a dependency

### Changes to katla_app

1. **Add dependency** in `katla_app/Cargo.toml`:
   ```toml
   katla_script = { path = "../katla_script" }
   ```

2. **Register the ScriptSystem** in the Application builder (alongside PhysicsSystem, TransformHierarchySystem, etc.):
   ```rust
   // In application/builder.rs or init.rs
   world.add_system(
       Box::new(ScriptSystem::new()),
       SystemExecutionOrder::NORMAL,
   );
   ```

3. **Input access from scripts**: The `ScriptWorld` proxy reads input state through a Resource:
   ```rust
   // Scripts access input through the world's resource system
   world.insert_resource(InputState { ... });
   // In ScriptWorld bindings:
   methods.add_method("is_action_pressed", |_, this, action: String| {
       let input = this.world.get_resource::<InputState>();
       Ok(input.map(|i| i.is_action_pressed(&action)).unwrap_or(false))
   });
   ```

4. **Script file discovery**: Use `ResourceManager::discover()` to find the `scripts/` directory alongside `resources/`.

5. **Scene serialization**: `ScriptComponent` must implement `Serialize`/`Deserialize` for scene files:
   ```ron
   // In a .scene file:
   ScriptComponent(
       script_path: "scripts/player_controller",
   )
   ```

### System Execution Order

```
FIRST:   InputSystem
EARLY:   ScriptSystem::on_spawn  (process newly spawned entities)
NORMAL:  PhysicsSystem
NORMAL:  ScriptSystem::on_update (per-frame script logic)
NORMAL:  TransformHierarchySystem
LATE:    AnimationSystem
LAST:    ScriptSystem::on_destroy (cleanup destroyed entities)
```

This could be split into two systems (ScriptSpawnSystem at EARLY, ScriptUpdateSystem at NORMAL, ScriptDestroySystem at LATE) or handled in a single system that processes events in order.

---

## 10. Potential Blockers & Risks

### Hard Problems

1. **`ScriptWorld` and `&mut World` in UserData callbacks**: The biggest design challenge. `mlua` callbacks receive `&Lua` and the userdata. We need mutable access to `World` from within callbacks. Solutions:
   - **Use `&mut World` through `RefCell`**: Wrap the world reference in a `RefCell<UnsafeWorldCell>` and pass a handle. This is the approach used by bevy_mod_scripting.
   - **Command queue pattern**: Script callbacks push commands (spawn, destroy, set_component) into a queue. The system applies them after all scripts run. This is safer but less immediate.
   - **Recommended**: Command queue pattern for MVP. It avoids all aliasing issues and is how most production engines handle it.

2. **Component access from scripts**: Scripts need to read/write arbitrary component types. But components are generic (`get_component::<T>`), and Luau is dynamically typed. Solutions:
   - **Hardcoded bindings**: Only expose specific known component types (Transform, Velocity, etc.) through typed methods on ScriptWorld. Simple, safe, sufficient for MVP.
   - **Reflection-based**: Implement a reflection system (like Fyrox's `Visit` trait) and expose generic get/set. Much more complex.
   - **Recommended**: Hardcoded bindings for MVP. Add reflection later if needed.

3. **Build complexity**: Luau is C++ and requires CMake. The `vendored` feature handles this, but:
   - First build takes ~30-60s extra.
   - CI needs C++ toolchain (usually present on GitHub Actions).
   - Cross-compilation may need additional setup.

4. **Thread safety**: `Lua` is `!Send` and `!Sync`. The ScriptSystem must run on the main thread. This is fine for Katla's single-threaded ECS update model but would need thought if parallel ECS updates are introduced.

5. **Error handling in scripts**: A script error should not crash the engine. All hook calls must be wrapped in `catch_unwind`-equivalent error handling:
   ```rust
   if let Err(e) = hook.call::<()>(entity, &world_proxy, dt) {
       log::error!("Script error in {}: {}", instance.script_path, e);
       // Optionally disable the script instance
   }
   ```

### Recommended Implementation Order

| Phase | Scope | Estimated Effort |
|-------|-------|-----------------|
| **Phase 1** | Crate skeleton, mlua vendored build, `ScriptComponent`, `ScriptEngine` with `on_update` only, `Vec3`/`Transform` bindings, command queue | ~3-4 days |
| **Phase 2** | Full lifecycle hooks (`on_spawn`, `on_destroy`), `EntityId` bindings, entity spawn/destroy from scripts | ~2-3 days |
| **Phase 3** | Input bindings (`is_action_pressed`), scene serialization for ScriptComponent, error handling/recovery | ~2 days |
| **Phase 4** | Hot reload (file watching, environment swap), sandboxing hardening, interrupt watchdog | ~2-3 days |
| **Phase 5** | Performance profiling, batch optimization, `Color`/`Quat` bindings, script-to-script communication (events) | ~2-3 days |
| **Phase 6** | Editor integration (script inspector, script file browser, console output), Luau type definitions for autocomplete | ~3-5 days |

### What to Build First

1. **The crate skeleton** with `mlua` building successfully.
2. **`ScriptEngine`** that can load and execute a trivial Luau script.
3. **`ScriptComponent`** + `ScriptSystem` that calls `on_update`.
4. **`Vec3` and `Transform` UserData** bindings.
5. **Command queue** for world mutations from scripts.

This gives you a minimal working pipeline: write a `.luau` file, attach it to an entity, see it run per-frame.

---

## Appendix A: Full ScriptWorld with Command Queue

```rust
// In katla_script/src/bindings/world.rs

use katla_ecs::{EntityId, World};
use katla_math::{Transform, Vec3};

/// Commands queued by script callbacks, applied after all scripts run.
pub enum ScriptCommand {
    SetTransform(EntityId, Transform),
    SpawnEntity { return_index: usize },
    DestroyEntity(EntityId),
    SetPosition(EntityId, Vec3),
}

/// Proxy passed to script hooks. Queues commands instead of directly mutating World.
pub struct ScriptWorld {
    commands: Vec<ScriptCommand>,
    // Read-only access to world data (via raw pointers, safe because we're
    // in a single-threaded context during system update)
    world_ptr: *mut World,
}

impl ScriptWorld {
    pub fn new(world: &mut World) -> Self {
        Self {
            commands: Vec::new(),
            world_ptr: world as *mut World,
        }
    }

    /// Read a component. Safe because scripts only read during hook execution.
    pub fn get_transform(&self, entity: EntityId) -> Option<Transform> {
        unsafe { &*self.world_ptr }
            .get_component::<katla_app::components::transform::TransformComponent>(entity)
            .map(|c| c.transform)
    }

    /// Queue a transform write.
    pub fn set_transform(&mut self, entity: EntityId, transform: Transform) {
        self.commands.push(ScriptCommand::SetTransform(entity, transform));
    }

    /// Consume all commands and apply them to the world.
    pub fn apply_commands(self, world: &mut World) -> Vec<EntityId> {
        let mut spawned = Vec::new();
        for cmd in self.commands {
            match cmd {
                ScriptCommand::SetTransform(eid, t) => {
                    if let Some(mut comp) = world.get_component_mut::<katla_app::components::transform::TransformComponent>(eid) {
                        comp.transform = t;
                    }
                }
                ScriptCommand::SpawnEntity { return_index } => {
                    let id = world.create_entity();
                    // Store spawned entity ID for the script to retrieve
                    spawned.insert(return_index, id);
                }
                ScriptCommand::DestroyEntity(eid) => {
                    world.destroy_entity(eid);
                }
                ScriptCommand::SetPosition(eid, pos) => {
                    if let Some(mut comp) = world.get_component_mut::<katla_app::components::transform::TransformComponent>(eid) {
                        comp.transform.position = pos;
                    }
                }
            }
        }
        spawned
    }
}
```

**Important**: The command queue approach means `ScriptWorld` needs to NOT be a `UserData` type registered in the VM. Instead, it's created as a Rust-side proxy and passed to Lua functions as a parameter. This requires using `mlua::Function::call()` with the proxy as an argument, where the proxy is converted using `IntoLua`.

```rust
// How to call a script hook:
let script_world = ScriptWorld::new(world);
let on_update: mlua::Function = /* get from registry */;
on_update.call::<()>(entity_id, &script_world, delta_time)?;
script_world.apply_commands(world);
```

For this to work, `ScriptWorld` must implement `mlua::UserData` (read-only methods only, since the command queue is internal):

```rust
impl UserData for ScriptWorld {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get_transform", |_, this, entity: EntityId| {
            Ok(this.get_transform(entity))
        });
        methods.add_method("is_action_pressed", |_, this, action: String| {
            let world = unsafe { &*this.world_ptr };
            let input = world.get_resource::<InputState>();
            Ok(input.map(|i| i.is_action_pressed(&action)).unwrap_or(false))
        });
        methods.add_method_mut("set_transform", |_, this, (entity, transform): (EntityId, Transform)| {
            this.set_transform(entity, transform);
            Ok(())
        });
        methods.add_method_mut("spawn_entity", |_, this, ()| {
            let idx = this.commands.len();
            this.commands.push(ScriptCommand::SpawnEntity { return_index: idx });
            Ok(idx) // Script gets a ticket ID, resolves later
        });
        methods.add_method_mut("destroy_entity", |_, this, entity: EntityId| {
            this.destroy_entity(entity);
            Ok(())
        });
    }
}
```

## Appendix B: Example Script

```lua
-- scripts/spin.luau
-- Spins the entity around the Y axis

local rotation_speed: number = 90.0 -- degrees per second

function on_update(entity: Entity, world: ScriptWorld, dt: number)
    local transform = world:get_transform(entity)
    if transform then
        local axis = Vec3.new(0.0, 1.0, 0.0)
        local angle_rad = math.rad(rotation_speed * dt)
        local delta_rot = Quat.from_axis_angle(axis, angle_rad)
        transform.rotation = delta_rot * transform.rotation
        world:set_transform(entity, transform)
    end
end
```

## Appendix C: Dependency Compliance Check

```
katla_script dependencies:
  ✅ katla_ecs    (allowed: katla_ecs has no restrictions on who depends on it)
  ✅ katla_math   (allowed: katla_math has no restrictions on who depends on it)
  ✅ mlua         (external crate, no restriction)
  ✅ log          (external crate)
  ✅ katla_derive (for Component derive macro)

katla_script does NOT depend on:
  ✅ katla_gfx    (not needed)
  ✅ katla_app    (not needed; katla_app depends on katla_script, not vice versa)
  ✅ katla_ui     (not needed)

Note on TransformComponent access:
  katla_script can only read/write components that are defined in crates it depends on.
  Transform is defined in katla_math (accessible).
  But TransformComponent is defined in katla_app (NOT accessible from katla_script).

  Solution: katla_script operates on raw katla_math types (Transform, Vec3).
  The ScriptSystem in katla_app bridges between TransformComponent and the raw Transform.
  katla_script never touches TransformComponent directly -- it only knows about Transform.
```

This means the actual component read/write happens in `katla_app` code, not in `katla_script`. The `ScriptWorld` proxy type in `katla_script` is generic/abstract:

```rust
// In katla_script: Abstract trait for world access
pub trait ScriptWorldAccess {
    fn get_transform(&self, entity: EntityId) -> Option<Transform>;
    fn set_transform(&mut self, entity: EntityId, transform: Transform);
    fn entity_exists(&self, entity: EntityId) -> bool;
    fn spawn_entity(&mut self) -> EntityId;
    fn destroy_entity(&mut self, entity: EntityId);
}
```

Then `katla_app` provides the concrete implementation that knows about `TransformComponent`, etc. This keeps the dependency graph clean.
