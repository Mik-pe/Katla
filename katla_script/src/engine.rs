use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use katla_ecs::EntityId;
use katla_math::{Color, Quat, Vec3};
use mlua::{Lua, LuaOptions, RegistryKey, StdLib, VmState};

use crate::bindings::entity::LuaEntityId;
use crate::bindings::math::{LuaColor, LuaQuat, LuaVec3};
use crate::bindings::script_world::ScriptWorldProxy;
use crate::bindings::world::ScriptCommand;
use crate::component::ScriptInstanceHandle;
use crate::error::ScriptError;

/// A scalar value from a script environment, for inspector display/editing.
/// Used to expose script variables to the editor UI.
#[derive(Debug, Clone)]
pub enum ScriptVarValue {
    /// A numeric value from the script.
    Number(f64),
    /// A boolean value from the script.
    Boolean(bool),
    /// A string value from the script.
    String(String),
}

/// Maximum number of Lua bytecode instructions allowed per script execution.
/// Prevents infinite loops from hanging the engine.
const INSTRUCTION_LIMIT: u64 = 10_000_000;

/// The script engine manages Lua script loading, execution, and instance lifecycle.
///
/// # Thread Safety
///
/// **Warning:** `ScriptEngine` is NOT thread-safe (`!Send + !Sync`).
/// The underlying `mlua::Lua` VM is single-threaded.
/// If you need to use scripts from multiple threads, create a separate `ScriptEngine`
/// per thread or use external synchronization.
///
/// # Usage
///
/// ```ignore
/// let mut engine = ScriptEngine::new()?;
/// engine.set_scripts_dir("resources/scripts");
/// engine.load_script("player")?;
/// let handle = engine.create_instance(entity_id, "player")?;
/// ```
pub struct ScriptEngine {
    pub(crate) vm: Lua,
    pub(crate) loaded_scripts: HashMap<String, RegistryKey>,
    pub(crate) instances: Vec<Option<ScriptInstance>>,
    pub(crate) generations: Vec<u32>,
    pub(crate) free_list: Vec<u32>,
    /// Base directory for script resolution (e.g. "resources/scripts").
    /// When set, bare script names are resolved relative to this directory.
    scripts_dir: Option<String>,
    instruction_count: Rc<Cell<u64>>,
}

/// Internal representation of a script instance attached to an entity.
///
/// Each entity with a `ScriptComponent` has one associated instance.
pub(crate) struct ScriptInstance {
    /// Path to the script file (relative to scripts_dir).
    pub script_path: String,
    /// The entity this script is attached to.
    pub entity: EntityId,
    /// Registry key for the Lua environment table.
    pub _env_key: RegistryKey,
    /// Extracted hook functions from the script.
    pub hooks: ScriptHooks,
    /// Generation counter for handle validation.
    pub generation: u32,
    /// Number of consecutive errors from this instance.
    pub(crate) error_count: u32,
}

/// Extracted hook function references from a script.
///
/// Scripts can define these optional functions:
/// - `on_update(entity, world, dt)` - Called every frame
/// - `on_spawn(entity, world)` - Called when entity is spawned
/// - `on_destroy(entity)` - Called when entity is destroyed
pub(crate) struct ScriptHooks {
    /// The on_update hook function, if defined in the script.
    pub on_update: Option<RegistryKey>,
    /// The on_spawn hook function, if defined in the script.
    pub on_spawn: Option<RegistryKey>,
    /// The on_destroy hook function, if defined in the script.
    pub on_destroy: Option<RegistryKey>,
}

impl Drop for ScriptEngine {
    fn drop(&mut self) {
        // Clean up all loaded script registry values to prevent memory leaks
        // We need to take ownership of the keys to pass them to remove_registry_value
        // Since we're in Drop, we can safely consume the HashMap
        let loaded_scripts = std::mem::take(&mut self.loaded_scripts);
        for (_, key) in loaded_scripts {
            let _ = self.vm.remove_registry_value(key);
        }

        // Clean up instance environment and hook registry values
        // Take ownership of instances to consume them
        let instances = std::mem::take(&mut self.instances);
        for inst in instances.into_iter().flatten() {
            let _ = self.vm.remove_registry_value(inst._env_key);
            if let Some(key) = inst.hooks.on_update {
                let _ = self.vm.remove_registry_value(key);
            }
            if let Some(key) = inst.hooks.on_spawn {
                let _ = self.vm.remove_registry_value(key);
            }
            if let Some(key) = inst.hooks.on_destroy {
                let _ = self.vm.remove_registry_value(key);
            }
        }
    }
}

impl ScriptEngine {
    pub fn new() -> Result<Self, ScriptError> {
        let vm = Lua::new_with(StdLib::ALL_SAFE, LuaOptions::default()).map_err(|e| {
            ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            }
        })?;

        crate::bindings::math::register_math_types(&vm).map_err(|e| ScriptError::LoadFailed {
            path: "<vm>".into(),
            source: e,
        })?;
        crate::bindings::entity::register_entity_type(&vm).map_err(|e| {
            ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            }
        })?;
        crate::bindings::script_world::register_script_world_type(&vm).map_err(|e| {
            ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            }
        })?;

        let globals = vm.globals();

        let vec3_table = vm.create_table().map_err(|e| ScriptError::LoadFailed {
            path: "<vm>".into(),
            source: e,
        })?;
        vec3_table
            .set(
                "new",
                vm.create_function(|_, (x, y, z): (f32, f32, f32)| Ok(LuaVec3(Vec3::new(x, y, z))))
                    .map_err(|e| ScriptError::LoadFailed {
                        path: "<vm>".into(),
                        source: e,
                    })?,
            )
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;
        globals
            .set("Vec3", vec3_table)
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;

        let quat_table = vm.create_table().map_err(|e| ScriptError::LoadFailed {
            path: "<vm>".into(),
            source: e,
        })?;
        quat_table
            .set(
                "new",
                vm.create_function(|_, (x, y, z, w): (f32, f32, f32, f32)| {
                    Ok(LuaQuat(Quat::new(x, y, z, w)))
                })
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?,
            )
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;
        quat_table
            .set(
                "identity",
                vm.create_function(|_, ()| Ok(LuaQuat(Quat::identity())))
                    .map_err(|e| ScriptError::LoadFailed {
                        path: "<vm>".into(),
                        source: e,
                    })?,
            )
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;
        quat_table
            .set(
                "from_axis_angle",
                vm.create_function(|_, (axis, angle): (LuaVec3, f32)| {
                    Ok(LuaQuat(Quat::from_axis_angle(axis.0, angle)))
                })
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?,
            )
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;
        globals
            .set("Quat", quat_table)
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;

        let color_table = vm.create_table().map_err(|e| ScriptError::LoadFailed {
            path: "<vm>".into(),
            source: e,
        })?;
        color_table
            .set(
                "new",
                vm.create_function(|_, (r, g, b, a): (f32, f32, f32, f32)| {
                    Ok(LuaColor(Color::new(r, g, b, a)))
                })
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?,
            )
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;
        color_table
            .set(
                "rgb",
                vm.create_function(|_, (r, g, b): (f32, f32, f32)| {
                    Ok(LuaColor(Color::rgb(r, g, b)))
                })
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?,
            )
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;
        color_table
            .set(
                "from_rgb_hex",
                vm.create_function(|_, hex: u32| Ok(LuaColor(Color::from_rgb_hex(hex))))
                    .map_err(|e| ScriptError::LoadFailed {
                        path: "<vm>".into(),
                        source: e,
                    })?,
            )
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;
        globals
            .set("Color", color_table)
            .map_err(|e| ScriptError::LoadFailed {
                path: "<vm>".into(),
                source: e,
            })?;

        #[cfg(debug_assertions)]
        {
            let print_fn = vm
                .create_function(|lua, args: mlua::MultiValue| {
                    let msg: String = args
                        .into_iter()
                        .map(|v| {
                            lua.coerce_string(v.clone())
                                .ok()
                                .flatten()
                                .map(|s| s.to_string_lossy())
                                .unwrap_or_else(|| format!("{v:?}"))
                        })
                        .collect::<Vec<_>>()
                        .join("\t");
                    log::info!("{msg}");
                    Ok(())
                })
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?;
            globals
                .set("print", print_fn)
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?;

            let warn_fn = vm
                .create_function(|lua, args: mlua::MultiValue| {
                    let msg: String = args
                        .into_iter()
                        .map(|v| {
                            lua.coerce_string(v.clone())
                                .ok()
                                .flatten()
                                .map(|s| s.to_string_lossy())
                                .unwrap_or_else(|| format!("{v:?}"))
                        })
                        .collect::<Vec<_>>()
                        .join("\t");
                    log::warn!("{msg}");
                    Ok(())
                })
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?;
            globals
                .set("warn", warn_fn)
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?;
        }

        #[cfg(not(debug_assertions))]
        {
            let print_fn = vm
                .create_function(|_, _: mlua::MultiValue| Ok(()))
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?;
            globals
                .set("print", print_fn)
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?;

            let warn_fn = vm
                .create_function(|_, _: mlua::MultiValue| Ok(()))
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?;
            globals
                .set("warn", warn_fn)
                .map_err(|e| ScriptError::LoadFailed {
                    path: "<vm>".into(),
                    source: e,
                })?;
        }

        let instruction_count = Rc::new(Cell::new(0u64));
        let count_clone = instruction_count.clone();
        vm.set_interrupt(move |_| {
            let c = count_clone.get();
            if c >= INSTRUCTION_LIMIT {
                return Err(mlua::Error::external(format!(
                    "Script exceeded instruction limit ({INSTRUCTION_LIMIT})"
                )));
            }
            count_clone.set(c + 1);
            Ok(VmState::Continue)
        });

        Ok(Self {
            vm,
            loaded_scripts: HashMap::new(),
            instances: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            scripts_dir: None,
            instruction_count,
        })
    }

    /// Set the base directory for resolving bare script names.
    pub fn set_scripts_dir(&mut self, dir: impl Into<String>) {
        self.scripts_dir = Some(dir.into());
    }

    /// Get the current scripts directory.
    pub fn scripts_dir(&self) -> Option<&str> {
        self.scripts_dir.as_deref()
    }

    /// Resolve a script path to a full filesystem path.
    /// Validates that the resolved path is within the scripts directory.
    fn resolve_script_path(&self, path: &str) -> Result<std::path::PathBuf, ScriptError> {
        use std::path::Path;

        let input_path = Path::new(path);

        // If it's already an absolute path that exists, check it's in scripts_dir
        if input_path.is_absolute() {
            if input_path.exists() {
                if let Some(dir) = &self.scripts_dir {
                    let dir_path = Path::new(dir);
                    if !input_path.starts_with(dir_path) {
                        return Err(ScriptError::PathOutsideScriptsDir {
                            path: path.to_string(),
                            scripts_dir: dir.clone(),
                        });
                    }
                }
                return Ok(input_path.to_path_buf());
            }
            // Absolute path doesn't exist, will fail later
            return Ok(input_path.to_path_buf());
        }

        // Relative path: resolve relative to scripts_dir or default
        let full_path = if let Some(dir) = &self.scripts_dir {
            Path::new(dir).join(path).with_extension("luau")
        } else {
            Path::new("resources/scripts")
                .join(path)
                .with_extension("luau")
        };

        Ok(full_path)
    }

    pub fn reset_instruction_counter(&self) {
        self.instruction_count.set(0);
    }

    pub fn load_script(&mut self, path: &str) -> Result<(), ScriptError> {
        if self.loaded_scripts.contains_key(path) {
            return Ok(());
        }

        let resolved_path = self.resolve_script_path(path)?;

        let source =
            std::fs::read_to_string(&resolved_path).map_err(|e| ScriptError::LoadFailed {
                path: resolved_path.display().to_string(),
                source: mlua::Error::external(e),
            })?;

        let func = self
            .vm
            .load(&source)
            .set_name(path)
            .into_function()
            .map_err(|e| ScriptError::LoadFailed {
                path: path.into(),
                source: e,
            })?;

        let key = self
            .vm
            .create_registry_value(func)
            .map_err(|e| ScriptError::LoadFailed {
                path: path.into(),
                source: e,
            })?;

        self.loaded_scripts.insert(path.to_string(), key);
        Ok(())
    }

    pub fn create_instance(
        &mut self,
        entity: EntityId,
        script_path: &str,
    ) -> Result<ScriptInstanceHandle, ScriptError> {
        self.load_script(script_path)?;

        let script_key =
            self.loaded_scripts
                .get(script_path)
                .ok_or(ScriptError::ScriptNotLoaded {
                    path: script_path.into(),
                })?;
        let script_func: mlua::Function =
            self.vm
                .registry_value(script_key)
                .map_err(|e| ScriptError::LoadFailed {
                    path: script_path.into(),
                    source: e,
                })?;

        let env = self
            .vm
            .create_table()
            .map_err(|e| ScriptError::LoadFailed {
                path: script_path.into(),
                source: e,
            })?;
        let globals = self.vm.globals();
        let metatable = self
            .vm
            .create_table()
            .map_err(|e| ScriptError::LoadFailed {
                path: script_path.into(),
                source: e,
            })?;
        metatable
            .set("__index", globals)
            .map_err(|e| ScriptError::LoadFailed {
                path: script_path.into(),
                source: e,
            })?;
        let _ = env.set_metatable(Some(metatable));

        script_func
            .set_environment(env.clone())
            .map_err(|e| ScriptError::LoadFailed {
                path: script_path.into(),
                source: e,
            })?;

        self.reset_instruction_counter();
        script_func
            .call::<()>(())
            .map_err(|e| ScriptError::ExecutionFailed {
                path: script_path.into(),
                function: "<top-level>".into(),
                source: e,
            })?;

        let hooks = ScriptHooks {
            on_update: self.extract_hook(&env, "on_update", script_path)?,
            on_spawn: self.extract_hook(&env, "on_spawn", script_path)?,
            on_destroy: self.extract_hook(&env, "on_destroy", script_path)?,
        };

        let env_key = self
            .vm
            .create_registry_value(env)
            .map_err(|e| ScriptError::LoadFailed {
                path: script_path.into(),
                source: e,
            })?;

        let (handle, slot) = if let Some(idx) = self.free_list.pop() {
            let generation = self.generations[idx as usize];
            (
                ScriptInstanceHandle {
                    index: idx,
                    generation,
                },
                idx as usize,
            )
        } else {
            let idx = self.instances.len() as u32;
            self.generations.push(0);
            (
                ScriptInstanceHandle {
                    index: idx,
                    generation: 0,
                },
                idx as usize,
            )
        };

        let instance = ScriptInstance {
            script_path: script_path.to_string(),
            entity,
            _env_key: env_key,
            hooks,
            generation: handle.generation,
            error_count: 0,
        };

        if slot < self.instances.len() {
            self.instances[slot] = Some(instance);
        } else {
            self.instances.push(Some(instance));
        }

        Ok(handle)
    }

    fn extract_hook(
        &self,
        env: &mlua::Table,
        name: &str,
        path: &str,
    ) -> Result<Option<RegistryKey>, ScriptError> {
        let value: mlua::Value = env.get(name).map_err(|e| ScriptError::LoadFailed {
            path: path.into(),
            source: e,
        })?;
        match value {
            mlua::Value::Function(func) => {
                let key =
                    self.vm
                        .create_registry_value(func)
                        .map_err(|e| ScriptError::LoadFailed {
                            path: path.into(),
                            source: e,
                        })?;
                Ok(Some(key))
            }
            mlua::Value::Nil => Ok(None),
            _ => Err(ScriptError::InvalidHook {
                path: path.to_string(),
                hook: name.to_string(),
            }),
        }
    }

    pub fn execute_on_update(
        &mut self,
        handle: ScriptInstanceHandle,
        entity: EntityId,
        proxy: ScriptWorldProxy,
        dt: f32,
    ) -> Result<Vec<ScriptCommand>, ScriptError> {
        let (script_path, hook_key) = {
            let instance = self
                .instances
                .get(handle.index as usize)
                .and_then(|opt| opt.as_ref())
                .filter(|inst| inst.generation == handle.generation)
                .ok_or(ScriptError::InstanceNotFound(handle))?;
            match &instance.hooks.on_update {
                Some(key) => (instance.script_path.clone(), key),
                None => return Ok(Vec::new()),
            }
        };

        let func: mlua::Function =
            self.vm
                .registry_value(hook_key)
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: script_path.clone(),
                    function: "on_update".into(),
                    source: e,
                })?;

        let ud = self
            .vm
            .create_userdata(proxy)
            .map_err(|e| ScriptError::ExecutionFailed {
                path: script_path.clone(),
                function: "on_update".into(),
                source: e,
            })?;

        self.reset_instruction_counter();
        func.call::<()>((LuaEntityId(entity), ud.clone(), dt))
            .map_err(|e| {
                if let Some(Some(inst)) = self.instances.get_mut(handle.index as usize)
                    && inst.generation == handle.generation
                {
                    inst.error_count += 1;
                }
                ScriptError::ExecutionFailed {
                    path: script_path.clone(),
                    function: "on_update".into(),
                    source: e,
                }
            })?;

        let borrowed =
            ud.borrow::<ScriptWorldProxy>()
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: script_path,
                    function: "on_update".into(),
                    source: e,
                })?;

        Ok(borrowed.commands.clone())
    }

    pub fn execute_on_spawn(
        &mut self,
        handle: ScriptInstanceHandle,
        entity: EntityId,
        proxy: ScriptWorldProxy,
    ) -> Result<Vec<ScriptCommand>, ScriptError> {
        let (script_path, hook_key) = {
            let instance = self
                .instances
                .get(handle.index as usize)
                .and_then(|opt| opt.as_ref())
                .filter(|inst| inst.generation == handle.generation)
                .ok_or(ScriptError::InstanceNotFound(handle))?;
            match &instance.hooks.on_spawn {
                Some(key) => (instance.script_path.clone(), key),
                None => return Ok(Vec::new()),
            }
        };

        let func: mlua::Function =
            self.vm
                .registry_value(hook_key)
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: script_path.clone(),
                    function: "on_spawn".into(),
                    source: e,
                })?;

        let ud = self
            .vm
            .create_userdata(proxy)
            .map_err(|e| ScriptError::ExecutionFailed {
                path: script_path.clone(),
                function: "on_spawn".into(),
                source: e,
            })?;

        self.reset_instruction_counter();
        func.call::<()>((LuaEntityId(entity), ud.clone()))
            .map_err(|e| ScriptError::ExecutionFailed {
                path: script_path.clone(),
                function: "on_spawn".into(),
                source: e,
            })?;

        let borrowed =
            ud.borrow::<ScriptWorldProxy>()
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: script_path,
                    function: "on_spawn".into(),
                    source: e,
                })?;

        Ok(borrowed.commands.clone())
    }

    pub fn call_on_destroy(
        &mut self,
        handle: ScriptInstanceHandle,
        entity: EntityId,
    ) -> Result<(), ScriptError> {
        let (script_path, hook_key) = {
            let instance = self
                .instances
                .get(handle.index as usize)
                .and_then(|opt| opt.as_ref())
                .filter(|inst| inst.generation == handle.generation)
                .ok_or(ScriptError::InstanceNotFound(handle))?;
            match &instance.hooks.on_destroy {
                Some(key) => (instance.script_path.clone(), key),
                None => return Ok(()),
            }
        };

        let func: mlua::Function =
            self.vm
                .registry_value(hook_key)
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: script_path.clone(),
                    function: "on_destroy".into(),
                    source: e,
                })?;

        self.reset_instruction_counter();
        func.call::<()>(LuaEntityId(entity))
            .map_err(|e| ScriptError::ExecutionFailed {
                path: script_path,
                function: "on_destroy".into(),
                source: e,
            })?;

        Ok(())
    }

    pub fn remove_instance(&mut self, handle: ScriptInstanceHandle) {
        let idx = handle.index as usize;
        if let Some(inst) = self.instances.get(idx).and_then(|opt| opt.as_ref())
            && inst.generation == handle.generation
        {
            self.generations[idx] += 1;
            self.instances[idx] = None;
            self.free_list.push(handle.index);
        }
    }

    /// Recompile a script from disk and replace the cached chunk.
    /// Returns true if the script was successfully reloaded.
    pub fn reload_script(&mut self, script_path: &str) -> Result<(), ScriptError> {
        let source = self.read_script_source(script_path)?;

        let func = self
            .vm
            .load(&source)
            .set_name(script_path)
            .into_function()
            .map_err(|e| ScriptError::LoadFailed {
                path: script_path.into(),
                source: e,
            })?;

        let new_key = self
            .vm
            .create_registry_value(func)
            .map_err(|e| ScriptError::LoadFailed {
                path: script_path.into(),
                source: e,
            })?;

        if let Some(old_key) = self.loaded_scripts.insert(script_path.to_string(), new_key) {
            let _ = self.vm.remove_registry_value(old_key);
        }

        Ok(())
    }

    /// Hot-reload all instances of a given script.
    /// Re-creates their environments, preserves scalar state (numbers, bools, strings)
    /// from the old environment, and re-extracts hooks.
    /// Returns the NEW handles for all successfully reloaded instances.
    pub fn hot_reload_instances(&mut self, script_path: &str) -> Vec<ScriptInstanceHandle> {
        let old_handles: Vec<ScriptInstanceHandle> = self
            .instances
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| {
                opt.as_ref()
                    .filter(|inst| inst.script_path == script_path)
                    .map(|inst| ScriptInstanceHandle {
                        index: i as u32,
                        generation: inst.generation,
                    })
            })
            .collect();

        let mut new_handles = Vec::new();
        for handle in &old_handles {
            match self.hot_reload_single_instance(*handle, script_path) {
                Ok(new_handle) => new_handles.push(new_handle),
                Err(e) => log::error!("Hot reload failed for '{script_path}': {e}"),
            }
        }

        new_handles
    }

    fn read_script_source(&self, script_path: &str) -> Result<String, ScriptError> {
        let resolved_path = self.resolve_script_path(script_path)?;
        std::fs::read_to_string(&resolved_path).map_err(|e| ScriptError::LoadFailed {
            path: resolved_path.display().to_string(),
            source: mlua::Error::external(e),
        })
    }

    fn hot_reload_single_instance(
        &mut self,
        handle: ScriptInstanceHandle,
        script_path: &str,
    ) -> Result<ScriptInstanceHandle, ScriptError> {
        let entity = {
            let instance = self
                .instances
                .get(handle.index as usize)
                .and_then(|opt| opt.as_ref())
                .filter(|inst| inst.generation == handle.generation)
                .ok_or(ScriptError::InstanceNotFound(handle))?;
            instance.entity
        };

        // Gather scalar state from old environment
        let preserved_state = self.gather_scalar_state(handle)?;
        let preserved_count = preserved_state.len();

        // Remove old instance (frees the slot but bumps generation)
        self.remove_instance(handle);

        // Create a fresh instance
        let new_handle = self.create_instance(entity, script_path)?;

        // Restore preserved scalar state into new environment
        self.restore_scalar_state(new_handle, preserved_state)?;

        log::info!(
            "Hot reloaded script '{script_path}' for entity {entity} ({preserved_count} vars preserved)"
        );

        Ok(new_handle)
    }

    pub(crate) fn gather_scalar_state(
        &self,
        handle: ScriptInstanceHandle,
    ) -> Result<Vec<(String, mlua::Value)>, ScriptError> {
        let instance = self
            .instances
            .get(handle.index as usize)
            .and_then(|opt| opt.as_ref())
            .filter(|inst| inst.generation == handle.generation)
            .ok_or(ScriptError::InstanceNotFound(handle))?;

        let env: mlua::Table = self.vm.registry_value(&instance._env_key).map_err(|e| {
            ScriptError::ExecutionFailed {
                path: instance.script_path.clone(),
                function: "<internal>".into(),
                source: e,
            }
        })?;

        let mut preserved = Vec::new();

        // Use Lua's next() to iterate raw table entries, bypassing metatable __pairs.
        let next_fn: mlua::Function =
            self.vm
                .globals()
                .raw_get("next")
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: instance.script_path.clone(),
                    function: "<internal>".into(),
                    source: e,
                })?;

        let mut key = mlua::Value::Nil;
        loop {
            let result: (mlua::Value, mlua::Value) =
                next_fn
                    .call((env.clone(), key))
                    .map_err(|e| ScriptError::ExecutionFailed {
                        path: instance.script_path.clone(),
                        function: "<internal>".into(),
                        source: e,
                    })?;

            let (next_key, value) = result;
            if matches!(next_key, mlua::Value::Nil) {
                break;
            }

            let key_str = match &next_key {
                mlua::Value::String(s) => s.to_str().ok().map(|s| s.to_string()),
                mlua::Value::Number(n) => Some(format!("{n}")),
                _ => {
                    key = next_key;
                    continue;
                }
            };
            let Some(key_str) = key_str else {
                key = next_key;
                continue;
            };
            match &value {
                mlua::Value::Number(_)
                | mlua::Value::String(_)
                | mlua::Value::Integer(_)
                | mlua::Value::Boolean(_)
                | mlua::Value::Nil => {
                    preserved.push((key_str, value));
                }
                _ => {}
            }
            key = next_key;
        }

        Ok(preserved)
    }

    fn restore_scalar_state(
        &self,
        handle: ScriptInstanceHandle,
        state: Vec<(String, mlua::Value)>,
    ) -> Result<(), ScriptError> {
        let instance = self
            .instances
            .get(handle.index as usize)
            .and_then(|opt| opt.as_ref())
            .filter(|inst| inst.generation == handle.generation)
            .ok_or(ScriptError::InstanceNotFound(handle))?;

        let env: mlua::Table = self.vm.registry_value(&instance._env_key).map_err(|e| {
            ScriptError::ExecutionFailed {
                path: instance.script_path.clone(),
                function: "<internal>".into(),
                source: e,
            }
        })?;

        for (key, value) in state {
            env.raw_set(key, value)
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: instance.script_path.clone(),
                    function: "<internal>".into(),
                    source: e,
                })?;
        }

        Ok(())
    }

    /// Inspect the scalar variables of a script instance's environment.
    /// Returns (variable_name, value) pairs for numbers, booleans, and strings.
    /// Skips functions and tables.
    pub fn inspect_instance_vars(
        &self,
        handle: ScriptInstanceHandle,
    ) -> Result<Vec<(String, ScriptVarValue)>, ScriptError> {
        let raw = self.gather_scalar_state(handle)?;
        let mut vars = Vec::with_capacity(raw.len());
        for (name, value) in raw {
            let var = match &value {
                mlua::Value::Number(n) => ScriptVarValue::Number(*n),
                mlua::Value::Integer(i) => ScriptVarValue::Number(*i as f64),
                mlua::Value::Boolean(b) => ScriptVarValue::Boolean(*b),
                mlua::Value::String(s) => {
                    ScriptVarValue::String(s.to_str().map(|s| s.to_string()).unwrap_or_default())
                }
                _ => continue,
            };
            vars.push((name, var));
        }
        vars.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(vars)
    }

    /// Set a scalar variable on a script instance's environment.
    pub fn set_instance_var(
        &self,
        handle: ScriptInstanceHandle,
        name: &str,
        value: ScriptVarValue,
    ) -> Result<(), ScriptError> {
        let instance = self
            .instances
            .get(handle.index as usize)
            .and_then(|opt| opt.as_ref())
            .filter(|inst| inst.generation == handle.generation)
            .ok_or(ScriptError::InstanceNotFound(handle))?;

        let env: mlua::Table = self.vm.registry_value(&instance._env_key).map_err(|e| {
            ScriptError::ExecutionFailed {
                path: instance.script_path.clone(),
                function: "<internal>".into(),
                source: e,
            }
        })?;

        let lua_val = match value {
            ScriptVarValue::Number(n) => mlua::Value::Number(n),
            ScriptVarValue::Boolean(b) => mlua::Value::Boolean(b),
            ScriptVarValue::String(s) => self
                .vm
                .create_string(&s)
                .map(mlua::Value::String)
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: instance.script_path.clone(),
                    function: "<internal>".into(),
                    source: e,
                })?,
        };

        env.raw_set(name, lua_val)
            .map_err(|e| ScriptError::ExecutionFailed {
                path: instance.script_path.clone(),
                function: "<internal>".into(),
                source: e,
            })?;

        Ok(())
    }

    /// Get the script path for a given instance handle.
    pub fn instance_script_path(&self, handle: ScriptInstanceHandle) -> Option<String> {
        self.instances
            .get(handle.index as usize)
            .and_then(|opt| opt.as_ref())
            .filter(|inst| inst.generation == handle.generation)
            .map(|inst| inst.script_path.clone())
    }

    /// Get the handle for a script instance attached to an entity.
    pub fn instance_for_entity(&self, entity: EntityId) -> Option<ScriptInstanceHandle> {
        self.instances.iter().enumerate().find_map(|(i, opt)| {
            opt.as_ref()
                .filter(|inst| inst.entity == entity)
                .map(|inst| ScriptInstanceHandle {
                    index: i as u32,
                    generation: inst.generation,
                })
        })
    }
}
