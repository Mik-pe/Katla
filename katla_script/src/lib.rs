pub mod bindings;
pub mod component;
pub mod engine;
pub mod error;
pub mod event_bus;
pub mod system;
pub mod watcher;

pub use bindings::script_world::InputSnapshot;
pub use bindings::world::ScriptCommand;
pub use component::{ScriptComponent, ScriptInstanceHandle};
pub use engine::ScriptEngine;
pub use error::ScriptError;
pub use event_bus::EventBus;
pub use system::PendingAudioCommands;
pub use system::PendingRaycastCommands;
pub use system::PendingRaycastResults;
pub use system::ScriptSystem;
pub use system::ScriptsActive;
pub use watcher::ScriptWatcher;

#[cfg(test)]
mod tests;
