//! Query filter types for excluding entities from queries.
//!
//! Provides [`With<T>`] and [`Without<T>`] marker types that can be combined
//! into filter tuples and passed to [`World::query_filtered`](crate::World::query_filtered).

use std::any::TypeId;
use std::marker::PhantomData;

use crate::components::Component;
use crate::entity::EntityId;
use crate::query::QueryData;
use crate::storage::ComponentStorageManager;

/// Marker type requiring that matched entities have component `T`.
pub struct With<T: Component>(PhantomData<T>);

/// Marker type requiring that matched entities do NOT have component `T`.
pub struct Without<T: Component>(PhantomData<T>);

/// Trait for query filter conditions.
///
/// Implementations check whether an entity satisfies a filter predicate
/// against the component storage. The trait is implemented for [`With<T>`],
/// [`Without<T>`], the unit type `()` (always passes), and tuples of filters
/// (all must pass).
pub trait QueryFilter {
    /// Check whether `entity` satisfies this filter.
    ///
    /// # Safety
    /// `storage` must be valid and not mutably aliased for the duration of the call.
    unsafe fn matches(storage: *const ComponentStorageManager, entity: EntityId) -> bool;

    /// Returns the TypeIds of all component types referenced by this filter.
    fn type_ids() -> Vec<TypeId>;
}

impl<T: Component + 'static> QueryFilter for With<T> {
    unsafe fn matches(storage: *const ComponentStorageManager, entity: EntityId) -> bool {
        // SAFETY: Caller guarantees storage is valid and not mutably aliased.
        unsafe {
            (*storage)
                .get_storage::<T>()
                .is_some_and(|s| s.contains(entity))
        }
    }

    fn type_ids() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
}

impl<T: Component + 'static> QueryFilter for Without<T> {
    unsafe fn matches(storage: *const ComponentStorageManager, entity: EntityId) -> bool {
        // SAFETY: Caller guarantees storage is valid and not mutably aliased.
        unsafe {
            !(*storage)
                .get_storage::<T>()
                .is_some_and(|s| s.contains(entity))
        }
    }

    fn type_ids() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
}

impl QueryFilter for () {
    unsafe fn matches(_storage: *const ComponentStorageManager, _entity: EntityId) -> bool {
        true
    }

    fn type_ids() -> Vec<TypeId> {
        Vec::new()
    }
}

impl<A: QueryFilter, B: QueryFilter> QueryFilter for (A, B) {
    unsafe fn matches(storage: *const ComponentStorageManager, entity: EntityId) -> bool {
        // SAFETY: Caller guarantees storage is valid for both sub-filters.
        unsafe { A::matches(storage, entity) && B::matches(storage, entity) }
    }

    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids
    }
}

impl<A: QueryFilter, B: QueryFilter, C: QueryFilter> QueryFilter for (A, B, C) {
    unsafe fn matches(storage: *const ComponentStorageManager, entity: EntityId) -> bool {
        // SAFETY: Caller guarantees storage is valid for all sub-filters.
        unsafe {
            A::matches(storage, entity)
                && B::matches(storage, entity)
                && C::matches(storage, entity)
        }
    }

    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids.extend(C::type_ids());
        ids
    }
}

impl<A: QueryFilter, B: QueryFilter, C: QueryFilter, D: QueryFilter> QueryFilter for (A, B, C, D) {
    unsafe fn matches(storage: *const ComponentStorageManager, entity: EntityId) -> bool {
        // SAFETY: Caller guarantees storage is valid for all sub-filters.
        unsafe {
            A::matches(storage, entity)
                && B::matches(storage, entity)
                && C::matches(storage, entity)
                && D::matches(storage, entity)
        }
    }

    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids.extend(C::type_ids());
        ids.extend(D::type_ids());
        ids
    }
}

/// Filtering wrapper around any [`QueryData`] iterator.
///
/// Yields only items whose entity satisfies the filter `F`.
///
/// # Panics
///
/// Panics at construction if any filter type overlaps with a query component type.
/// Use the component directly in the query instead of filtering on it.
pub struct FilteredQueryIter<'a, Q: QueryData, F: QueryFilter> {
    pub(crate) inner: Q::Iter<'a>,
    pub(crate) storage_ptr: *const ComponentStorageManager,
    pub(crate) _filter: PhantomData<F>,
}

impl<'a, Q: QueryData, F: QueryFilter> Iterator for FilteredQueryIter<'a, Q, F> {
    type Item = Q::Item<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.inner.next()?;
            let entity = Q::entity_id_from_item(&item);
            // SAFETY: storage_ptr borrows World's storage, which outlives the iterator.
            // Overlap between filter and query types is checked at construction time
            // in World::query_filtered, so filter accesses are always disjoint from
            // any mutable query borrows.
            if unsafe { F::matches(self.storage_ptr, entity) } {
                return Some(item);
            }
        }
    }
}

/// Checks that filter types and query types are disjoint. Panics with a clear
/// message if any type appears in both sets.
pub(crate) fn assert_filter_query_disjoint<Q: QueryData, F: QueryFilter>() {
    let query_ids = Q::type_ids_for_changed();
    let filter_ids = F::type_ids();
    for fid in &filter_ids {
        for qid in &query_ids {
            if fid == qid {
                panic!(
                    "Filter type overlaps with query component type — \
                     use the component directly in the query instead"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Component, World};

    #[derive(Component, Default, PartialEq, Debug)]
    struct Pos {
        x: f32,
    }

    #[derive(Component, Default)]
    struct Vel {
        dx: f32,
    }

    #[derive(Component, Default)]
    struct Static;

    #[test]
    fn test_without_filter() {
        let mut world = World::new();
        let _e1 = world.spawn((Pos { x: 1.0 }, Vel { dx: 0.1 }));
        let _e2 = world.spawn((Pos { x: 2.0 }, Static));

        let results: Vec<_> = world.query_filtered::<&Pos, Without<Static>>().collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.x, 1.0);
    }

    #[test]
    fn test_with_filter() {
        let mut world = World::new();
        let _e1 = world.spawn((Pos { x: 1.0 }, Vel { dx: 0.1 }));
        let _e2 = world.spawn((Pos { x: 2.0 }, Static));

        let results: Vec<_> = world.query_filtered::<&Pos, With<Vel>>().collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.x, 1.0);
    }

    #[test]
    fn test_combined_filter() {
        let mut world = World::new();
        let _e1 = world.spawn((Pos { x: 1.0 }, Vel { dx: 0.1 }));
        let _e2 = world.spawn((Pos { x: 2.0 }, Static));
        let _e3 = world.spawn((Pos { x: 3.0 }, Vel { dx: 0.3 }, Static));

        let results: Vec<_> = world
            .query_filtered::<&Pos, (With<Vel>, Without<Static>)>()
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.x, 1.0);
    }

    #[test]
    fn test_no_filter_unit() {
        let mut world = World::new();
        world.spawn((Pos { x: 1.0 },));
        world.spawn((Pos { x: 2.0 },));

        let results: Vec<_> = world.query_filtered::<&Pos, ()>().collect();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_mutable_query_with_filter() {
        let mut world = World::new();
        let _e1 = world.spawn((Pos { x: 1.0 }, Vel { dx: 0.1 }));
        let _e2 = world.spawn((Pos { x: 2.0 }, Static));

        for (_id, pos, _vel) in world.query_filtered::<(&mut Pos, &Vel), Without<Static>>() {
            pos.x += 10.0;
        }

        let ids: Vec<_> = world.entity_ids().collect();
        let p1 = world.get_component::<Pos>(ids[0]).unwrap();
        assert_eq!(p1.x, 11.0);
    }

    #[test]
    fn test_with_and_without_excludes_all() {
        let mut world = World::new();
        let _e1 = world.spawn((Pos { x: 1.0 }, Vel { dx: 0.1 }));
        let _e2 = world.spawn((Pos { x: 2.0 }, Static));

        // With<Vel> AND Without<Vel> — impossible, should return nothing
        let results: Vec<_> = world
            .query_filtered::<&Pos, (With<Vel>, Without<Vel>)>()
            .collect();
        assert!(results.is_empty());
    }

    #[test]
    fn test_filter_with_multi_component_query() {
        let mut world = World::new();
        let _e1 = world.spawn((Pos { x: 1.0 }, Vel { dx: 0.1 }));
        let _e2 = world.spawn((Pos { x: 2.0 }, Vel { dx: 0.2 }, Static));
        let _e3 = world.spawn((Pos { x: 3.0 },));

        let results: Vec<_> = world
            .query_filtered::<(&Pos, &Vel), Without<Static>>()
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.x, 1.0);
        assert_eq!(results[0].2.dx, 0.1);
    }

    #[test]
    fn test_filter_on_empty_world() {
        let mut world = World::new();
        let results: Vec<_> = world.query_filtered::<&Pos, Without<Static>>().collect();
        assert!(results.is_empty());
    }

    #[test]
    fn test_without_returns_entities_lacking_component() {
        let mut world = World::new();
        let _e1 = world.spawn((Pos { x: 1.0 },));
        let _e2 = world.spawn((Pos { x: 2.0 }, Vel { dx: 0.1 }));

        // Without<Vel> should return only e1
        let results: Vec<_> = world.query_filtered::<&Pos, Without<Vel>>().collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.x, 1.0);
    }

    #[test]
    #[should_panic(expected = "Filter type overlaps with query component type")]
    fn test_filter_overlap_with_query_panics() {
        let mut world = World::new();
        world.spawn((Pos { x: 1.0 },));

        // With<Pos> overlaps with &Pos in the query — must panic
        let _ = world
            .query_filtered::<&Pos, With<Pos>>()
            .collect::<Vec<_>>();
    }

    #[test]
    #[should_panic(expected = "Filter type overlaps with query component type")]
    fn test_filter_overlap_mut_query_panics() {
        let mut world = World::new();
        world.spawn((Pos { x: 1.0 }, Vel { dx: 0.1 }));

        // Without<Vel> overlaps with &Vel in the query — must panic
        let _ = world
            .query_filtered::<(&mut Pos, &Vel), Without<Vel>>()
            .collect::<Vec<_>>();
    }
}
