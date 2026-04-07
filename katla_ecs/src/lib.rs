pub mod components;
pub mod entity;
pub mod events;
pub mod query;
pub mod resource;
pub mod spawn;
pub(crate) mod storage;
pub mod system;
pub mod world;

// Internal implementation modules
mod entity_allocator;
mod entity_slot;
mod sparse_set;

// Re-export commonly used types for convenience
pub use components::Component;
pub use entity::EntityId;
pub use events::{ComponentEvent, EntityEvent};
pub use resource::Resource;
pub use spawn::Spawnable;
pub(crate) use storage::ComponentStorageManager;
pub use system::{System, SystemExecutionOrder};
pub use world::World;
