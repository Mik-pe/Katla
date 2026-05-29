pub mod bindings;
pub mod component;
pub mod engine;
pub mod error;
pub mod event_bus;
mod sandbox;
pub mod system;
pub mod watcher;

pub use bindings::script_world::InputSnapshot;
pub use bindings::world::ScriptCommand;
pub use component::{ScriptComponent, ScriptInstanceHandle};
pub use engine::ScriptEngine;
pub use engine::ScriptVarValue;
pub use error::ScriptError;
pub use event_bus::EventBus;
pub use system::PendingAudioCommands;
pub use system::PendingPhysicsEvents;
pub use system::PendingRaycastCommands;
pub use system::PendingRaycastResults;
pub use system::PendingScriptVarEdits;
pub use system::PhysicsCollisionEvent;
pub use system::PhysicsCollisionEventType;
pub use system::PopulateScriptInspector;
pub use system::ScriptInspectorData;
pub use system::ScriptSystem;
pub use system::ScriptsActive;
pub use watcher::ScriptWatcher;

#[cfg(test)]
mod tests;
