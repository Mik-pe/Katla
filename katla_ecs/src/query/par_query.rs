//! Parallel query system using rayon for concurrent iteration.
//!
//! Provides `ParQueryData` trait and implementations for read-only parallel
//! iteration over component storages. Follows the same pattern as the
//! sequential query system but uses rayon's parallel iterators.

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::EntityId;
use crate::components::Component;
use crate::storage::{ComponentStorage, ComponentStorageManager};
use std::any::TypeId;

/// Trait for parallel querying of components from storage.
///
/// This is the parallel counterpart to [`QueryData`](super::QueryData).
/// Only supports read-only access (`&T`) for soundness in parallel contexts.
pub trait ParQueryData {
    /// The item type yielded by the parallel iterator.
    /// Must be `Send` because rayon moves items between threads.
    type Item<'a>: Send;

    /// Fetches a parallel iterator from the storage manager.
    fn par_fetch(storage: &ComponentStorageManager)
    -> impl ParallelIterator<Item = Self::Item<'_>>;
}

/// Returns the dense slice from a component storage, or an empty slice if none exists.
///
/// The `'static` lifetime is a fiction to unify match-arm types. The real lifetime
/// is bounded by the caller's return type (`impl ParallelIterator`), which borrows
/// from storage for the duration of iteration.
fn dense_slice<T: Component + 'static>(
    storage: &ComponentStorageManager,
) -> &'static [(EntityId, T)] {
    match storage.get_storage::<T>() {
        Some(s) => {
            let slice: &[(EntityId, T)] = s.components_vec().as_slice();
            unsafe { std::mem::transmute::<&[(EntityId, T)], &[(EntityId, T)]>(slice) }
        }
        None => &[],
    }
}

/// Returns a reference to a component storage, transmuted to `'static` lifetime.
///
/// Same soundness rationale as [`dense_slice`].
fn storage_ref<T: Component + 'static>(
    storage: &ComponentStorageManager,
) -> Option<&'static ComponentStorage<T>> {
    storage
        .get_storage::<T>()
        .map(|s| unsafe { std::mem::transmute::<&ComponentStorage<T>, &ComponentStorage<T>>(s) })
}

// ── Arity 1: single component ──────────────────────────────────────────────

impl<T: Component + Sync + 'static> ParQueryData for &T {
    type Item<'a> = (EntityId, &'a T);

    fn par_fetch(
        storage: &ComponentStorageManager,
    ) -> impl ParallelIterator<Item = Self::Item<'_>> {
        dense_slice::<T>(storage).par_iter().map(|(id, c)| (*id, c))
    }
}

// ── Arity 2: all-ref ───────────────────────────────────────────────────────

impl<T1: Component + Sync + 'static, T2: Component + Sync + 'static> ParQueryData for (&T1, &T2) {
    type Item<'a> = (EntityId, &'a T1, &'a T2);

    fn par_fetch(
        storage: &ComponentStorageManager,
    ) -> impl ParallelIterator<Item = Self::Item<'_>> {
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            "Cannot query the same component type twice"
        );
        let s2 = storage_ref::<T2>(storage);
        dense_slice::<T1>(storage)
            .par_iter()
            .filter_map(move |(id, c1)| {
                let c2 = s2.as_ref()?.get(*id)?;
                Some((*id, c1, c2))
            })
    }
}

// ── Arity 3: all-ref ───────────────────────────────────────────────────────

impl<T1: Component + Sync + 'static, T2: Component + Sync + 'static, T3: Component + Sync + 'static>
    ParQueryData for (&T1, &T2, &T3)
{
    type Item<'a> = (EntityId, &'a T1, &'a T2, &'a T3);

    fn par_fetch(
        storage: &ComponentStorageManager,
    ) -> impl ParallelIterator<Item = Self::Item<'_>> {
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T3>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T2>(),
            TypeId::of::<T3>(),
            "Cannot query the same component type twice"
        );

        let s2 = storage_ref::<T2>(storage);
        let s3 = storage_ref::<T3>(storage);
        dense_slice::<T1>(storage)
            .par_iter()
            .filter_map(move |(id, c1)| {
                let c2 = s2.as_ref()?.get(*id)?;
                let c3 = s3.as_ref()?.get(*id)?;
                Some((*id, c1, c2, c3))
            })
    }
}

// ── Arity 4: all-ref ───────────────────────────────────────────────────────

impl<
    T1: Component + Sync + 'static,
    T2: Component + Sync + 'static,
    T3: Component + Sync + 'static,
    T4: Component + Sync + 'static,
> ParQueryData for (&T1, &T2, &T3, &T4)
{
    type Item<'a> = (EntityId, &'a T1, &'a T2, &'a T3, &'a T4);

    fn par_fetch(
        storage: &ComponentStorageManager,
    ) -> impl ParallelIterator<Item = Self::Item<'_>> {
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T3>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T4>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T2>(),
            TypeId::of::<T3>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T2>(),
            TypeId::of::<T4>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T3>(),
            TypeId::of::<T4>(),
            "Cannot query the same component type twice"
        );

        let s2 = storage_ref::<T2>(storage);
        let s3 = storage_ref::<T3>(storage);
        let s4 = storage_ref::<T4>(storage);
        dense_slice::<T1>(storage)
            .par_iter()
            .filter_map(move |(id, c1)| {
                let c2 = s2.as_ref()?.get(*id)?;
                let c3 = s3.as_ref()?.get(*id)?;
                let c4 = s4.as_ref()?.get(*id)?;
                Some((*id, c1, c2, c3, c4))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Component;
    use crate::storage::ComponentStorageManager;
    use std::collections::HashSet;

    #[derive(Component, Default)]
    struct Transform {
        x: f32,
        y: f32,
    }

    #[derive(Component, Default)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }

    #[derive(Component, Default)]
    struct Mass;

    #[derive(Component, Default)]
    struct Tag;

    fn spawn_entities(storage: &mut ComponentStorageManager, count: usize) -> Vec<EntityId> {
        let mut ids = Vec::with_capacity(count);
        for i in 0..count {
            let id = EntityId::test_new(i as u32);
            ids.push(id);
            storage.add_component(
                id,
                Transform {
                    x: i as f32,
                    y: (i * 2) as f32,
                },
            );
            storage.add_component(
                id,
                Velocity {
                    dx: i as f32 * 0.1,
                    dy: i as f32 * 0.2,
                },
            );
            storage.add_component(id, Mass);
            storage.add_component(id, Tag);
        }
        ids
    }

    #[test]
    fn test_par_query_single_component() {
        let mut storage = ComponentStorageManager::new();
        let ids = spawn_entities(&mut storage, 100);

        let results: HashSet<EntityId> = <&Transform>::par_fetch(&storage)
            .map(|(id, _)| id)
            .collect();

        for id in &ids {
            assert!(results.contains(id));
        }
        assert_eq!(results.len(), 100);
    }

    #[test]
    fn test_par_query_two_components() {
        let mut storage = ComponentStorageManager::new();
        let ids = spawn_entities(&mut storage, 100);

        let results: HashSet<EntityId> = <(&Transform, &Velocity)>::par_fetch(&storage)
            .map(|(id, _, _)| id)
            .collect();

        for id in &ids {
            assert!(results.contains(id));
        }
        assert_eq!(results.len(), 100);
    }

    #[test]
    fn test_par_query_three_components() {
        let mut storage = ComponentStorageManager::new();
        let ids = spawn_entities(&mut storage, 100);

        let results: HashSet<EntityId> = <(&Transform, &Velocity, &Mass)>::par_fetch(&storage)
            .map(|(id, _, _, _)| id)
            .collect();

        for id in &ids {
            assert!(results.contains(id));
        }
        assert_eq!(results.len(), 100);
    }

    #[test]
    fn test_par_query_four_components() {
        let mut storage = ComponentStorageManager::new();
        let ids = spawn_entities(&mut storage, 100);

        let results: HashSet<EntityId> =
            <(&Transform, &Velocity, &Mass, &Tag)>::par_fetch(&storage)
                .map(|(id, _, _, _, _)| id)
                .collect();

        for id in &ids {
            assert!(results.contains(id));
        }
        assert_eq!(results.len(), 100);
    }

    #[test]
    fn test_par_query_matches_sequential_values() {
        let mut storage = ComponentStorageManager::new();
        let _ids = spawn_entities(&mut storage, 100);

        let mut seq: Vec<(EntityId, f32, f32)> = storage
            .query::<(&Transform, &Velocity)>()
            .map(|(id, t, v)| (id, t.x, v.dx))
            .collect();
        seq.sort_by_key(|(id, _, _)| id.id());

        let mut par: Vec<(EntityId, f32, f32)> = <(&Transform, &Velocity)>::par_fetch(&storage)
            .map(|(id, t, v)| (id, t.x, v.dx))
            .collect();
        par.sort_by_key(|(id, _, _)| id.id());

        assert_eq!(seq, par);
    }

    #[test]
    fn test_par_query_filters_missing_components() {
        let mut storage = ComponentStorageManager::new();

        let id0 = EntityId::test_new(0);
        storage.add_component(id0, Transform { x: 1.0, y: 2.0 });

        let id1 = EntityId::test_new(1);
        storage.add_component(id1, Transform { x: 3.0, y: 4.0 });
        storage.add_component(id1, Velocity { dx: 0.5, dy: 0.6 });

        let results: HashSet<EntityId> = <(&Transform, &Velocity)>::par_fetch(&storage)
            .map(|(id, _, _)| id)
            .collect();

        assert_eq!(results.len(), 1);
        assert!(results.contains(&id1));
        assert!(!results.contains(&id0));
    }

    #[test]
    fn test_par_query_empty_storage() {
        let storage = ComponentStorageManager::new();

        assert_eq!(<&Transform>::par_fetch(&storage).count(), 0);
        assert_eq!(<(&Transform, &Velocity)>::par_fetch(&storage).count(), 0);
    }

    #[test]
    fn test_par_query_correct_values() {
        let mut storage = ComponentStorageManager::new();
        let id = EntityId::test_new(42);
        storage.add_component(id, Transform { x: 10.0, y: 20.0 });
        storage.add_component(id, Velocity { dx: 0.5, dy: 0.7 });

        let results: Vec<_> = <(&Transform, &Velocity)>::par_fetch(&storage).collect();
        assert_eq!(results.len(), 1);
        let (eid, t, v) = results[0];
        assert_eq!(eid, id);
        assert_eq!(t.x, 10.0);
        assert_eq!(t.y, 20.0);
        assert_eq!(v.dx, 0.5);
        assert_eq!(v.dy, 0.7);
    }
}
