//! Query system for ergonomic component access.
//!
//! This module provides a type-safe query API for iterating over entities with specific
//! component combinations. The query system uses the type system to express access patterns
//! (mutable vs immutable) and automatically filters entities that don't have all required
//! components.
//!
//! # Examples
//!
//! ```ignore
//! // Query single component
//! for (entity, transform) in storage.query::<&mut TransformComponent>() {
//!     transform.position += Vec3::new(0.0, 1.0, 0.0);
//! }
//!
//! // Query two components
//! for (entity, velocity, force) in storage.query::<(&mut VelocityComponent, &ForceComponent)>() {
//!     velocity.acceleration = force.value / velocity.mass;
//! }
//!
//! // Query three components
//! for (entity, vel, drag, force) in
//!     storage.query::<(&VelocityComponent, &DragComponent, &mut ForceComponent)>() {
//!     force.value += calculate_drag(vel, drag);
//! }
//! ```

mod iter1;
mod iter2;
mod iter3;

pub use iter1::*;
pub use iter2::*;
pub use iter3::*;

/// Trait for querying components from storage.
///
/// This trait is implemented for tuples of component references, allowing ergonomic
/// iteration over entities with specific component combinations.
///
/// # Safety
///
/// Implementations use unsafe code to create multiple mutable references from a single
/// mutable reference to ComponentStorageManager. This is sound because:
///
/// 1. Each component type has a unique TypeId mapping to distinct HashMap entries
/// 2. HashMap entries don't overlap in memory
/// 3. Runtime checks verify type uniqueness before creating raw pointers
/// 4. Lifetimes ensure references don't outlive the storage manager
pub trait QueryData {
    /// The item type returned by the iterator.
    type Item<'a>;

    /// The iterator type that yields items.
    type Iter<'a>: Iterator<Item = Self::Item<'a>>;

    /// Fetches the query from the storage manager.
    ///
    /// # Panics
    ///
    /// Panics if the same component type is requested multiple times in the query.
    fn fetch(storage: &mut crate::ComponentStorageManager) -> Self::Iter<'_>;
}
