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

/// Wrapper that holds a raw pointer to `ComponentStorageManager` with an erased lifetime.
///
/// Similar to [`UnsafeWorldCell`](crate::unsafe_world_cell::UnsafeWorldCell) — the actual
/// lifetime is bounded by the scope that creates this cell. The caller must ensure the
/// underlying `ComponentStorageManager` outlives all usage of this cell and that no mutable
/// access to the storage occurs while the cell is in use.
///
/// This is the building block for parallel query iteration: rayon's `ParallelIterator` needs
/// `'static` items, so we erase the lifetime through a raw pointer. The `'static` lifetime on
/// the returned references is a fiction — the real lifetime is the duration of the
/// `impl ParallelIterator` returned by `par_fetch`, which borrows the storage manager for
/// the duration of iteration.
#[derive(Copy, Clone)]
struct UnsafeStorageCell(*const ComponentStorageManager);

impl UnsafeStorageCell {
    /// Create a new cell from a shared storage reference.
    ///
    /// The returned cell borrows from `storage` for its entire useful lifetime, even
    /// though this is not expressed in the type system. The caller must ensure no
    /// `&mut ComponentStorageManager` exists while the cell (or anything derived from it)
    /// is in use.
    #[inline]
    fn new(storage: &ComponentStorageManager) -> Self {
        Self(storage as *const ComponentStorageManager)
    }

    /// Returns the dense `(EntityId, T)` slice from the component storage, or an empty
    /// slice if the component type has no storage.
    ///
    /// # Safety (caller-side)
    ///
    /// The returned `&'static` reference is valid for as long as the `ComponentStorageManager`
    /// that produced this cell remains borrowed by the enclosing `par_fetch` call. No
    /// mutable access to that storage may occur during that window.
    #[inline]
    fn dense_slice<T: Component + 'static>(&self) -> &'static [(EntityId, T)] {
        // SAFETY: The caller guarantees `self.0` is a valid pointer to a
        // `ComponentStorageManager` that outlives the returned reference's actual
        // use. We reborrow through the raw pointer instead of using `transmute`,
        // making the lifetime erasure explicit and concentrated here.
        unsafe {
            match (*self.0).get_storage::<T>() {
                Some(s) => s.components_vec().as_slice(),
                None => &[],
            }
        }
    }

    /// Returns a reference to a `ComponentStorage<T>`, or `None` if no storage exists.
    ///
    /// # Safety (caller-side)
    ///
    /// Same invariant as [`UnsafeStorageCell::dense_slice`].
    #[inline]
    fn storage_ref<T: Component + 'static>(&self) -> Option<&'static ComponentStorage<T>> {
        // SAFETY: Same as dense_slice — pointer dereference with caller-guaranteed lifetime.
        unsafe { (*self.0).get_storage::<T>() }
    }
}

// SAFETY: UnsafeStorageCell is designed for concurrent read-only access from rayon
// worker threads. The caller is responsible for ensuring no mutable access to the
// underlying storage occurs while the cell is in use.
unsafe impl Send for UnsafeStorageCell {}
unsafe impl Sync for UnsafeStorageCell {}

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

// ── Arity 1: single component ──────────────────────────────────────────────

impl<T: Component + Sync + 'static> ParQueryData for &T {
    type Item<'a> = (EntityId, &'a T);

    fn par_fetch(
        storage: &ComponentStorageManager,
    ) -> impl ParallelIterator<Item = Self::Item<'_>> {
        let cell = UnsafeStorageCell::new(storage);
        cell.dense_slice::<T>().par_iter().map(|(id, c)| (*id, c))
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
        let cell = UnsafeStorageCell::new(storage);
        let s2 = cell.storage_ref::<T2>();
        cell.dense_slice::<T1>()
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

        let cell = UnsafeStorageCell::new(storage);
        let s2 = cell.storage_ref::<T2>();
        let s3 = cell.storage_ref::<T3>();
        cell.dense_slice::<T1>()
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

        let cell = UnsafeStorageCell::new(storage);
        let s2 = cell.storage_ref::<T2>();
        let s3 = cell.storage_ref::<T3>();
        let s4 = cell.storage_ref::<T4>();
        cell.dense_slice::<T1>()
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
