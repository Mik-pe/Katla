//! Query system for ergonomic component access.
//!
//! This module provides a type-safe query API for iterating over entities with specific
//! component combinations. The query system uses the type system to express access patterns
//! (mutable vs immutable) and automatically filters entities that don't have all required
//! components.
//!
//! # Examples
//!
//! ```
//! use katla_ecs::{World, Component};
//!
//! #[derive(Component)]
//! struct TransformComponent { x: f32, y: f32, z: f32 }
//!
//! #[derive(Component)]
//! struct VelocityComponent { vx: f32, vy: f32, vz: f32 }
//!
//! let mut world = World::new();
//! world.spawn((
//!     TransformComponent { x: 0.0, y: 0.0, z: 0.0 },
//!     VelocityComponent { vx: 1.0, vy: 0.0, vz: 0.0 },
//! ));
//! world.spawn((
//!     TransformComponent { x: 5.0, y: 0.0, z: 0.0 },
//!     VelocityComponent { vx: 2.0, vy: 0.0, vz: 0.0 },
//! ));
//!
//! // Query single component
//! for (_entity, velocity) in world.query::<&VelocityComponent>() {
//!     assert!(velocity.vx > 0.0);
//! }
//!
//! // Query two components
//! for (_entity, transform, velocity) in world.query::<(&TransformComponent, &VelocityComponent)>() {
//!     assert!(velocity.vx > 0.0);
//! }
//! ```

#[macro_use]
mod macros;
pub mod filter;
pub mod par_query;

pub(crate) use filter::assert_filter_query_disjoint;
pub use filter::{FilteredQueryIter, QueryFilter, With, Without};
pub use par_query::ParQueryData;

use paste::paste;

use crate::EntityId;
use crate::components::Component;
use crate::storage::{ComponentStorage, ComponentStorageManager};
use std::any::TypeId;

// ── Arity 1 ────────────────────────────────────────────────────────────────
impl_query_iter_arity1!();

// ── Arity 2 ────────────────────────────────────────────────────────────────
impl_query_iter_all_ref!(2, T1, T2);
impl_query_iter_single_mut!(2, T1Mut, [], T1, [T2]);
impl_query_iter_single_mut!(2, T2Mut, [T1], T2, []);
impl_query_iter_double_mut!(2, T1T2Mut, [T1, T2], []);

// ── Arity 3 ────────────────────────────────────────────────────────────────
impl_query_iter_all_ref!(3, T1, T2, T3);
impl_query_iter_single_mut!(3, T1Mut, [], T1, [T2, T3]);
impl_query_iter_single_mut!(3, T2Mut, [T1], T2, [T3]);
impl_query_iter_single_mut!(3, T3Mut, [T1, T2], T3, []);

// ── Arity 4 ────────────────────────────────────────────────────────────────
impl_query_iter_all_ref!(4, T1, T2, T3, T4);
impl_query_iter_single_mut!(4, T1Mut, [], T1, [T2, T3, T4]);
impl_query_iter_single_mut!(4, T2Mut, [T1], T2, [T3, T4]);
impl_query_iter_single_mut!(4, T3Mut, [T1, T2], T3, [T4]);
impl_query_iter_single_mut!(4, T4Mut, [T1, T2, T3], T4, []);

// ── Arity 5 ────────────────────────────────────────────────────────────────
impl_query_iter_all_ref!(5, T1, T2, T3, T4, T5);
impl_query_iter_single_mut!(5, T1Mut, [], T1, [T2, T3, T4, T5]);
impl_query_iter_single_mut!(5, T2Mut, [T1], T2, [T3, T4, T5]);
impl_query_iter_single_mut!(5, T3Mut, [T1, T2], T3, [T4, T5]);
impl_query_iter_single_mut!(5, T4Mut, [T1, T2, T3], T4, [T5]);
impl_query_iter_single_mut!(5, T5Mut, [T1, T2, T3, T4], T5, []);

// ── Arity 6 ────────────────────────────────────────────────────────────────
impl_query_iter_all_ref!(6, T1, T2, T3, T4, T5, T6);
impl_query_iter_single_mut!(6, T1Mut, [], T1, [T2, T3, T4, T5, T6]);
impl_query_iter_single_mut!(6, T2Mut, [T1], T2, [T3, T4, T5, T6]);
impl_query_iter_single_mut!(6, T3Mut, [T1, T2], T3, [T4, T5, T6]);
impl_query_iter_single_mut!(6, T4Mut, [T1, T2, T3], T4, [T5, T6]);
impl_query_iter_single_mut!(6, T5Mut, [T1, T2, T3, T4], T5, [T6]);
impl_query_iter_single_mut!(6, T6Mut, [T1, T2, T3, T4, T5], T6, []);

// ── Arity 7 ────────────────────────────────────────────────────────────────
impl_query_iter_all_ref!(7, T1, T2, T3, T4, T5, T6, T7);
impl_query_iter_single_mut!(7, T1Mut, [], T1, [T2, T3, T4, T5, T6, T7]);
impl_query_iter_single_mut!(7, T2Mut, [T1], T2, [T3, T4, T5, T6, T7]);
impl_query_iter_single_mut!(7, T3Mut, [T1, T2], T3, [T4, T5, T6, T7]);
impl_query_iter_single_mut!(7, T4Mut, [T1, T2, T3], T4, [T5, T6, T7]);
impl_query_iter_single_mut!(7, T5Mut, [T1, T2, T3, T4], T5, [T6, T7]);
impl_query_iter_single_mut!(7, T6Mut, [T1, T2, T3, T4, T5], T6, [T7]);
impl_query_iter_single_mut!(7, T7Mut, [T1, T2, T3, T4, T5, T6], T7, []);

// ── Arity 8 ────────────────────────────────────────────────────────────────
impl_query_iter_all_ref!(8, T1, T2, T3, T4, T5, T6, T7, T8);
impl_query_iter_single_mut!(8, T1Mut, [], T1, [T2, T3, T4, T5, T6, T7, T8]);
impl_query_iter_single_mut!(8, T2Mut, [T1], T2, [T3, T4, T5, T6, T7, T8]);
impl_query_iter_single_mut!(8, T3Mut, [T1, T2], T3, [T4, T5, T6, T7, T8]);
impl_query_iter_single_mut!(8, T4Mut, [T1, T2, T3], T4, [T5, T6, T7, T8]);
impl_query_iter_single_mut!(8, T5Mut, [T1, T2, T3, T4], T5, [T6, T7, T8]);
impl_query_iter_single_mut!(8, T6Mut, [T1, T2, T3, T4, T5], T6, [T7, T8]);
impl_query_iter_single_mut!(8, T7Mut, [T1, T2, T3, T4, T5, T6], T7, [T8]);
impl_query_iter_single_mut!(8, T8Mut, [T1, T2, T3, T4, T5, T6, T7], T8, []);

// ── Adding arity 9 is a one-line invocation per permutation: ─────────────
// impl_query_iter_all_ref!(9, T1, T2, T3, T4, T5, T6, T7, T8, T9);
// impl_query_iter_single_mut!(9, T1Mut, [], T1, [T2, T3, T4, T5, T6, T7, T8, T9]);
// impl_query_iter_single_mut!(9, T2Mut, [T1], T2, [T3, T4, T5, T6, T7, T8, T9]);
// ... etc.

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for query types that only produce immutable references.
///
/// This trait is implemented only for patterns that yield shared references
/// (`&T`, `(&T, &U)`, etc.), never for patterns containing `&mut T`.
/// It is used as a bound on [`World::query_ref`](crate::World::query_ref)
/// to close the soundness hole where `query_ref::<&mut T>()` would create
/// mutable references from a shared `&World`.
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
    fn type_ids_for_changed() -> Vec<TypeId>;

    /// Extracts the EntityId from a query item.
    fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId;
}
