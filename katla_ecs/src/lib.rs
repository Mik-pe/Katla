extern crate self as katla_ecs;

pub mod components;
pub mod entity;
pub mod events;
pub mod query;
pub mod resource;
pub(crate) mod scheduler;
pub mod spawn;
pub(crate) mod storage;
pub mod system;
pub mod world;

#[cfg(feature = "editor")]
pub mod inspect;

#[cfg(feature = "editor")]
pub mod agent;

#[cfg(feature = "editor")]
pub mod scene_tool;

mod archetype;

// Internal implementation modules
mod entity_allocator;
mod entity_slot;
mod sparse_set;
pub(crate) mod unsafe_world_cell;

// Re-export commonly used types for convenience
pub use components::Component;
pub use entity::EntityId;
pub use events::{ComponentEvent, EntityEvent};
pub use query::{FilteredQueryIter, QueryFilter, With, Without};
pub use resource::Resource;
pub use spawn::Spawnable;
pub(crate) use storage::ComponentStorageManager;
pub use system::{ComponentAccess, System, SystemExecutionOrder};
pub use world::World;

#[cfg(feature = "editor")]
pub use inspect::{FieldConstraints, FieldInfo, FieldKind, FieldMut, Inspect};
