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

use crate::sparse_set::KeyCursor;
use crate::typed_query::{QueryDescriptor, Read, Write};
use crate::{Component, ComponentStorageManager, EntityId};
use std::{any::TypeId, marker::PhantomData};

mod sealed {
    pub trait Sealed {}
}

/// Sealed query patterns that only yield shared references.
pub trait ImmutableQuery: QueryData + sealed::Sealed {
    /// Fetches an immutable query without borrowing the manager mutably.
    fn fetch_ref(storage: &ComponentStorageManager) -> Self::Iter<'_>;
}

/// A query pattern expressed as component references, up to arity eight.
pub trait QueryData: sealed::Sealed {
    /// The row yielded for an entity.
    type Item<'a>;
    /// Iterator borrowing the component storages.
    type Iter<'a>: Iterator<Item = Self::Item<'a>>;
    /// Borrows the requested component columns.
    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_>;
    /// Component types used by this query.
    fn type_ids_for_changed() -> Vec<TypeId>;
    /// Entity associated with an iterator row.
    fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId;
}

/// Internal mapping between reference patterns and typed column descriptors.
#[doc(hidden)]
pub trait QueryElement: sealed::Sealed {
    type Descriptor: QueryDescriptor;
}
impl<T: Component> sealed::Sealed for &T {}
impl<T: Component> sealed::Sealed for &mut T {}
impl<T: Component> QueryElement for &T {
    type Descriptor = Read<T>;
}
impl<T: Component> QueryElement for &mut T {
    type Descriptor = Write<T>;
}

/// Internal projection from prepared columns into flat legacy rows.
#[doc(hidden)]
pub trait QueryRows: QueryData {
    type Descriptor: QueryDescriptor;
    /// Projects one validated component row.
    ///
    /// # Safety
    /// Addresses must be live for the returned lifetime and exclusively held
    /// when the reference pattern includes mutable components.
    unsafe fn row<'a>(
        id: EntityId,
        pointers: <Self::Descriptor as QueryDescriptor>::Pointers,
    ) -> Self::Item<'a>;
}

/// Lazy join over component columns resolved once at query construction.
pub struct QueryIter<'a, Q: QueryRows> {
    columns: <Q::Descriptor as QueryDescriptor>::Columns<'a>,
    cursor: Option<KeyCursor<'a, EntityId>>,
    marker: PhantomData<&'a mut ()>,
}
impl<'a, Q: QueryRows> Iterator for QueryIter<'a, Q> {
    type Item = Q::Item<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let id = self.cursor.as_mut()?.next()?;
            let Some(pointers) = (unsafe { Q::Descriptor::row_pointer(&self.columns, id) }) else {
                continue;
            };
            return Some(unsafe { Q::row(id, pointers) });
        }
    }
}

fn prepare<'a, Q: QueryRows>(storage: &'a ComponentStorageManager) -> QueryIter<'a, Q> {
    let ids = Q::type_ids_for_changed();
    for (i, id) in ids.iter().enumerate() {
        assert!(
            !ids[..i].contains(id),
            "Cannot query the same component type twice"
        );
    }
    // SAFETY: Mutable patterns are constructed from an exclusive manager borrow;
    // shared construction is restricted to sealed immutable patterns. Each
    // column is split once into immutable sparse metadata and a dense base.
    let columns = unsafe { Q::Descriptor::columns(storage) };
    let cursor = Q::Descriptor::cursor(&columns);
    QueryIter {
        columns,
        cursor,
        marker: PhantomData,
    }
}

impl<T: Component> QueryData for &T {
    type Item<'a> = (EntityId, &'a T);
    type Iter<'a> = QueryIter<'a, Self>;
    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
        prepare::<Self>(storage)
    }
    fn type_ids_for_changed() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
    fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId {
        item.0
    }
}
impl<T: Component> QueryRows for &T {
    type Descriptor = Read<T>;
    unsafe fn row<'a>(
        id: EntityId,
        pointers: <Self::Descriptor as QueryDescriptor>::Pointers,
    ) -> Self::Item<'a> {
        (id, unsafe { Self::Descriptor::borrow(pointers) })
    }
}
impl<T: Component> ImmutableQuery for &T {
    fn fetch_ref(storage: &ComponentStorageManager) -> Self::Iter<'_> {
        prepare::<Self>(storage)
    }
}
impl<T: Component> QueryData for &mut T {
    type Item<'a> = (EntityId, &'a mut T);
    type Iter<'a> = QueryIter<'a, Self>;
    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
        prepare::<Self>(storage)
    }
    fn type_ids_for_changed() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
    fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId {
        item.0
    }
}
impl<T: Component> QueryRows for &mut T {
    type Descriptor = Write<T>;
    unsafe fn row<'a>(
        id: EntityId,
        pointers: <Self::Descriptor as QueryDescriptor>::Pointers,
    ) -> Self::Item<'a> {
        (id, unsafe { Self::Descriptor::borrow(pointers) })
    }
}

impl_legacy_tuple!(A:0);
impl_legacy_tuple!(A:0,B:1);
impl_legacy_tuple!(A:0,B:1,C:2);
impl_legacy_tuple!(A:0,B:1,C:2,D:3);
impl_legacy_tuple!(A:0,B:1,C:2,D:3,E:4);
impl_legacy_tuple!(A:0,B:1,C:2,D:3,E:4,F:5);
impl_legacy_tuple!(A:0,B:1,C:2,D:3,E:4,F:5,G:6);
impl_legacy_tuple!(A:0,B:1,C:2,D:3,E:4,F:5,G:6,H:7);
