//! Four-component query iterators.

use super::QueryData;
use crate::{Component, ComponentStorage, ComponentStorageManager, EntityId};
use std::any::TypeId;

/// Iterator for querying four components (all immutable).
pub struct QueryIter4<'a, T1: Component, T2: Component, T3: Component, T4: Component> {
    storage2: Option<&'a ComponentStorage<T2>>,
    storage3: Option<&'a ComponentStorage<T3>>,
    storage4: Option<&'a ComponentStorage<T4>>,
    iter1: std::slice::Iter<'a, (EntityId, T1)>,
}

impl<'a, T1: Component, T2: Component, T3: Component, T4: Component> Iterator
    for QueryIter4<'a, T1, T2, T3, T4>
{
    type Item = (EntityId, &'a T1, &'a T2, &'a T3, &'a T4);

    fn next(&mut self) -> Option<Self::Item> {
        let storage2 = self.storage2.as_ref()?;
        let storage3 = self.storage3.as_ref()?;
        let storage4 = self.storage4.as_ref()?;
        loop {
            let (entity_id, component1) = self.iter1.next()?;
            if let Some(component2) = storage2.get(*entity_id)
                && let Some(component3) = storage3.get(*entity_id)
                && let Some(component4) = storage4.get(*entity_id)
            {
                return Some((*entity_id, component1, component2, component3, component4));
            }
        }
    }
}

/// Iterator for querying four components (first mutable, rest immutable).
pub struct QueryIter4MutRefRefRef<'a, T1: Component, T2: Component, T3: Component, T4: Component> {
    storage2: Option<&'a ComponentStorage<T2>>,
    storage3: Option<&'a ComponentStorage<T3>>,
    storage4: Option<&'a ComponentStorage<T4>>,
    iter1: std::slice::IterMut<'a, (EntityId, T1)>,
}

impl<'a, T1: Component, T2: Component, T3: Component, T4: Component> Iterator
    for QueryIter4MutRefRefRef<'a, T1, T2, T3, T4>
{
    type Item = (EntityId, &'a mut T1, &'a T2, &'a T3, &'a T4);

    fn next(&mut self) -> Option<Self::Item> {
        let storage2 = self.storage2.as_ref()?;
        let storage3 = self.storage3.as_ref()?;
        let storage4 = self.storage4.as_ref()?;
        loop {
            let (entity_id, component1) = self.iter1.next()?;
            if let Some(component2) = storage2.get(*entity_id)
                && let Some(component3) = storage3.get(*entity_id)
                && let Some(component4) = storage4.get(*entity_id)
            {
                return Some((*entity_id, component1, component2, component3, component4));
            }
        }
    }
}

// Implement QueryData for four immutable components
impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
> QueryData for (&T1, &T2, &T3, &T4)
{
    type Item<'a> = (EntityId, &'a T1, &'a T2, &'a T3, &'a T4);
    type Iter<'a> = QueryIter4<'a, T1, T2, T3, T4>;

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

        let storage1 = storage.get_storage::<T1>();
        let storage2 = storage.get_storage::<T2>();
        let storage3 = storage.get_storage::<T3>();
        let storage4 = storage.get_storage::<T4>();

        if let (Some(s1), Some(s2), Some(s3), Some(s4)) = (storage1, storage2, storage3, storage4) {
            QueryIter4 {
                storage2: Some(s2),
                storage3: Some(s3),
                storage4: Some(s4),
                iter1: s1.components_vec().iter(),
            }
        } else {
            QueryIter4 {
                storage2: None,
                storage3: None,
                storage4: None,
                iter1: [].iter(),
            }
        }
    }

    fn type_ids_for_changed() -> Vec<TypeId> {
        vec![
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            TypeId::of::<T3>(),
            TypeId::of::<T4>(),
        ]
    }

    fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId {
        item.0
    }
}

// Implement QueryData for (&mut T1, &T2, &T3, &T4)
impl<
    T1: Component + 'static,
    T2: Component + 'static,
    T3: Component + 'static,
    T4: Component + 'static,
> QueryData for (&mut T1, &T2, &T3, &T4)
{
    type Item<'a> = (EntityId, &'a mut T1, &'a T2, &'a T3, &'a T4);
    type Iter<'a> = QueryIter4MutRefRefRef<'a, T1, T2, T3, T4>;

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

        unsafe {
            let ptr = storage.borrow_ptr();
            let (storage1, storage2) =
                ComponentStorageManager::get_storage_mut_and_ref::<T1, T2>(ptr);
            let storage3 = (*ptr).get_storage::<T3>();
            let storage4 = (*ptr).get_storage::<T4>();

            if let (Some(s1), Some(s2), Some(s3), Some(s4)) =
                (storage1, storage2, storage3, storage4)
            {
                QueryIter4MutRefRefRef {
                    storage2: Some(s2),
                    storage3: Some(s3),
                    storage4: Some(s4),
                    iter1: s1.components_vec_mut().iter_mut(),
                }
            } else {
                QueryIter4MutRefRefRef {
                    storage2: None,
                    storage3: None,
                    storage4: None,
                    iter1: [].iter_mut(),
                }
            }
        }
    }

    fn type_ids_for_changed() -> Vec<TypeId> {
        vec![
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            TypeId::of::<T3>(),
            TypeId::of::<T4>(),
        ]
    }

    fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId {
        item.0
    }
}
