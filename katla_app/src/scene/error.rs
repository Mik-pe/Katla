//! Structured scene diagnostics for tools, editors and runtime callers.

use super::SceneEntityId;
use std::{fmt, path::PathBuf};

/// A validation diagnostic with an optional entity and precise field path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneIssue {
    pub entity: Option<SceneEntityId>,
    pub field: String,
    pub message: String,
}

impl fmt::Display for SceneIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(id) = self.entity {
            write!(f, "entity {id}, ")?;
        }
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Loading never replaces the current document after any of these failures.
#[derive(Debug)]
pub enum SceneError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse(ron::error::SpannedError),
    Encode(ron::Error),
    UnsupportedVersion {
        found: u32,
        supported: u32,
    },
    Migration(String),
    Capture(String),
    Limit {
        field: &'static str,
        maximum: usize,
    },
    Validation(Vec<SceneIssue>),
    Component {
        key: String,
        message: String,
    },
    Entity {
        id: SceneEntityId,
        field: String,
        message: String,
    },
}

impl SceneError {
    pub(crate) fn entity(
        id: SceneEntityId,
        field: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Entity {
            id,
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Parse(error) => write!(f, "Cannot parse scene: {error}"),
            Self::Encode(error) => write!(f, "Cannot serialize scene: {error}"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "Scene version {found} is newer than supported version {supported}"
            ),
            Self::Capture(message) => write!(f, "Cannot capture scene: {message}"),
            Self::Limit { field, maximum } => {
                write!(f, "Scene {field} exceeds the limit of {maximum}")
            }
            Self::Migration(message) => write!(f, "Cannot migrate scene: {message}"),
            Self::Validation(issues) => {
                write!(f, "Invalid scene")?;
                for issue in issues {
                    write!(f, "\n{issue}")?;
                }
                Ok(())
            }
            Self::Component { key, message } => write!(f, "Component '{key}': {message}"),
            Self::Entity { id, field, message } => write!(f, "Entity {id}, {field}: {message}"),
        }
    }
}

impl std::error::Error for SceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse(source) => Some(source),
            Self::Encode(source) => Some(source),
            _ => None,
        }
    }
}
