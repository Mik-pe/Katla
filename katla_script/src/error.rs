use std::fmt;

use crate::component::ScriptInstanceHandle;

/// Errors that can occur during script loading, execution, or management.
#[derive(Debug)]
pub enum ScriptError {
    /// Failed to load a script file (IO error or Lua syntax error).
    LoadFailed { path: String, source: mlua::Error },
    /// A script execution error occurred (runtime error in Lua code).
    ExecutionFailed {
        path: String,
        function: String,
        source: mlua::Error,
    },
    /// A script defines a hook (on_update, on_spawn, on_destroy) with a non-function value.
    InvalidHook { path: String, hook: String },
    /// Attempted to access a script instance that no longer exists.
    /// This can happen if the instance was removed or the entity was destroyed.
    InstanceNotFound(ScriptInstanceHandle),
    /// Attempted to access a script that was never loaded or was unloaded.
    ScriptNotLoaded { path: String },
    /// Attempted to load a script from a path outside the configured scripts directory.
    /// This is a security measure to prevent scripts from reading arbitrary files.
    PathOutsideScriptsDir { path: String, scripts_dir: String },
    /// Script execution exceeded the configured time limit.
    ScriptTimeout { path: String, timeout_secs: f64 },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::LoadFailed { path, source } => {
                write!(f, "failed to load script '{path}': {source}")
            }
            ScriptError::ExecutionFailed {
                path,
                function,
                source,
            } => {
                write!(f, "error in '{function}' (script: '{path}'): {source}")
            }
            ScriptError::InvalidHook { path, hook } => {
                write!(f, "invalid hook '{hook}' in script '{path}'")
            }
            ScriptError::InstanceNotFound(handle) => {
                write!(f, "script instance not found: {:?}", handle)
            }
            ScriptError::ScriptNotLoaded { path } => {
                write!(f, "script not loaded: '{path}'")
            }
            ScriptError::PathOutsideScriptsDir { path, scripts_dir } => {
                write!(
                    f,
                    "script path '{path}' is outside the scripts directory '{scripts_dir}'"
                )
            }
            ScriptError::ScriptTimeout { path, timeout_secs } => {
                write!(f, "script '{path}' timed out after {timeout_secs}s")
            }
        }
    }
}

impl std::error::Error for ScriptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScriptError::LoadFailed { source, .. } => Some(source),
            ScriptError::ExecutionFailed { source, .. } => Some(source),
            ScriptError::InvalidHook { .. } => None,
            ScriptError::InstanceNotFound(_) => None,
            ScriptError::ScriptNotLoaded { .. } => None,
            ScriptError::PathOutsideScriptsDir { .. } => None,
            ScriptError::ScriptTimeout { .. } => None,
        }
    }
}
