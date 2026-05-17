use std::fmt;

use crate::component::ScriptInstanceHandle;

#[derive(Debug)]
pub enum ScriptError {
    LoadFailed {
        path: String,
        source: mlua::Error,
    },
    ExecutionFailed {
        path: String,
        line: Option<usize>,
        source: mlua::Error,
    },
    InvalidHook {
        path: String,
        hook: String,
    },
    InstanceNotFound(ScriptInstanceHandle),
    ScriptNotLoaded {
        path: String,
    },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::LoadFailed { path, source } => {
                write!(f, "failed to load script '{path}': {source}")
            }
            ScriptError::ExecutionFailed {
                path,
                line: Some(line),
                source,
            } => {
                write!(f, "execution error in '{path}' at line {line}: {source}")
            }
            ScriptError::ExecutionFailed {
                path,
                line: None,
                source,
            } => {
                write!(f, "execution error in '{path}': {source}")
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
        }
    }
}
