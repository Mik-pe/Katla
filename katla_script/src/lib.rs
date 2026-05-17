pub mod bindings;
pub mod component;
pub mod engine;
pub mod error;
pub mod system;

pub use bindings::script_world::InputSnapshot;
pub use bindings::world::ScriptCommand;
pub use component::{ScriptComponent, ScriptInstanceHandle};
pub use engine::ScriptEngine;
pub use error::ScriptError;
pub use system::ScriptSystem;
pub use system::ScriptsActive;

#[cfg(test)]
mod tests;
