use mlua::Lua;

use crate::error::ScriptError;

const SANDBOX_ERR_PATH: &str = "<sandbox>";

fn set_nil(lua: &Lua, name: &str) -> Result<(), ScriptError> {
    lua.globals()
        .set(name, mlua::Value::Nil)
        .map_err(|e| ScriptError::LoadFailed {
            path: SANDBOX_ERR_PATH.into(),
            source: e,
        })
}

fn os_set_nil(lua: &Lua, name: &str) -> Result<(), ScriptError> {
    let os: mlua::Value = lua
        .globals()
        .get("os")
        .map_err(|e| ScriptError::LoadFailed {
            path: SANDBOX_ERR_PATH.into(),
            source: e,
        })?;

    if let mlua::Value::Table(os_table) = os {
        os_table
            .set(name, mlua::Value::Nil)
            .map_err(|e| ScriptError::LoadFailed {
                path: SANDBOX_ERR_PATH.into(),
                source: e,
            })?;
    }
    Ok(())
}

pub(crate) fn apply_sandbox(lua: &Lua) -> Result<(), ScriptError> {
    set_nil(lua, "debug")?;
    set_nil(lua, "io")?;
    set_nil(lua, "package")?;
    set_nil(lua, "require")?;
    set_nil(lua, "dofile")?;
    set_nil(lua, "loadfile")?;

    let dangerous_os_fns = ["execute", "getenv", "remove", "rename", "tmpname", "exit"];
    for name in &dangerous_os_fns {
        os_set_nil(lua, name)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandboxed_vm() -> Lua {
        let lua = Lua::new_with(mlua::StdLib::ALL_SAFE, mlua::LuaOptions::default()).unwrap();
        apply_sandbox(&lua).unwrap();
        lua
    }

    #[test]
    fn test_debug_is_nil() {
        let lua = sandboxed_vm();
        let val: mlua::Value = lua.globals().get("debug").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_io_is_nil() {
        let lua = sandboxed_vm();
        let val: mlua::Value = lua.globals().get("io").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_package_is_nil() {
        let lua = sandboxed_vm();
        let val: mlua::Value = lua.globals().get("package").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_require_is_nil() {
        let lua = sandboxed_vm();
        let val: mlua::Value = lua.globals().get("require").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_dofile_is_nil() {
        let lua = sandboxed_vm();
        let val: mlua::Value = lua.globals().get("dofile").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_loadfile_is_nil() {
        let lua = sandboxed_vm();
        let val: mlua::Value = lua.globals().get("loadfile").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_execute_is_nil() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("execute").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_getenv_is_nil() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("getenv").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_remove_is_nil() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("remove").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_rename_is_nil() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("rename").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_tmpname_is_nil() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("tmpname").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_exit_is_nil() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("exit").unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_clock_is_available() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("clock").unwrap();
        assert!(!matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_time_is_available() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("time").unwrap();
        assert!(!matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_date_is_available() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("date").unwrap();
        assert!(!matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_os_difftime_is_available() {
        let lua = sandboxed_vm();
        let os: mlua::Table = lua.globals().get("os").unwrap();
        let val: mlua::Value = os.get("difftime").unwrap();
        assert!(!matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_safe_libs_available() {
        let lua = sandboxed_vm();
        let globals = lua.globals();

        let math: mlua::Value = globals.get("math").unwrap();
        assert!(!matches!(math, mlua::Value::Nil));

        let string: mlua::Value = globals.get("string").unwrap();
        assert!(!matches!(string, mlua::Value::Nil));

        let table: mlua::Value = globals.get("table").unwrap();
        assert!(!matches!(table, mlua::Value::Nil));

        let os: mlua::Value = globals.get("os").unwrap();
        assert!(!matches!(os, mlua::Value::Nil));
    }

    #[test]
    fn test_sandboxed_script_cannot_access_debug() {
        let lua = sandboxed_vm();
        let result: mlua::Result<mlua::Value> = lua.load("return debug").eval();
        let val = result.unwrap();
        assert!(matches!(val, mlua::Value::Nil));
    }

    #[test]
    fn test_sandboxed_script_can_use_os_time() {
        let lua = sandboxed_vm();
        let result: f64 = lua.load("return os.time()").eval().unwrap();
        assert!(result > 0.0);
    }

    #[test]
    fn test_sandboxed_script_can_use_os_clock() {
        let lua = sandboxed_vm();
        let result: f64 = lua.load("return os.clock()").eval().unwrap();
        assert!(result >= 0.0);
    }
}
