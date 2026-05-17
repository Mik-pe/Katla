use katla_ecs::EntityId;
use mlua::{FromLua, Lua, UserData, UserDataMethods, Value};

#[derive(Clone)]
pub struct LuaEntityId(pub EntityId);

impl FromLua for LuaEntityId {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::UserData(ud) => {
                let borrowed = ud.borrow::<LuaEntityId>()?;
                Ok(borrowed.clone())
            }
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaEntityId".into(),
                message: Some("expected EntityId userdata".into()),
            }),
        }
    }
}

impl UserData for LuaEntityId {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, ()| Ok(this.0.id()));

        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(format!("{}", this.0))
        });

        methods.add_meta_method(mlua::MetaMethod::Eq, |_, this, other: LuaEntityId| {
            Ok(this.0 == other.0)
        });
    }
}

pub fn register_entity_type(lua: &mlua::Lua) -> Result<(), mlua::Error> {
    let entity_table = lua.create_table()?;

    entity_table.set(
        "from_raw",
        lua.create_function(|_, raw_id: u64| Ok(LuaEntityId(EntityId::from_raw(raw_id))))?,
    )?;

    let globals = lua.globals();
    globals.set("Entity", entity_table)?;

    Ok(())
}
