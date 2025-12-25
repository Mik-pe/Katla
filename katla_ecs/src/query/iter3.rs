//! Three-component query iterators.

use super::QueryData;
use crate::{Component, ComponentStorage, ComponentStorageManager, EntityId};
use std::any::TypeId;

/// Iterator for querying three components (all immutable).
pub struct QueryIter3<'a, T1: Component, T2: Component, T3: Component> {
    storage2: Option<&'a ComponentStorage<T2>>,
    storage3: Option<&'a ComponentStorage<T3>>,
    iter1: std::slice::Iter<'a, (EntityId, T1)>,
}

impl<'a, T1: Component, T2: Component, T3: Component> Iterator for QueryIter3<'a, T1, T2, T3> {
    type Item = (EntityId, &'a T1, &'a T2, &'a T3);

    fn next(&mut self) -> Option<Self::Item> {
        let storage2 = self.storage2.as_ref()?;
        let storage3 = self.storage3.as_ref()?;
        loop {
            let (entity_id, component1) = self.iter1.next()?;
            if let Some(component2) = storage2.get(*entity_id) {
                if let Some(component3) = storage3.get(*entity_id) {
                    return Some((*entity_id, component1, component2, component3));
                }
            }
        }
    }
}

/// Iterator for querying three components (mutable, immutable, immutable).
pub struct QueryIter3MutRefRef<'a, T1: Component, T2: Component, T3: Component> {
    storage2: Option<&'a ComponentStorage<T2>>,
    storage3: Option<&'a ComponentStorage<T3>>,
    iter1: std::slice::IterMut<'a, (EntityId, T1)>,
}

impl<'a, T1: Component, T2: Component, T3: Component> Iterator
    for QueryIter3MutRefRef<'a, T1, T2, T3>
{
    type Item = (EntityId, &'a mut T1, &'a T2, &'a T3);

    fn next(&mut self) -> Option<Self::Item> {
        let storage2 = self.storage2.as_ref()?;
        let storage3 = self.storage3.as_ref()?;
        loop {
            let (entity_id, component1) = self.iter1.next()?;
            if let Some(component2) = storage2.get(*entity_id) {
                if let Some(component3) = storage3.get(*entity_id) {
                    return Some((*entity_id, component1, component2, component3));
                }
            }
        }
    }
}

/// Iterator for querying three components (immutable, mutable, immutable).
pub struct QueryIter3RefMutRef<'a, T1: Component, T2: Component, T3: Component> {
    storage1: Option<&'a ComponentStorage<T1>>,
    storage3: Option<&'a ComponentStorage<T3>>,
    iter2: std::slice::IterMut<'a, (EntityId, T2)>,
}

impl<'a, T1: Component, T2: Component, T3: Component> Iterator
    for QueryIter3RefMutRef<'a, T1, T2, T3>
{
    type Item = (EntityId, &'a T1, &'a mut T2, &'a T3);

    fn next(&mut self) -> Option<Self::Item> {
        let storage1 = self.storage1.as_ref()?;
        let storage3 = self.storage3.as_ref()?;
        loop {
            let (entity_id, component2) = self.iter2.next()?;
            if let Some(component1) = storage1.get(*entity_id) {
                if let Some(component3) = storage3.get(*entity_id) {
                    return Some((*entity_id, component1, component2, component3));
                }
            }
        }
    }
}

/// Iterator for querying three components (immutable, immutable, mutable).
pub struct QueryIter3RefRefMut<'a, T1: Component, T2: Component, T3: Component> {
    storage1: Option<&'a ComponentStorage<T1>>,
    storage2: Option<&'a ComponentStorage<T2>>,
    iter3: std::slice::IterMut<'a, (EntityId, T3)>,
}

impl<'a, T1: Component, T2: Component, T3: Component> Iterator
    for QueryIter3RefRefMut<'a, T1, T2, T3>
{
    type Item = (EntityId, &'a T1, &'a T2, &'a mut T3);

    fn next(&mut self) -> Option<Self::Item> {
        let storage1 = self.storage1.as_ref()?;
        let storage2 = self.storage2.as_ref()?;
        loop {
            let (entity_id, component3) = self.iter3.next()?;
            if let Some(component1) = storage1.get(*entity_id) {
                if let Some(component2) = storage2.get(*entity_id) {
                    return Some((*entity_id, component1, component2, component3));
                }
            }
        }
    }
}

// Implement QueryData for three immutable components
impl<T1: Component + 'static, T2: Component + 'static, T3: Component + 'static> QueryData
    for (&T1, &T2, &T3)
{
    type Item<'a> = (EntityId, &'a T1, &'a T2, &'a T3);
    type Iter<'a> = QueryIter3<'a, T1, T2, T3>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
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

        let storage1 = storage.get_storage::<T1>();
        let storage2 = storage.get_storage::<T2>();
        let storage3 = storage.get_storage::<T3>();

        if let (Some(s1), Some(s2), Some(s3)) = (storage1, storage2, storage3) {
            QueryIter3 {
                storage2: Some(s2),
                storage3: Some(s3),
                iter1: s1.components.iter(),
            }
        } else {
            QueryIter3 {
                storage2: None,
                storage3: None,
                iter1: [].iter(),
            }
        }
    }
}

// Implement QueryData for (&mut T1, &T2, &T3)
impl<T1: Component + 'static, T2: Component + 'static, T3: Component + 'static> QueryData
    for (&mut T1, &T2, &T3)
{
    type Item<'a> = (EntityId, &'a mut T1, &'a T2, &'a T3);
    type Iter<'a> = QueryIter3MutRefRef<'a, T1, T2, T3>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
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

        unsafe {
            let ptr_mut = storage as *mut ComponentStorageManager;
            let ptr_const = storage as *const ComponentStorageManager;
            let storage1 = (*ptr_mut).get_storage_mut::<T1>();
            let storage2 = (*ptr_const).get_storage::<T2>();
            let storage3 = (*ptr_const).get_storage::<T3>();

            if let (Some(s1), Some(s2), Some(s3)) = (storage1, storage2, storage3) {
                QueryIter3MutRefRef {
                    storage2: Some(s2),
                    storage3: Some(s3),
                    iter1: s1.components.iter_mut(),
                }
            } else {
                QueryIter3MutRefRef {
                    storage2: None,
                    storage3: None,
                    iter1: [].iter_mut(),
                }
            }
        }
    }
}

// Implement QueryData for (&T1, &mut T2, &T3)
impl<T1: Component + 'static, T2: Component + 'static, T3: Component + 'static> QueryData
    for (&T1, &mut T2, &T3)
{
    type Item<'a> = (EntityId, &'a T1, &'a mut T2, &'a T3);
    type Iter<'a> = QueryIter3RefMutRef<'a, T1, T2, T3>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
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

        unsafe {
            let ptr_const = storage as *const ComponentStorageManager;
            let ptr_mut = storage as *mut ComponentStorageManager;
            let storage1 = (*ptr_const).get_storage::<T1>();
            let storage2 = (*ptr_mut).get_storage_mut::<T2>();
            let storage3 = (*ptr_const).get_storage::<T3>();

            if let (Some(s1), Some(s2), Some(s3)) = (storage1, storage2, storage3) {
                QueryIter3RefMutRef {
                    storage1: Some(s1),
                    storage3: Some(s3),
                    iter2: s2.components.iter_mut(),
                }
            } else {
                QueryIter3RefMutRef {
                    storage1: None,
                    storage3: None,
                    iter2: [].iter_mut(),
                }
            }
        }
    }
}

// Implement QueryData for (&T1, &T2, &mut T3)
impl<T1: Component + 'static, T2: Component + 'static, T3: Component + 'static> QueryData
    for (&T1, &T2, &mut T3)
{
    type Item<'a> = (EntityId, &'a T1, &'a T2, &'a mut T3);
    type Iter<'a> = QueryIter3RefRefMut<'a, T1, T2, T3>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
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

        unsafe {
            let ptr_const = storage as *const ComponentStorageManager;
            let ptr_mut = storage as *mut ComponentStorageManager;
            let storage1 = (*ptr_const).get_storage::<T1>();
            let storage2 = (*ptr_const).get_storage::<T2>();
            let storage3 = (*ptr_mut).get_storage_mut::<T3>();

            if let (Some(s1), Some(s2), Some(s3)) = (storage1, storage2, storage3) {
                QueryIter3RefRefMut {
                    storage1: Some(s1),
                    storage2: Some(s2),
                    iter3: s3.components.iter_mut(),
                }
            } else {
                QueryIter3RefRefMut {
                    storage1: None,
                    storage2: None,
                    iter3: [].iter_mut(),
                }
            }
        }
    }
}
