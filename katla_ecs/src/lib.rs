pub mod components;
pub mod entity;
pub mod entity_allocator;
pub mod entity_slot;
pub mod input;
pub mod query;
pub mod resource;
pub mod sparse_set;
pub mod spawn;
pub mod storage;
pub mod system;
pub mod world;

// Re-export commonly used types for convenience
pub use components::Component;
pub use entity::EntityId;
pub use input::InputState;
pub use query::QueryData;
pub use resource::Resource;
pub use sparse_set::SparseSet;
pub use spawn::Spawnable;
pub use storage::{ComponentStorage, ComponentStorageManager};
pub use system::{OrderedSystem, System, SystemExecutionOrder};
pub use world::World;
