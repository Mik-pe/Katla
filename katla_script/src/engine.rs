use std::cell::Cell;
use std::collections::HashMap;
use std::path::Path;
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

const INSTRUCTION_LIMIT: u64 = 10_000_000;

pub struct ScriptEngine {
    pub(crate) vm: Lua,
    pub(crate) loaded_scripts: HashMap<String, RegistryKey>,
    pub(crate) instances: Vec<Option<ScriptInstance>>,
    pub(crate) generations: Vec<u32>,
    pub(crate) free_list: Vec<u32>,
    /// Base directory for script resolution (e.g. "resources/scripts").
    /// When set, bare script names are resolved relative to this directory.
    pub(crate) scripts_dir: Option<String>,
    instruction_count: Rc<Cell<u64>>,
}

pub(crate) struct ScriptInstance {
    pub script_path: String,
    pub entity: EntityId,
    pub env_key: RegistryKey,
    pub hooks: ScriptHooks,
    pub generation: u32,
    pub(crate) error_count: u32,
}

pub(crate) struct ScriptHooks {
    pub on_update: Option<RegistryKey>,
    pub on_spawn: Option<RegistryKey>,
    pub on_destroy: Option<RegistryKey>,
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

    pub fn reset_instruction_counter(&self) {
        self.instruction_count.set(0);
    }

    pub fn load_script(&mut self, path: &str) -> Result<(), ScriptError> {
        if self.loaded_scripts.contains_key(path) {
            return Ok(());
        }

        let source = if Path::new(path).exists() {
            std::fs::read_to_string(path).map_err(|e| ScriptError::LoadFailed {
                path: path.into(),
                source: mlua::Error::external(e),
            })?
        } else {
            let full_path = match &self.scripts_dir {
                Some(dir) => format!("{dir}/{path}.luau"),
                None => format!("resources/scripts/{path}.luau"),
            };
            std::fs::read_to_string(&full_path).map_err(|e| ScriptError::LoadFailed {
                path: full_path.clone(),
                source: mlua::Error::external(e),
            })?
        };

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
        script_func.call::<()>(()).map_err(|e| {
            log::error!("Script top-level execution failed for '{script_path}': {e}");
            ScriptError::ExecutionFailed {
                path: script_path.into(),
                line: extract_line_number(&e),
                source: e,
            }
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
            env_key,
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
                    line: None,
                    source: e,
                })?;

        let ud = self
            .vm
            .create_userdata(proxy)
            .map_err(|e| ScriptError::ExecutionFailed {
                path: script_path.clone(),
                line: None,
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
                log::error!("Script on_update failed for '{script_path}': {e}");
                ScriptError::ExecutionFailed {
                    path: script_path.clone(),
                    line: extract_line_number(&e),
                    source: e,
                }
            })?;

        let borrowed =
            ud.borrow::<ScriptWorldProxy>()
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: script_path,
                    line: None,
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
                    line: None,
                    source: e,
                })?;

        let ud = self
            .vm
            .create_userdata(proxy)
            .map_err(|e| ScriptError::ExecutionFailed {
                path: script_path.clone(),
                line: None,
                source: e,
            })?;

        self.reset_instruction_counter();
        func.call::<()>((LuaEntityId(entity), ud.clone()))
            .map_err(|e| {
                log::error!("Script on_spawn failed for '{script_path}': {e}");
                ScriptError::ExecutionFailed {
                    path: script_path.clone(),
                    line: extract_line_number(&e),
                    source: e,
                }
            })?;

        let borrowed =
            ud.borrow::<ScriptWorldProxy>()
                .map_err(|e| ScriptError::ExecutionFailed {
                    path: script_path,
                    line: None,
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
                    line: None,
                    source: e,
                })?;

        self.reset_instruction_counter();
        func.call::<()>(LuaEntityId(entity)).map_err(|e| {
            log::error!("Script on_destroy failed for '{script_path}': {e}");
            ScriptError::ExecutionFailed {
                path: script_path,
                line: extract_line_number(&e),
                source: e,
            }
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
}

fn extract_line_number(error: &mlua::Error) -> Option<usize> {
    let msg = error.to_string();
    let line_prefixes = [":", "line "];
    for prefix in &line_prefixes {
        if let Some(pos) = msg.rfind(prefix) {
            let rest = &msg[pos + prefix.len()..];
            let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(line) = num_str.parse::<usize>() {
                return Some(line);
            }
        }
    }
    None
}
