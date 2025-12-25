//! Two-component query iterators.

use super::QueryData;
use crate::{Component, ComponentStorage, ComponentStorageManager, EntityId};
use std::any::TypeId;

/// Iterator for querying two components (both immutable).
pub struct QueryIter2RefRef<'a, T1: Component, T2: Component> {
    storage2: Option<&'a ComponentStorage<T2>>,
    iter1: std::slice::Iter<'a, (EntityId, T1)>,
}

impl<'a, T1: Component, T2: Component> Iterator for QueryIter2RefRef<'a, T1, T2> {
    type Item = (EntityId, &'a T1, &'a T2);

    fn next(&mut self) -> Option<Self::Item> {
        let storage2 = self.storage2.as_ref()?;
        loop {
            let (entity_id, component1) = self.iter1.next()?;
            if let Some(component2) = storage2.get(*entity_id) {
                return Some((*entity_id, component1, component2));
            }
        }
    }
}

/// Iterator for querying two components (both mutable).
pub struct QueryIter2MutMut<'a, T1: Component, T2: Component> {
    storage1: Option<&'a mut ComponentStorage<T1>>,
    storage2_vec: Option<&'a mut Vec<(EntityId, T2)>>,
    index: usize,
}

impl<'a, T1: Component, T2: Component> Iterator for QueryIter2MutMut<'a, T1, T2> {
    type Item = (EntityId, &'a mut T1, &'a mut T2);

    fn next(&mut self) -> Option<Self::Item> {
        let storage1 = self.storage1.as_mut()?;
        let storage2_vec = self.storage2_vec.as_mut()?;

        while self.index < storage2_vec.len() {
            let idx = self.index;
            self.index += 1;

            let entity_id = storage2_vec[idx].0;
            if let Some(component1) = storage1.get_mut(entity_id) {
                // SAFETY: We're extending the lifetime here, but it's safe because:
                // 1. component1 comes from storage1 which is borrowed for 'a
                // 2. component2 comes from storage2_vec which is borrowed for 'a
                // 3. We only access each element once due to the index
                let component1_ptr = component1 as *mut T1;
                let component2_ptr = &mut storage2_vec[idx].1 as *mut T2;
                unsafe {
                    return Some((entity_id, &mut *component1_ptr, &mut *component2_ptr));
                }
            }
        }
        None
    }
}

/// Iterator for querying two components (mutable, immutable).
pub struct QueryIter2MutRef<'a, T1: Component, T2: Component> {
    storage2: Option<&'a ComponentStorage<T2>>,
    iter1: std::slice::IterMut<'a, (EntityId, T1)>,
}

impl<'a, T1: Component, T2: Component> Iterator for QueryIter2MutRef<'a, T1, T2> {
    type Item = (EntityId, &'a mut T1, &'a T2);

    fn next(&mut self) -> Option<Self::Item> {
        let storage2 = self.storage2.as_ref()?;
        loop {
            let (entity_id, component1) = self.iter1.next()?;
            if let Some(component2) = storage2.get(*entity_id) {
                return Some((*entity_id, component1, component2));
            }
        }
    }
}

/// Iterator for querying two components (immutable, mutable).
pub struct QueryIter2RefMut<'a, T1: Component, T2: Component> {
    storage1: Option<&'a ComponentStorage<T1>>,
    iter2: std::slice::IterMut<'a, (EntityId, T2)>,
}

impl<'a, T1: Component, T2: Component> Iterator for QueryIter2RefMut<'a, T1, T2> {
    type Item = (EntityId, &'a T1, &'a mut T2);

    fn next(&mut self) -> Option<Self::Item> {
        let storage1 = self.storage1.as_ref()?;
        loop {
            let (entity_id, component2) = self.iter2.next()?;
            if let Some(component1) = storage1.get(*entity_id) {
                return Some((*entity_id, component1, component2));
            }
        }
    }
}

// Implement QueryData for two mutable components
impl<T1: Component + 'static, T2: Component + 'static> QueryData for (&mut T1, &mut T2) {
    type Item<'a> = (EntityId, &'a mut T1, &'a mut T2);
    type Iter<'a> = QueryIter2MutMut<'a, T1, T2>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            "Cannot query the same component type twice"
        );

        unsafe {
            let ptr = storage as *mut ComponentStorageManager;
            let storage1 = (*ptr).get_storage_mut::<T1>();
            let storage2 = (*ptr).get_storage_mut::<T2>();

            if let (Some(s1), Some(s2)) = (storage1, storage2) {
                QueryIter2MutMut {
                    storage1: Some(s1),
                    storage2_vec: Some(&mut s2.components),
                    index: 0,
                }
            } else {
                QueryIter2MutMut {
                    storage1: None,
                    storage2_vec: None,
                    index: 0,
                }
            }
        }
    }
}

// Implement QueryData for (mutable, immutable)
impl<T1: Component + 'static, T2: Component + 'static> QueryData for (&mut T1, &T2) {
    type Item<'a> = (EntityId, &'a mut T1, &'a T2);
    type Iter<'a> = QueryIter2MutRef<'a, T1, T2>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            "Cannot query the same component type twice"
        );

        unsafe {
            let ptr_mut = storage as *mut ComponentStorageManager;
            let ptr_const = storage as *const ComponentStorageManager;
            let storage1 = (*ptr_mut).get_storage_mut::<T1>();
            let storage2 = (*ptr_const).get_storage::<T2>();

            if let (Some(s1), Some(s2)) = (storage1, storage2) {
                QueryIter2MutRef {
                    storage2: Some(s2),
                    iter1: s1.components.iter_mut(),
                }
            } else {
                QueryIter2MutRef {
                    storage2: None,
                    iter1: [].iter_mut(),
                }
            }
        }
    }
}

// Implement QueryData for two immutable components
impl<T1: Component + 'static, T2: Component + 'static> QueryData for (&T1, &T2) {
    type Item<'a> = (EntityId, &'a T1, &'a T2);
    type Iter<'a> = QueryIter2RefRef<'a, T1, T2>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            "Cannot query the same component type twice"
        );

        let storage1 = storage.get_storage::<T1>();
        let storage2 = storage.get_storage::<T2>();

        if let (Some(s1), Some(s2)) = (storage1, storage2) {
            QueryIter2RefRef {
                storage2: Some(s2),
                iter1: s1.components.iter(),
            }
        } else {
            QueryIter2RefRef {
                storage2: None,
                iter1: [].iter(),
            }
        }
    }
}

// Implement QueryData for (immutable, mutable)
impl<T1: Component + 'static, T2: Component + 'static> QueryData for (&T1, &mut T2) {
    type Item<'a> = (EntityId, &'a T1, &'a mut T2);
    type Iter<'a> = QueryIter2RefMut<'a, T1, T2>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
        assert_ne!(
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            "Cannot query the same component type twice"
        );

        unsafe {
            let ptr_const = storage as *const ComponentStorageManager;
            let ptr_mut = storage as *mut ComponentStorageManager;
            let storage1 = (*ptr_const).get_storage::<T1>();
            let storage2 = (*ptr_mut).get_storage_mut::<T2>();

            if let (Some(s1), Some(s2)) = (storage1, storage2) {
                QueryIter2RefMut {
                    storage1: Some(s1),
                    iter2: s2.components.iter_mut(),
                }
            } else {
                QueryIter2RefMut {
                    storage1: None,
                    iter2: [].iter_mut(),
                }
            }
        }
    }
}
