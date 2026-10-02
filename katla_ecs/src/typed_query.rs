//! Typed query descriptors and batch-scoped component views.

use crate::query::QueryFilter;
use crate::sparse_set::{KeyCursor, SparseView};
use crate::{Component, ComponentAccess, ComponentStorageManager, EntityId};
use rayon::prelude::*;
use std::{marker::PhantomData, sync::Arc};

mod sealed {
    pub trait Sealed {}
}

/// Shared component access in a typed system query.
pub struct Read<T: Component>(PhantomData<T>);
/// Exclusive component access in a typed system query.
pub struct Write<T: Component>(PhantomData<T>);

/// A sealed description of the references yielded by a query.
pub trait QueryDescriptor: sealed::Sealed + 'static {
    /// References borrowed for one view access.
    type Item<'a>;
    /// Prepared component addresses, with no storage-manager address.
    #[doc(hidden)]
    type Pointers: Copy + Send + Sync;
    /// Dense offsets cached until a structural change.
    #[doc(hidden)]
    type Indices: Copy + Send + Sync;
    /// Current dense allocation addresses, refreshed for each batch.
    #[doc(hidden)]
    type Bases: Copy + Send + Sync;
    /// Component access derived from the descriptor.
    fn accesses() -> Vec<ComponentAccess>;
    /// Column views for a direct lazy join.
    #[doc(hidden)]
    type Columns<'a>;
    /// Resolves columns without borrowing component values during iteration.
    ///
    /// # Safety
    /// Writes must be exclusively claimed and structural mutation frozen.
    #[doc(hidden)]
    unsafe fn columns(storage: &ComponentStorageManager) -> Self::Columns<'_>;
    /// Selects the smallest prepared column as a query driver.
    #[doc(hidden)]
    fn cursor<'a>(columns: &Self::Columns<'a>) -> Option<KeyCursor<'a, EntityId>>;
    /// Resolves matching component addresses using prepared sparse metadata.
    ///
    /// # Safety
    /// Columns must remain valid for this structurally frozen query.
    #[doc(hidden)]
    unsafe fn row_pointer(columns: &Self::Columns<'_>, id: EntityId) -> Option<Self::Pointers>;
    /// Resolves dense offsets while storage is structurally frozen.
    #[doc(hidden)]
    fn indices(storage: &ComponentStorageManager, ids: &[EntityId]) -> Vec<Self::Indices>;
    /// Claims current column bases for one batch.
    ///
    /// # Safety
    /// Component writes must be exclusively claimed for the batch.
    #[doc(hidden)]
    unsafe fn bases(storage: &ComponentStorageManager, ids: &[EntityId]) -> Self::Bases;
    /// Resolves a cached row against current dense bases.
    ///
    /// # Safety
    /// Indices must match the current structural epoch and bases.
    #[doc(hidden)]
    unsafe fn pointer(bases: Self::Bases, index: Self::Indices) -> Self::Pointers;
    /// Borrow one resolved row.
    ///
    /// # Safety
    /// Pointers must be live and mutable rows must be exclusively borrowed.
    #[doc(hidden)]
    unsafe fn borrow<'a>(pointers: Self::Pointers) -> Self::Item<'a>;
}

/// Descriptors permitting simultaneous shared view access.
pub trait ReadOnlyDescriptor: QueryDescriptor {}

/// A prepared address whose cross-thread access follows its descriptor's claims.
#[doc(hidden)]
pub struct ComponentPointer<T>(*mut T);
impl<T> Copy for ComponentPointer<T> {}
impl<T> Clone for ComponentPointer<T> {
    fn clone(&self) -> Self {
        *self
    }
}
// SAFETY: Components are Send + Sync. Only sealed descriptors borrow these
// addresses, and a batch claims each write type exclusively before preparation.
unsafe impl<T: Send + Sync> Send for ComponentPointer<T> {}
unsafe impl<T: Send + Sync> Sync for ComponentPointer<T> {}

impl<T: Component> sealed::Sealed for Read<T> {}
impl<T: Component> ReadOnlyDescriptor for Read<T> {}
impl<T: Component> QueryDescriptor for Read<T> {
    type Item<'a> = &'a T;
    type Pointers = ComponentPointer<T>;
    type Indices = usize;
    type Bases = ComponentPointer<(EntityId, T)>;
    type Columns<'a> = Option<SparseView<'a, EntityId, T>>;
    unsafe fn columns(storage: &ComponentStorageManager) -> Self::Columns<'_> {
        storage.get_storage::<T>().map(|column| column.query_view())
    }
    fn cursor<'a>(columns: &Self::Columns<'a>) -> Option<KeyCursor<'a, EntityId>> {
        columns.as_ref().map(|column| column.cursor())
    }
    unsafe fn row_pointer(columns: &Self::Columns<'_>, id: EntityId) -> Option<Self::Pointers> {
        columns.as_ref()?.get_ptr(id).map(ComponentPointer)
    }

    fn accesses() -> Vec<ComponentAccess> {
        vec![ComponentAccess::read::<T>()]
    }
    fn indices(storage: &ComponentStorageManager, ids: &[EntityId]) -> Vec<usize> {
        storage
            .get_storage::<T>()
            .map_or_else(Vec::new, |column| column.selected_indices(ids))
    }
    unsafe fn bases(storage: &ComponentStorageManager, _ids: &[EntityId]) -> Self::Bases {
        ComponentPointer(
            storage
                .get_storage::<T>()
                .map_or(std::ptr::null_mut(), |column| column.dense_base() as *mut _),
        )
    }
    unsafe fn pointer(base: Self::Bases, index: usize) -> Self::Pointers {
        ComponentPointer(unsafe { std::ptr::addr_of!((*base.0.add(index)).1) } as *mut T)
    }
    unsafe fn borrow<'a>(pointer: Self::Pointers) -> Self::Item<'a> {
        unsafe { &*pointer.0 }
    }
}
impl<T: Component> sealed::Sealed for Write<T> {}
impl<T: Component> QueryDescriptor for Write<T> {
    type Item<'a> = &'a mut T;
    type Pointers = ComponentPointer<T>;
    type Indices = usize;
    type Bases = ComponentPointer<(EntityId, T)>;
    type Columns<'a> = Option<SparseView<'a, EntityId, T>>;
    unsafe fn columns(storage: &ComponentStorageManager) -> Self::Columns<'_> {
        unsafe { storage.storage_mut_unchecked::<T>() }
            .map(|column| unsafe { &mut *column }.query_view_mut())
    }
    fn cursor<'a>(columns: &Self::Columns<'a>) -> Option<KeyCursor<'a, EntityId>> {
        columns.as_ref().map(|column| column.cursor())
    }
    unsafe fn row_pointer(columns: &Self::Columns<'_>, id: EntityId) -> Option<Self::Pointers> {
        columns.as_ref()?.get_ptr(id).map(ComponentPointer)
    }

    fn accesses() -> Vec<ComponentAccess> {
        vec![ComponentAccess::write::<T>()]
    }
    fn indices(storage: &ComponentStorageManager, ids: &[EntityId]) -> Vec<usize> {
        storage
            .get_storage::<T>()
            .map_or_else(Vec::new, |column| column.selected_indices(ids))
    }
    unsafe fn bases(storage: &ComponentStorageManager, ids: &[EntityId]) -> Self::Bases {
        ComponentPointer(
            unsafe { storage.storage_mut_unchecked::<T>() }
                .map_or(std::ptr::null_mut(), |column| {
                    unsafe { &mut *column }.selected_mut_base(ids)
                }),
        )
    }
    unsafe fn pointer(base: Self::Bases, index: usize) -> Self::Pointers {
        ComponentPointer(unsafe { std::ptr::addr_of_mut!((*base.0.add(index)).1) })
    }
    unsafe fn borrow<'a>(pointer: Self::Pointers) -> Self::Item<'a> {
        unsafe { &mut *pointer.0 }
    }
}

macro_rules! descriptor_tuple {
    ($($D:ident:$index:tt),+) => {
        impl<$($D:QueryDescriptor),+> sealed::Sealed for ($($D,)+) {}
        impl<$($D:ReadOnlyDescriptor),+> ReadOnlyDescriptor for ($($D,)+) {}
        impl<$($D:QueryDescriptor),+> QueryDescriptor for ($($D,)+) {
            type Item<'a> = ($($D::Item<'a>,)+);
            type Pointers = ($($D::Pointers,)+);
            type Indices = ($($D::Indices,)+);
            type Bases = ($($D::Bases,)+);
            type Columns<'a> = ($($D::Columns<'a>,)+);
            unsafe fn columns(storage:&ComponentStorageManager)->Self::Columns<'_> { ($(unsafe { $D::columns(storage) },)+) }
            fn cursor<'a>(columns:&Self::Columns<'a>)->Option<KeyCursor<'a,EntityId>> {
                let cursors=[$($D::cursor(&columns.$index)?,)+];
                cursors.into_iter().min_by_key(|cursor|cursor.len())
            }
            unsafe fn row_pointer(columns:&Self::Columns<'_>,id:EntityId)->Option<Self::Pointers> { Some(($(unsafe { $D::row_pointer(&columns.$index,id)? },)+)) }
            fn accesses()->Vec<ComponentAccess> { let mut accesses=Vec::new(); $(accesses.extend($D::accesses());)+ accesses }
            fn indices(storage:&ComponentStorageManager,ids:&[EntityId])->Vec<Self::Indices> {
                let columns=($($D::indices(storage,ids),)+);
                (0..ids.len()).map(|i| ($(columns.$index[i],)+)).collect()
            }
            unsafe fn bases(storage:&ComponentStorageManager,ids:&[EntityId])->Self::Bases { ($(unsafe { $D::bases(storage,ids) },)+) }
            unsafe fn pointer(bases:Self::Bases,indices:Self::Indices)->Self::Pointers { ($(unsafe { $D::pointer(bases.$index,indices.$index) },)+) }
            unsafe fn borrow<'a>(pointers:Self::Pointers)->Self::Item<'a> { ($(unsafe { $D::borrow(pointers.$index) },)+) }
        }
    }
}
descriptor_tuple!(A:0);
descriptor_tuple!(A:0,B:1);
descriptor_tuple!(A:0,B:1,C:2);
descriptor_tuple!(A:0,B:1,C:2,D:3);
descriptor_tuple!(A:0,B:1,C:2,D:3,E:4);
descriptor_tuple!(A:0,B:1,C:2,D:3,E:4,F:5);
descriptor_tuple!(A:0,B:1,C:2,D:3,E:4,F:5,G:6);
descriptor_tuple!(A:0,B:1,C:2,D:3,E:4,F:5,G:6,H:7);

/// Typed query parameter, with references supplied only by a runtime view.
pub struct Query<D: QueryDescriptor, F: QueryFilter = ()>(PhantomData<(D, F)>);
impl<D: QueryDescriptor, F: QueryFilter> Query<D, F> {
    /// Validates aliases and returns the scheduler access declaration.
    pub fn accesses() -> Vec<ComponentAccess> {
        let accesses = D::accesses();
        for (i, &a) in accesses.iter().enumerate() {
            for &b in &accesses[..i] {
                assert!(
                    !a.conflicts_with(b),
                    "Query contains overlapping mutable component access"
                );
            }
        }
        for filter in F::type_ids() {
            assert!(
                !accesses.iter().any(|&a| a.type_id() == filter),
                "Filter type overlaps with query component type"
            );
        }
        let mut accesses = accesses;
        accesses.extend(F::type_ids().into_iter().map(ComponentAccess::Read));
        accesses
    }
}

type CachedRows<D> = Arc<[(EntityId, <D as QueryDescriptor>::Indices)]>;

/// Matched entity IDs and dense offsets retained until a structural change.
pub struct QueryCache<D: QueryDescriptor> {
    epoch: Option<u64>,
    ids: Vec<EntityId>,
    rows: CachedRows<D>,
}
impl<D: QueryDescriptor> Default for QueryCache<D> {
    fn default() -> Self {
        Self {
            epoch: None,
            ids: Vec::new(),
            rows: Arc::from([]),
        }
    }
}

/// Current column bases and cached dense row offsets for one frozen batch.
pub struct PreparedQuery<D: QueryDescriptor, F: QueryFilter = ()> {
    rows: CachedRows<D>,
    bases: D::Bases,
    marker: PhantomData<fn() -> F>,
}
impl<D: QueryDescriptor, F: QueryFilter> PreparedQuery<D, F> {
    /// Refreshes column bases; rebuilds membership and offsets at structural changes.
    /// Mutable matches are conservatively marked changed at preparation.
    ///
    /// # Safety
    /// The caller must validate parameter and batch access claims, freeze
    /// structural mutation, and retain storage until every view has been dropped.
    pub(crate) unsafe fn prepare(
        storage: &ComponentStorageManager,
        epoch: u64,
        cache: &mut QueryCache<D>,
    ) -> Self {
        if cache.epoch != Some(epoch) {
            let accesses = D::accesses();
            let driver = accesses
                .iter()
                .copied()
                .map(ComponentAccess::type_id)
                .min_by_key(|&id| storage.type_len(id))
                .expect("nonempty descriptor");
            cache.ids = storage
                .ids_for_type(driver)
                .into_iter()
                .filter(|&id| {
                    accesses
                        .iter()
                        .all(|&a| storage.contains_type(a.type_id(), id))
                        && unsafe { F::matches(storage, id) }
                })
                .collect();
            cache.rows = cache
                .ids
                .iter()
                .copied()
                .zip(D::indices(storage, &cache.ids))
                .collect::<Vec<_>>()
                .into();
            cache.epoch = Some(epoch);
        }
        Self {
            rows: Arc::clone(&cache.rows),
            bases: unsafe { D::bases(storage, &cache.ids) },
            marker: PhantomData,
        }
    }
    /// Grants access within the caller's frozen batch lifetime.
    ///
    /// # Safety
    /// Lifetime `'w` must end before access claims or storage validity end.
    pub(crate) unsafe fn into_view<'w>(self) -> QueryView<'w, D, F> {
        QueryView {
            rows: self.rows,
            bases: self.bases,
            borrow: PhantomData,
            filter: PhantomData,
        }
    }
}

/// A component view whose references cannot outlive a borrow of the view.
///
/// ```compile_fail
/// use katla_ecs::{Component, QueryView, Write, EntityId};
/// #[derive(Component)] struct Position { x:f32 }
/// fn overlap(mut query: QueryView<'_, Write<Position>>, id: EntityId) {
///     let first=query.get_mut(id).unwrap();
///     let second=query.get_mut(id).unwrap();
///     first.x=second.x;
/// }
/// ```
///
/// ```compile_fail
/// use katla_ecs::{Component, QueryView, Read};
/// #[derive(Component)] struct Position { x:f32 }
/// fn escape<'w>(query: QueryView<'w, Read<Position>>) -> &'w Position {
///     query.iter().next().unwrap().1
/// }
/// ```
pub struct QueryView<'w, D: QueryDescriptor, F: QueryFilter = ()> {
    rows: CachedRows<D>,
    bases: D::Bases,
    borrow: PhantomData<&'w mut ()>,
    filter: PhantomData<fn() -> F>,
}
impl<D: QueryDescriptor, F: QueryFilter> QueryView<'_, D, F> {
    /// Number of matching entities.
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    /// Whether no entities match.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    /// Borrows every matching row exclusively, yielding each entity once.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, D::Item<'_>)> {
        self.rows
            .iter()
            .map(|&(id, pointers)| (id, unsafe { D::borrow(D::pointer(self.bases, pointers)) }))
    }
    /// Borrows one matching entity exclusively.
    pub fn get_mut(&mut self, id: EntityId) -> Option<D::Item<'_>> {
        self.rows
            .iter()
            .find(|row| row.0 == id)
            .map(|row| unsafe { D::borrow(D::pointer(self.bases, row.1)) })
    }
    /// Executes each matching row on the calling thread.
    pub fn for_each_mut(&mut self, mut f: impl for<'a> FnMut(EntityId, D::Item<'a>)) {
        for &(id, pointers) in self.rows.iter() {
            f(id, unsafe { D::borrow(D::pointer(self.bases, pointers)) });
        }
    }
    /// Processes disjoint row chunks concurrently and joins before returning.
    pub fn par_for_each_mut(
        &mut self,
        chunk_size: usize,
        f: impl for<'a> Fn(EntityId, D::Item<'a>) + Send + Sync,
    ) {
        assert!(chunk_size > 0, "query chunk size must be positive");
        self.rows.par_chunks(chunk_size).for_each(|chunk| {
            for &(id, pointers) in chunk {
                f(id, unsafe { D::borrow(D::pointer(self.bases, pointers)) });
            }
        });
    }
}
impl<D: ReadOnlyDescriptor, F: QueryFilter> QueryView<'_, D, F> {
    /// Iterates shared rows without granting mutable access.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, D::Item<'_>)> {
        self.rows
            .iter()
            .map(|&(id, pointers)| (id, unsafe { D::borrow(D::pointer(self.bases, pointers)) }))
    }
    /// Borrows one matching entity through a shared view.
    pub fn get(&self, id: EntityId) -> Option<D::Item<'_>> {
        self.rows
            .iter()
            .find(|row| row.0 == id)
            .map(|row| unsafe { D::borrow(D::pointer(self.bases, row.1)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{With, Without, World};
    #[derive(Component)]
    struct Position {
        x: i32,
    }
    #[derive(Component)]
    struct Velocity {
        x: i32,
    }
    #[derive(Component)]
    struct Marker;

    #[test]
    fn test_mutable_rows_can_be_held_and_collected() {
        let mut storage = ComponentStorageManager::new();
        for i in 0..20 {
            let id = EntityId::test_new(i);
            storage.add_component(id, Position { x: i as i32 });
            storage.add_component(id, Velocity { x: 2 });
        }
        let mut cache = QueryCache::default();
        let prepared = unsafe {
            PreparedQuery::<(Write<Position>, Write<Velocity>)>::prepare(&storage, 1, &mut cache)
        };
        let mut query = unsafe { prepared.into_view() };
        let mut rows: Vec<_> = query.iter_mut().collect();
        for (_, (p, v)) in &mut rows {
            p.x += v.x;
            v.x += 1;
        }
        drop(rows);
        for (_, (p, v)) in query.iter_mut() {
            assert!(p.x >= 2);
            assert_eq!(v.x, 3);
        }
    }

    #[test]
    fn test_cached_membership_refreshes_row_addresses_and_rebuilds_at_epoch() {
        let mut storage = ComponentStorageManager::new();
        let id = EntityId::test_new(0);
        storage.add_component(id, Position { x: 1 });
        let mut cache = QueryCache::default();
        {
            let prepared =
                unsafe { PreparedQuery::<Write<Position>>::prepare(&storage, 1, &mut cache) };
            let mut query = unsafe { prepared.into_view() };
            query.get_mut(id).unwrap().x = 2;
        }
        storage.get_component_mut::<Position>(id).unwrap().x = 3;
        {
            let prepared =
                unsafe { PreparedQuery::<Write<Position>>::prepare(&storage, 1, &mut cache) };
            let mut query = unsafe { prepared.into_view() };
            assert_eq!(query.get_mut(id).unwrap().x, 3);
        }
        storage.remove_component::<Position>(id);
        let replacement = EntityId::new(0, 1);
        storage.add_component(replacement, Position { x: 7 });
        let prepared =
            unsafe { PreparedQuery::<Write<Position>>::prepare(&storage, 2, &mut cache) };
        let mut query = unsafe { prepared.into_view() };
        assert!(query.get_mut(id).is_none());
        assert_eq!(query.get_mut(replacement).unwrap().x, 7);
    }

    #[test]
    fn test_duplicate_reads_share_values_and_filters_are_applied() {
        let mut storage = ComponentStorageManager::new();
        let first = EntityId::test_new(0);
        let second = EntityId::test_new(1);
        for id in [first, second] {
            storage.add_component(id, Position { x: 3 });
            storage.add_component(id, Velocity { x: 2 });
        }
        storage.add_component(first, Marker);
        let mut cache = QueryCache::default();
        let prepared = unsafe {
            PreparedQuery::<(Read<Position>,Read<Position>),(With<Velocity>,Without<Marker>)>::prepare(&storage,1,&mut cache)
        };
        let query = unsafe { prepared.into_view() };
        let rows: Vec<_> = query.iter().collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, second);
        assert!(std::ptr::eq(rows[0].1.0, rows[0].1.1));
    }

    #[test]
    fn test_filter_access_is_declared_and_without_missing_column_matches() {
        let access = Query::<Read<Position>, Without<Velocity>>::accesses();
        assert_eq!(
            access,
            vec![
                ComponentAccess::read::<Position>(),
                ComponentAccess::read::<Velocity>()
            ]
        );
        let mut storage = ComponentStorageManager::new();
        let id = EntityId::test_new(0);
        storage.add_component(id, Position { x: 3 });
        let mut cache = QueryCache::default();
        let prepared = unsafe {
            PreparedQuery::<Read<Position>, Without<Velocity>>::prepare(&storage, 1, &mut cache)
        };
        let query = unsafe { prepared.into_view() };
        assert_eq!(query.len(), 1);
        assert_eq!(query.get(id).unwrap().x, 3);
    }

    #[test]
    #[should_panic(expected = "overlapping mutable")]
    fn test_read_write_alias_is_rejected() {
        Query::<(Read<Position>, Write<Position>)>::accesses();
    }
    #[test]
    #[should_panic(expected = "overlapping mutable")]
    fn test_write_write_alias_is_rejected() {
        Query::<(Write<Position>, Write<Position>)>::accesses();
    }
    #[test]
    #[should_panic(expected = "Filter type overlaps")]
    fn test_filter_overlap_is_rejected() {
        Query::<Write<Position>, With<Position>>::accesses();
    }

    #[test]
    fn test_legacy_mutable_rows_are_disjoint_and_mark_changed() {
        let mut world = World::new();
        let mut ids = Vec::new();
        for i in 0..10 {
            ids.push(world.spawn((Position { x: i }, Velocity { x: 2 })));
        }
        world.clear_changed();
        let mut rows: Vec<_> = world.query::<(&mut Position, &mut Velocity)>().collect();
        for (_, p, v) in &mut rows {
            p.x += v.x;
            v.x += 1;
        }
        drop(rows);
        assert_eq!(world.query_changed::<&Position>().count(), 10);
        assert_eq!(world.query_changed::<&Velocity>().count(), 10);
        world.clear_changed();
        let mut rows: Vec<_> = world.query::<(&mut Position, &Velocity)>().collect();
        rows[0].1.x += 1;
        drop(rows);
        assert_eq!(world.query_changed::<&Position>().count(), 10);
        assert_eq!(world.query_changed::<&Velocity>().count(), 0);
        let first = world
            .query_ref::<(&Position, &Velocity)>()
            .collect::<Vec<_>>();
        let second = world.query_ref::<&Position>().collect::<Vec<_>>();
        assert_eq!(first.len(), second.len());
        assert!(first.iter().all(|(_, _, v)| v.x == 3));
        assert_eq!(ids.len(), 10);
    }

    #[test]
    fn test_direct_partial_query_marks_whole_mutable_column_changed() {
        let mut world = World::new();
        world.spawn((Position { x: 1 }, Velocity { x: 2 }));
        world.spawn((Position { x: 3 },));
        world.clear_changed();
        assert_eq!(world.query::<(&mut Position, &Velocity)>().count(), 1);
        assert_eq!(world.query_changed::<&Position>().count(), 2);
        assert_eq!(world.query_changed::<&Velocity>().count(), 0);
        world.clear_changed();
        {
            let mut query = world.query_typed::<(Write<Position>, Read<Velocity>), ()>();
            assert_eq!(query.iter_mut().count(), 1);
        }
        assert_eq!(world.query_changed::<&Position>().count(), 1);
        assert_eq!(world.query_changed::<&Velocity>().count(), 0);
    }

    #[test]
    #[cfg(not(miri))]
    fn test_parallel_chunks_visit_every_mutable_row_once() {
        let mut storage = ComponentStorageManager::new();
        for i in 0..1024 {
            storage.add_component(EntityId::test_new(i), Position { x: 0 });
        }
        let mut cache = QueryCache::default();
        let prepared =
            unsafe { PreparedQuery::<Write<Position>>::prepare(&storage, 1, &mut cache) };
        let mut query = unsafe { prepared.into_view() };
        query.par_for_each_mut(37, |_, p| p.x += 1);
        assert!(query.iter_mut().all(|(_, p)| p.x == 1));
    }
}
