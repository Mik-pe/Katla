//! Sandboxed Luau instances produce owned deferred commands without borrowing Odin state.
use mlua::{
    AnyUserData, Function, Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods,
    Value as LuaValue, Variadic, VmState,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    time::Instant,
};

#[derive(Clone, Copy, Debug)]
struct Entity(u64);
impl UserData for Entity {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Eq, |_, entity, other: AnyUserData| {
            Ok(other
                .borrow::<Entity>()
                .is_ok_and(|other| other.0 == entity.0))
        });
        methods.add_meta_method(MetaMethod::ToString, |_, entity, ()| {
            Ok(entity.0.to_string())
        });
        methods.add_method("id", |_, entity, ()| Ok(entity.0.to_string()));
    }
}
#[derive(Clone, Copy)]
struct Vector3([f32; 3]);
impl UserData for Vector3 {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("x", |_, value| Ok(value.0[0]));
        fields.add_field_method_get("y", |_, value| Ok(value.0[1]));
        fields.add_field_method_get("z", |_, value| Ok(value.0[2]));
    }
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Add, |_, value, other: AnyUserData| {
            let other = other.borrow::<Vector3>()?;
            Ok(Vector3(std::array::from_fn(|i| value.0[i] + other.0[i])))
        });
        methods.add_meta_method(MetaMethod::Sub, |_, value, other: AnyUserData| {
            let other = other.borrow::<Vector3>()?;
            Ok(Vector3(std::array::from_fn(|i| value.0[i] - other.0[i])))
        });
        methods.add_meta_method(MetaMethod::Mul, |_, value, factor: f32| {
            Ok(Vector3(value.0.map(|v| v * factor)))
        });
        methods.add_meta_method(MetaMethod::Div, |_, value, factor: f32| {
            if factor == 0.0 {
                return Err(mlua::Error::external("Division by zero"));
            }
            Ok(Vector3(value.0.map(|v| v / factor)))
        });
        methods.add_method("length", |_, value, ()| {
            Ok(value.0.iter().map(|v| v * v).sum::<f32>().sqrt())
        });
        methods.add_method("normalized", |_, value, ()| {
            let len = value.0.iter().map(|v| v * v).sum::<f32>().sqrt();
            Ok(Vector3(if len > 0.0 {
                value.0.map(|v| v / len)
            } else {
                [0.0; 3]
            }))
        });
    }
}
fn vector_from_lua(value: LuaValue) -> mlua::Result<[f32; 3]> {
    match value {
        LuaValue::UserData(userdata) => Ok(userdata.borrow::<Vector3>()?.0),
        LuaValue::Table(table) => Ok([
            table.get("x").or_else(|_| table.get(1))?,
            table.get("y").or_else(|_| table.get(2))?,
            table.get("z").or_else(|_| table.get(3))?,
        ]),
        _ => Err(mlua::Error::external(
            "Expected Vec3 or three-component table",
        )),
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneEntity {
    id: String,
    name: String,
    position: [f32; 3],
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Command {
    SetPosition {
        entity_id: String,
        position: [f32; 3],
    },
    BurstParticles {
        entity_id: String,
        count: u32,
    },
    SetParticlesActive {
        entity_id: String,
        active: bool,
    },
    Emit {
        name: String,
        trigger: String,
        other: String,
    },
}
#[derive(Clone, Serialize)]
struct QueuedCommand {
    owner_id: String,
    #[serde(flatten)]
    command: Command,
}
#[derive(Clone)]
struct Subscription {
    name: String,
    callback: Function,
}
#[derive(Clone)]
struct Proxy {
    owner: u64,
    entities: Rc<BTreeMap<u64, SceneEntity>>,
    commands: Rc<RefCell<Vec<QueuedCommand>>>,
    subscriptions: Rc<RefCell<Vec<Subscription>>>,
}
impl Proxy {
    fn command(&self, command: Command) -> mlua::Result<()> {
        let mut queue = self.commands.borrow_mut();
        if queue.len() >= 4096 {
            return Err(mlua::Error::external(
                "Deferred script command queue is full",
            ));
        };
        queue.push(QueuedCommand {
            owner_id: self.owner.to_string(),
            command,
        });
        Ok(())
    }
}
impl UserData for Proxy {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("find_entity", |lua, proxy, name: String| {
            match proxy
                .entities
                .iter()
                .find(|(_, entity)| entity.name == name)
            {
                Some((id, _)) => Ok(LuaValue::UserData(lua.create_userdata(Entity(*id))?)),
                None => Ok(LuaValue::Nil),
            }
        });
        methods.add_method("get_position", |_, proxy, id: AnyUserData| {
            let id = id.borrow::<Entity>()?.0;
            let entity = proxy
                .entities
                .get(&id)
                .ok_or_else(|| mlua::Error::external("Entity is stale"))?;
            Ok(Vector3(entity.position))
        });
        methods.add_method("get_transform", |lua, proxy, id: AnyUserData| {
            let id = id.borrow::<Entity>()?.0;
            let Some(entity) = proxy.entities.get(&id) else {
                return Ok(None);
            };
            let table = lua.create_table()?;
            table.set("position", Vector3(entity.position))?;
            Ok(Some(table))
        });
        methods.add_method("entity_exists", |_, proxy, id: AnyUserData| {
            Ok(proxy.entities.contains_key(&id.borrow::<Entity>()?.0))
        });
        methods.add_method(
            "set_position",
            |_, proxy, (id, position): (AnyUserData, LuaValue)| {
                let id = id.borrow::<Entity>()?.0;
                if !proxy.entities.contains_key(&id) {
                    return Err(mlua::Error::external("Entity is stale"));
                }
                let position = vector_from_lua(position)?;
                if position.iter().any(|v| !v.is_finite()) {
                    return Err(mlua::Error::external("Position must be finite"));
                };
                proxy.command(Command::SetPosition {
                    entity_id: id.to_string(),
                    position,
                })
            },
        );
        methods.add_method(
            "burst_particles",
            |_, proxy, (id, count): (AnyUserData, u32)| {
                if !(1..=100_000).contains(&count) {
                    return Err(mlua::Error::external("Burst count requires 1..100000"));
                };
                proxy.command(Command::BurstParticles {
                    entity_id: id.borrow::<Entity>()?.0.to_string(),
                    count,
                })
            },
        );
        methods.add_method(
            "set_particles_active",
            |_, proxy, (id, active): (AnyUserData, bool)| {
                proxy.command(Command::SetParticlesActive {
                    entity_id: id.borrow::<Entity>()?.0.to_string(),
                    active,
                })
            },
        );
        methods.add_method("emit", |_, proxy, name: String| {
            if name.trim().is_empty() || name.len() > 128 {
                return Err(mlua::Error::external("Event name requires 1..128 bytes"));
            };
            proxy.command(Command::Emit {
                name,
                trigger: proxy.owner.to_string(),
                other: proxy.owner.to_string(),
            })
        });
        methods.add_method(
            "on_event",
            |_, proxy, (name, callback): (String, Function)| {
                if name.trim().is_empty() || name.len() > 128 {
                    return Err(mlua::Error::external("Event name requires 1..128 bytes"));
                };
                let mut subscriptions = proxy.subscriptions.borrow_mut();
                if subscriptions.len() >= 256 {
                    return Err(mlua::Error::external("Subscription capacity exhausted"));
                };
                subscriptions.push(Subscription { name, callback });
                Ok(())
            },
        );
    }
}
struct Instance {
    environment: Table,
    subscriptions: Rc<RefCell<Vec<Subscription>>>,
    spawned: bool,
    errors: u32,
    disabled: bool,
    path: String,
    source: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    name: String,
    trigger: String,
    other: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attachment {
    entity_id: String,
    source: String,
    path: String,
}
#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    #[serde(rename = "script_validate")]
    Validate { source: String, path: String },
    #[serde(rename = "script_attach")]
    Attach {
        entity_id: String,
        source: String,
        path: String,
    },
    #[serde(rename = "script_detach")]
    Detach { entity_id: String },
    #[serde(rename = "script_reset")]
    Reset,
    #[serde(rename = "script_sync")]
    Sync { scripts: Vec<Attachment> },
    #[serde(rename = "script_tick")]
    Tick {
        delta_seconds: f32,
        entities: Vec<SceneEntity>,
        events: Vec<Event>,
    },
}
struct Budget {
    count: Cell<u64>,
    start: Cell<Option<Instant>>,
}
impl Budget {
    fn begin(&self) {
        self.count.set(0);
        self.start.set(Some(Instant::now()));
    }
}
pub struct Scripts {
    lua: Lua,
    instances: BTreeMap<u64, Instance>,
    budget: Rc<Budget>,
    deferred_events: Vec<Event>,
}
fn parse_id(text: &str) -> Result<u64, String> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Entity ID must be a decimal u64 string".into());
    };
    text.parse().map_err(|_| "Entity ID overflows u64".into())
}
impl Scripts {
    pub fn new() -> Result<Self, String> {
        let lua = Lua::new();
        let globals = lua.globals();
        for name in ["debug", "io", "package", "require", "dofile", "loadfile"] {
            globals
                .set(name, LuaValue::Nil)
                .map_err(|e| e.to_string())?;
        }
        if let Ok(os) = globals.get::<Table>("os") {
            for name in ["execute", "getenv", "remove", "rename", "tmpname", "exit"] {
                os.set(name, LuaValue::Nil).map_err(|e| e.to_string())?;
            }
        }
        let print = lua
            .create_function(|lua, values: Variadic<LuaValue>| {
                let stringify: Function = lua.globals().get("tostring")?;
                let mut output = String::new();
                for value in values {
                    if !output.is_empty() {
                        output.push('\t');
                    }
                    let text: String = stringify.call(value)?;
                    if output.len() + text.len() > 8192 {
                        return Err(mlua::Error::external("Script log line exceeds 8192 bytes"));
                    }
                    output.push_str(&text);
                }
                eprintln!("{output}");
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        globals
            .set("print", print.clone())
            .map_err(|e| e.to_string())?;
        globals.set("warn", print).map_err(|e| e.to_string())?;
        let vector = lua.create_table().map_err(|e| e.to_string())?;
        vector
            .set(
                "new",
                lua.create_function(|_, (x, y, z): (f32, f32, f32)| Ok(Vector3([x, y, z])))
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        vector
            .set("zero", Vector3([0.0; 3]))
            .map_err(|e| e.to_string())?;
        globals.set("Vec3", vector).map_err(|e| e.to_string())?;
        lua.sandbox(true).map_err(|e| e.to_string())?;
        let budget = Rc::new(Budget {
            count: Cell::new(0),
            start: Cell::new(None),
        });
        let checked = budget.clone();
        lua.set_interrupt(move |_| {
            let count = checked.count.get();
            checked.count.set(count + 1);
            if count >= 10_000_000
                || checked
                    .start
                    .get()
                    .is_some_and(|start| start.elapsed().as_secs_f64() >= 5.0)
            {
                return Err(mlua::Error::external("Script execution budget exhausted"));
            };
            Ok(VmState::Continue)
        });
        Ok(Self {
            lua,
            instances: BTreeMap::new(),
            budget,
            deferred_events: Vec::new(),
        })
    }
    fn begin(&self) {
        self.budget.begin();
    }
    fn environment(&self, source: &str, path: &str) -> Result<Table, String> {
        if source.len() > 1024 * 1024 {
            return Err("Script exceeds one MiB".into());
        }
        let environment = self.lua.create_table().map_err(|e| e.to_string())?;
        let metatable = self.lua.create_table().map_err(|e| e.to_string())?;
        metatable
            .set("__index", self.lua.globals())
            .map_err(|e| e.to_string())?;
        environment
            .set_metatable(Some(metatable))
            .map_err(|e| e.to_string())?;
        self.begin();
        self.lua
            .load(source)
            .set_name(path)
            .set_environment(environment.clone())
            .exec()
            .map_err(|e| e.to_string())?;
        for hook in ["on_spawn", "on_update", "on_destroy"] {
            let value: LuaValue = environment.get(hook).map_err(|e| e.to_string())?;
            if !matches!(value, LuaValue::Nil | LuaValue::Function(_)) {
                return Err(format!("{hook} must be a function"));
            }
        }
        Ok(environment)
    }
    fn destroy(&mut self, id: u64) -> Result<(), String> {
        if let Some(instance) = self.instances.remove(&id)
            && instance.spawned
            && !instance.disabled
            && let Some(hook) = instance
                .environment
                .get::<Option<Function>>("on_destroy")
                .map_err(|e| e.to_string())?
        {
            self.begin();
            hook.call::<()>(Entity(id)).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn call(&mut self, value: Value) -> Result<Value, String> {
        match serde_json::from_value::<Request>(value).map_err(|e| e.to_string())? {
            Request::Validate { source, path } => {
                self.environment(&source, &path)?;
                Ok(json!({"valid":true}))
            }
            Request::Attach {
                entity_id,
                source,
                path,
            } => {
                let id = parse_id(&entity_id)?;
                let environment = self.environment(&source, &path)?;
                let destroy_error = self.destroy(id).err();
                self.instances.insert(
                    id,
                    Instance {
                        environment,
                        subscriptions: Rc::new(RefCell::new(Vec::new())),
                        spawned: false,
                        errors: 0,
                        disabled: false,
                        path,
                        source,
                    },
                );
                Ok(json!({"attached":true,"destroy_error":destroy_error}))
            }
            Request::Sync { scripts } => self.sync(scripts),
            Request::Detach { entity_id } => {
                let destroy_error = self.destroy(parse_id(&entity_id)?).err();
                Ok(json!({"detached":true,"destroy_error":destroy_error}))
            }
            Request::Reset => {
                let ids = self.instances.keys().copied().collect::<Vec<_>>();
                let mut diagnostics = Vec::new();
                for id in ids {
                    if let Err(error) = self.destroy(id) {
                        diagnostics.push(json!({"entity_id":id.to_string(),"error":error}));
                    }
                }
                self.deferred_events.clear();
                Ok(json!({"reset":true,"diagnostics":diagnostics}))
            }
            Request::Tick {
                delta_seconds,
                entities,
                events,
            } => self.tick(delta_seconds, entities, events),
        }
    }
    fn sync(&mut self, attachments: Vec<Attachment>) -> Result<Value, String> {
        if attachments.len() > 100000 {
            return Err("Script attachment capacity exhausted".into());
        }
        let mut staged = BTreeMap::new();
        let mut seen = std::collections::BTreeSet::new();
        for attachment in attachments {
            let id = parse_id(&attachment.entity_id)?;
            if !seen.insert(id) {
                return Err("Duplicate script entity".into());
            }
            if self.instances.get(&id).is_some_and(|instance| {
                instance.path == attachment.path && instance.source == attachment.source
            }) {
                continue;
            }
            let environment = self.environment(&attachment.source, &attachment.path)?;
            staged.insert(
                id,
                Instance {
                    environment,
                    subscriptions: Rc::new(RefCell::new(Vec::new())),
                    spawned: false,
                    errors: 0,
                    disabled: false,
                    path: attachment.path,
                    source: attachment.source,
                },
            );
        }
        let mut diagnostics = Vec::new();
        let retired = self
            .instances
            .keys()
            .filter(|id| !seen.contains(id) || staged.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        for id in retired {
            if let Err(error) = self.destroy(id) {
                diagnostics.push(json!({"entity_id":id.to_string(),"error":error}));
            }
        }
        self.instances.extend(staged);
        Ok(json!({"synchronized":true,"diagnostics":diagnostics}))
    }
    fn tick(
        &mut self,
        delta: f32,
        entities: Vec<SceneEntity>,
        events: Vec<Event>,
    ) -> Result<Value, String> {
        if !delta.is_finite() || delta < 0.0 || entities.len() > 100_000 || events.len() > 4096 {
            return Err("Invalid bounded script tick".into());
        }
        for event in &events {
            parse_id(&event.trigger)?;
            parse_id(&event.other)?;
            if event.name.trim().is_empty() || event.name.len() > 128 {
                return Err("Invalid event name".into());
            }
        }
        let mut snapshot = BTreeMap::new();
        for entity in entities {
            let id = parse_id(&entity.id)?;
            if entity.position.iter().any(|v| !v.is_finite())
                || snapshot.insert(id, entity).is_some()
            {
                return Err("Invalid entity snapshot".into());
            }
        }
        let snapshot = Rc::new(snapshot);
        if self.deferred_events.len() + events.len() > 8192 {
            return Err("Event delivery capacity exhausted".into());
        }
        let mut delivery = Vec::new();
        for event in self.deferred_events.iter().chain(events.iter()) {
            let trigger = parse_id(&event.trigger)?;
            let other = parse_id(&event.other)?;
            let data = self.lua.create_table().map_err(|e| e.to_string())?;
            data.set("trigger", Entity(trigger))
                .map_err(|e| e.to_string())?;
            data.set("other", Entity(other))
                .map_err(|e| e.to_string())?;
            data.set("trigger_entity", trigger.to_string())
                .map_err(|e| e.to_string())?;
            data.set("other_entity", other.to_string())
                .map_err(|e| e.to_string())?;
            delivery.push((event.name.clone(), data));
        }
        let stale = self
            .instances
            .keys()
            .filter(|id| !snapshot.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        let mut diagnostics = Vec::new();
        for id in stale {
            if let Err(error) = self.destroy(id) {
                diagnostics.push(json!({"entity_id":id.to_string(),"error":error}));
            }
        }
        self.deferred_events.clear();
        let commands = Rc::new(RefCell::new(Vec::new()));
        let budget = self.budget.clone();
        for (&id, instance) in &mut self.instances {
            if instance.disabled {
                continue;
            };
            let proxy = Proxy {
                owner: id,
                entities: snapshot.clone(),
                commands: commands.clone(),
                subscriptions: instance.subscriptions.clone(),
            };
            let mut failed = false;
            let call_hook = |name: &str, spawn: bool| -> Result<(), String> {
                if let Some(hook) = instance
                    .environment
                    .get::<Option<Function>>(name)
                    .map_err(|e| e.to_string())?
                {
                    budget.begin();
                    let checkpoint = commands.borrow().len();
                    let result = if spawn {
                        hook.call::<()>((Entity(id), proxy.clone()))
                    } else {
                        hook.call::<()>((Entity(id), proxy.clone(), delta))
                    };
                    if result.is_err() {
                        commands.borrow_mut().truncate(checkpoint);
                    }
                    result.map_err(|e| e.to_string())?;
                };
                Ok(())
            };
            if !instance.spawned {
                instance.spawned = true;
                if let Err(error) = call_hook("on_spawn", true) {
                    failed = true;
                    diagnostics.push(
                        json!({"entity_id":id.to_string(),"path":instance.path,"error":error}),
                    );
                }
            }
            if let Err(error) = call_hook("on_update", false) {
                failed = true;
                diagnostics
                    .push(json!({"entity_id":id.to_string(),"path":instance.path,"error":error}));
            }
            let subscriptions = instance.subscriptions.borrow().clone();
            for (name, data) in &delivery {
                for subscription in subscriptions
                    .iter()
                    .filter(|subscription| subscription.name == *name)
                {
                    budget.begin();
                    let checkpoint = commands.borrow().len();
                    if let Err(error) = subscription.callback.call::<()>((
                        name.clone(),
                        data.clone(),
                        proxy.clone(),
                    )) {
                        commands.borrow_mut().truncate(checkpoint);
                        failed = true;
                        diagnostics.push(json!({"entity_id":id.to_string(),"path":instance.path,"error":error.to_string()}));
                    }
                }
            }
            instance.errors = if failed { instance.errors + 1 } else { 0 };
            if instance.errors >= 10 {
                instance.disabled = true;
                instance.subscriptions.borrow_mut().clear();
            }
        }
        let commands = std::mem::take(&mut *commands.borrow_mut());
        for command in &commands {
            if let Command::Emit {
                name,
                trigger,
                other,
            } = &command.command
            {
                self.deferred_events.push(Event {
                    name: name.clone(),
                    trigger: trigger.clone(),
                    other: other.clone(),
                });
            }
        }
        Ok(
            json!({"commands":commands,"diagnostics":diagnostics,"instances":self.instances.iter().map(|(id,instance)|json!({"entity_id":id.to_string(),"spawned":instance.spawned,"disabled":instance.disabled,"consecutive_errors":instance.errors})).collect::<Vec<_>>()}),
        )
    }
}

impl Drop for Scripts {
    fn drop(&mut self) {
        let ids = self.instances.keys().copied().collect::<Vec<_>>();
        for id in ids {
            if let Err(error) = self.destroy(id) {
                eprintln!("Luau on_destroy failed for {id}: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entity(id: &str) -> Value {
        json!({"id":id,"name":"Actor","position":[0,0,0]})
    }
    fn tick(scripts: &mut Scripts, id: &str, events: Value) -> Value {
        scripts.call(json!({"method":"script_tick","delta_seconds":0.25,"entities":[entity(id)],"events":events})).expect("valid tick")
    }
    #[test]
    fn test_spawn_events_and_deferred_lossless_commands() {
        let mut scripts = Scripts::new().expect("Luau");
        let id = u64::MAX.to_string();
        scripts.call(json!({"method":"script_attach","entity_id":id,"path":"scripts/test.luau","source":r#"
            function on_spawn(entity, world)
                world:on_event("activated", function(name,data,current)
                    assert(tostring(data.trigger) == tostring(entity))
                    current:set_particles_active(entity,true)
                    current:burst_particles(entity,32)
                    current:emit("next")
                end)
                world:on_event("next", function(name,data,current) current:burst_particles(entity,7) end)
            end
        "#})).expect("attach");
        let first = tick(
            &mut scripts,
            &id,
            json!([{ "name":"activated","trigger":id,"other":"1" }]),
        );
        assert_eq!(first["commands"].as_array().expect("commands").len(), 3);
        assert_eq!(first["commands"][1]["entity_id"], id);
        assert_eq!(first["commands"][1]["count"], 32);
        let next = tick(&mut scripts, &id, json!([]));
        assert_eq!(next["commands"][0]["count"], 7);
        assert_eq!(next["commands"].as_array().expect("commands").len(), 1);
        let last = tick(&mut scripts, &id, json!([]));
        assert_eq!(last["commands"], json!([]));
    }
    #[test]
    fn test_errors_disable_after_ten_ticks_and_replacement_expires_subscriptions() {
        let mut scripts = Scripts::new().expect("Luau");
        scripts.call(json!({"method":"script_attach","entity_id":"1","path":"error.luau","source":"function on_update(entity,world,dt) error('broken') end"})).expect("attach");
        for errors in 1..=10 {
            let result = tick(&mut scripts, "1", json!([]));
            assert_eq!(result["instances"][0]["consecutive_errors"], errors);
            assert_eq!(result["instances"][0]["disabled"], errors == 10);
        }
        assert_eq!(tick(&mut scripts, "1", json!([]))["diagnostics"], json!([]));
        scripts.call(json!({"method":"script_attach","entity_id":"1","path":"fresh.luau","source":"function on_spawn(entity,world) world:burst_particles(entity,5) end"})).expect("replace");
        assert_eq!(
            tick(&mut scripts, "1", json!([]))["commands"][0]["count"],
            5
        );
        assert_eq!(tick(&mut scripts, "1", json!([]))["commands"], json!([]));
    }
    #[test]
    fn test_sandbox_and_per_instance_environment_isolation() {
        let mut scripts = Scripts::new().expect("Luau");
        scripts.call(json!({"method":"script_validate","path":"safe.luau","source":"assert(debug==nil and io==nil and package==nil and require==nil and os.execute==nil and os.getenv==nil)"})).expect("sandbox");
        assert!(
            scripts
                .call(
                    json!({"method":"script_validate","path":"escape.luau","source":"math.sin=nil"})
                )
                .is_err()
        );
        scripts.call(json!({"method":"script_attach","entity_id":"1","path":"isolated.luau","source":"counter=0; function on_update(entity,world,dt) counter+=1; world:burst_particles(entity,counter) end"})).expect("attach");
        scripts.call(json!({"method":"script_attach","entity_id":"2","path":"isolated.luau","source":"counter=100; function on_update(entity,world,dt) counter+=1; world:burst_particles(entity,counter) end"})).expect("attach");
        let result=scripts.call(json!({"method":"script_tick","delta_seconds":1,"entities":[entity("1"),entity("2")],"events":[]})).expect("tick");
        assert_eq!(result["commands"][0]["count"], 1);
        assert_eq!(result["commands"][1]["count"], 101);
    }
    #[test]
    fn test_destroy_is_once_and_failed_validation_preserves_existing_instance() {
        let mut scripts = Scripts::new().expect("Luau");
        scripts.call(json!({"method":"script_attach","entity_id":"1","path":"old.luau","source":"function on_spawn(entity,world) world:burst_particles(entity,3) end; function on_destroy(entity) assert(tostring(entity)=='1') end"})).expect("attach");
        assert!(scripts.call(json!({"method":"script_attach","entity_id":"1","path":"bad.luau","source":"function ("})).is_err());
        assert_eq!(
            tick(&mut scripts, "1", json!([]))["commands"][0]["count"],
            3
        );
        scripts
            .call(json!({"method":"script_detach","entity_id":"1"}))
            .expect("destroy");
        scripts
            .call(json!({"method":"script_detach","entity_id":"1"}))
            .expect("idempotent");
    }
    #[test]
    fn test_failed_hooks_discard_their_commands_and_keep_instance_ownership() {
        let mut scripts = Scripts::new().expect("Luau");
        scripts.call(json!({"method":"script_attach","entity_id":"1","path":"errors.luau","source":r#"
            function on_spawn(entity,world)
                world:burst_particles(entity,3)
                world:on_event("bad",function(name,data,current)
                    current:burst_particles(entity,11); error("event failure")
                end)
                world:on_event("good",function(name,data,current) current:burst_particles(entity,7) end)
            end
            function on_update(entity,world,dt) world:burst_particles(entity,9); error("update failure") end
        "#})).expect("attach");
        let events = json!([{"name":"bad","trigger":"1","other":"1"},{"name":"good","trigger":"1","other":"1"}]);
        let first = tick(&mut scripts, "1", events.clone());
        assert_eq!(first["commands"].as_array().expect("commands").len(), 2);
        assert_eq!(first["commands"][0]["count"], 3);
        assert_eq!(first["commands"][1]["count"], 7);
        assert_eq!(first["commands"][1]["owner_id"], "1");
        assert_eq!(first["diagnostics"].as_array().expect("errors").len(), 2);
        let next = tick(&mut scripts, "1", events);
        assert_eq!(next["commands"].as_array().expect("commands").len(), 1);
        assert_eq!(next["instances"][0]["consecutive_errors"], 2);
    }
    #[test]
    fn test_sync_preflights_every_revision_and_retains_unchanged_state() {
        let mut scripts = Scripts::new().expect("Luau");
        let old = "counter=0; function on_update(entity,world,dt) counter+=1; world:burst_particles(entity,counter) end";
        let fresh = "function on_spawn(entity,world) world:burst_particles(entity,100) end";
        scripts.call(json!({"method":"script_sync","scripts":[{"entity_id":"1","source":old,"path":"old.luau"}]})).expect("sync");
        assert_eq!(
            tick(&mut scripts, "1", json!([]))["commands"][0]["count"],
            1
        );
        scripts.call(json!({"method":"script_sync","scripts":[{"entity_id":"1","source":old,"path":"old.luau"}]})).expect("unchanged");
        assert!(scripts.call(json!({"method":"script_sync","scripts":[{"entity_id":"1","source":fresh,"path":"new.luau"},{"entity_id":"2","source":"function (","path":"bad.luau"}]})).is_err());
        assert_eq!(
            tick(&mut scripts, "1", json!([]))["commands"][0]["count"],
            2
        );
        scripts.call(json!({"method":"script_sync","scripts":[{"entity_id":"1","source":fresh,"path":"new.luau"}]})).expect("new revision");
        assert_eq!(
            tick(&mut scripts, "1", json!([]))["commands"][0]["count"],
            100
        );
        assert_eq!(tick(&mut scripts, "1", json!([]))["commands"], json!([]));
    }
    #[test]
    fn test_vec3_snapshot_arithmetic_and_invalid_tick_keeps_deferred_delivery() {
        let mut scripts = Scripts::new().expect("Luau");
        scripts.call(json!({"method":"script_attach","entity_id":"1","path":"vector.luau","source":r#"
            function on_spawn(entity,world)
                local position=world:get_position(entity)
                local direction=Vec3.new(3,4,0)
                assert(direction:length()==5 and world:get_transform(entity).position.x==0)
                world:set_position(entity,position+direction:normalized()*10)
                assert(world:get_position(entity).x==0)
                world:on_event("next",function(name,data,current) current:burst_particles(entity,8) end)
                world:emit("next")
            end
        "#})).expect("attach");
        let first = tick(&mut scripts, "1", json!([]));
        assert_eq!(first["commands"][0]["position"], json!([6.0, 8.0, 0.0]));
        assert!(scripts.call(json!({"method":"script_tick","delta_seconds":0.1,"entities":[entity("1")],"events":[{"name":"","trigger":"1","other":"1"}]})).is_err());
        assert_eq!(
            tick(&mut scripts, "1", json!([]))["commands"][0]["count"],
            8
        );
        assert_eq!(tick(&mut scripts, "1", json!([]))["commands"], json!([]));
    }
    #[test]
    fn test_execution_budget_cancels_runaway_hook_and_discards_partial_commands() {
        let mut scripts = Scripts::new().expect("Luau");
        scripts.call(json!({"method":"script_attach","entity_id":"1","path":"loop.luau","source":"function on_update(entity,world,dt) world:burst_particles(entity,4); while true do end end"})).expect("attach");
        let output = tick(&mut scripts, "1", json!([]));
        assert_eq!(output["commands"], json!([]));
        assert!(
            output["diagnostics"][0]["error"]
                .as_str()
                .expect("error")
                .contains("budget exhausted")
        );
        assert_eq!(output["instances"][0]["consecutive_errors"], 1);
        scripts.call(json!({"method":"script_attach","entity_id":"1","path":"recovered.luau","source":"function on_spawn(entity,world) world:burst_particles(entity,5) end"})).expect("replace");
        assert_eq!(
            tick(&mut scripts, "1", json!([]))["commands"][0]["count"],
            5
        );
    }
}
