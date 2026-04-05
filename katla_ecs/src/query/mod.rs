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
mod iter4;
mod iter5;
mod iter6;
mod iter7;
mod iter8;

pub use iter1::*;
pub use iter2::*;
pub use iter3::*;
pub use iter4::*;
pub use iter5::*;
pub use iter6::*;
pub use iter7::*;
pub use iter8::*;

use crate::components::Component;
use std::any::TypeId;

use crate::EntityId;

mod sealed {
    /// Sealed trait to prevent external implementations of `ImmutableQuery`.
    pub trait Sealed {}
}

/// Marker trait for query types that only produce immutable references.
///
/// This trait is implemented only for patterns that yield shared references
/// (`&T`, `(&T, &U)`, etc.), never for patterns containing `&mut T`.
/// It is used as a bound on [`World::query_ref`](crate::World::query_ref)
/// to close the soundness hole where `query_ref::<&mut T>()` would create
/// mutable references from a shared `&World`.
///
/// # Design
///
/// The trait is sealed via the `Sealed` supertrait — external crates cannot
/// implement `ImmutableQuery` for custom types, so mutable query patterns
/// can never satisfy the bound.
pub trait ImmutableQuery: sealed::Sealed {}

impl<T: Component + 'static> sealed::Sealed for &T {}
impl<T: Component + 'static> ImmutableQuery for &T {}

impl<T1: Component + 'static, T2: Component + 'static> sealed::Sealed for (&T1, &T2) {}
impl<T1: Component + 'static, T2: Component + 'static> ImmutableQuery for (&T1, &T2) {}

impl<T1: Component + 'static, T2: Component + 'static, T3: Component + 'static> sealed::Sealed
    for (&T1, &T2, &T3)
{
}
impl<T1: Component + 'static, T2: Component + 'static, T3: Component + 'static> ImmutableQuery
    for (&T1, &T2, &T3)
{
}

impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
> sealed::Sealed for (&T1, &T2, &T3, &T4)
{
}
impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
> ImmutableQuery for (&T1, &T2, &T3, &T4)
{
}

impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
    T5: Component + 'static,
> sealed::Sealed for (&T1, &T2, &T3, &T4, &T5)
{
}
impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
    T5: Component + 'static,
> ImmutableQuery for (&T1, &T2, &T3, &T4, &T5)
{
}

impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
    T5: Component + 'static,
    T6: Component + 'static,
> sealed::Sealed for (&T1, &T2, &T3, &T4, &T5, &T6)
{
}
impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
    T5: Component + 'static,
    T6: Component + 'static,
> ImmutableQuery for (&T1, &T2, &T3, &T4, &T5, &T6)
{
}

impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
    T5: Component + 'static,
    T6: Component + 'static,
    T7: Component + 'static,
> sealed::Sealed for (&T1, &T2, &T3, &T4, &T5, &T6, &T7)
{
}
impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
    T5: Component + 'static,
    T6: Component + 'static,
    T7: Component + 'static,
> ImmutableQuery for (&T1, &T2, &T3, &T4, &T5, &T6, &T7)
{
}

impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
    T5: Component + 'static,
    T6: Component + 'static,
    T7: Component + 'static,
    T8: Component + 'static,
> sealed::Sealed for (&T1, &T2, &T3, &T4, &T5, &T6, &T7, &T8)
{
}
impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
    T5: Component + 'static,
    T6: Component + 'static,
    T7: Component + 'static,
    T8: Component + 'static,
> ImmutableQuery for (&T1, &T2, &T3, &T4, &T5, &T6, &T7, &T8)
{
}

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

    /// Returns the TypeIds of all component types in this query tuple.
    ///
    /// Used by change detection to determine which component storages to check
    /// for generation counter changes.
    fn type_ids_for_changed() -> Vec<TypeId>;

    /// Extracts the EntityId from a query item.
    ///
    /// All query items are tuples where EntityId is the first element.
    fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId;
}
