//! Five-component query iterators.

use super::QueryData;
use crate::{Component, ComponentStorage, ComponentStorageManager, EntityId};
use std::any::TypeId;

/// Iterator for querying five components (all immutable).
pub struct QueryIter5<'a, T1: Component, T2: Component, T3: Component, T4: Component, T5: Component>
{
    storage2: Option<&'a ComponentStorage<T2>>,
    storage3: Option<&'a ComponentStorage<T3>>,
    storage4: Option<&'a ComponentStorage<T4>>,
    storage5: Option<&'a ComponentStorage<T5>>,
    iter1: std::slice::Iter<'a, (EntityId, T1)>,
}

impl<'a, T1: Component, T2: Component, T3: Component, T4: Component, T5: Component> Iterator
    for QueryIter5<'a, T1, T2, T3, T4, T5>
{
    type Item = (EntityId, &'a T1, &'a T2, &'a T3, &'a T4, &'a T5);

    fn next(&mut self) -> Option<Self::Item> {
        let storage2 = self.storage2.as_ref()?;
        let storage3 = self.storage3.as_ref()?;
        let storage4 = self.storage4.as_ref()?;
        let storage5 = self.storage5.as_ref()?;
        loop {
            let (entity_id, component1) = self.iter1.next()?;
            if let Some(component2) = storage2.get(*entity_id) {
                if let Some(component3) = storage3.get(*entity_id) {
                    if let Some(component4) = storage4.get(*entity_id) {
                        if let Some(component5) = storage5.get(*entity_id) {
                            return Some((
                                *entity_id, component1, component2, component3, component4,
                                component5,
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// Iterator for querying five components (first mutable, rest immutable).
pub struct QueryIter5MutRefRefRefRef<
    'a,
    T1: Component,
    T2: Component,
    T3: Component,
    T4: Component,
    T5: Component,
> {
    storage2: Option<&'a ComponentStorage<T2>>,
    storage3: Option<&'a ComponentStorage<T3>>,
    storage4: Option<&'a ComponentStorage<T4>>,
    storage5: Option<&'a ComponentStorage<T5>>,
    iter1: std::slice::IterMut<'a, (EntityId, T1)>,
}

impl<'a, T1: Component, T2: Component, T3: Component, T4: Component, T5: Component> Iterator
    for QueryIter5MutRefRefRefRef<'a, T1, T2, T3, T4, T5>
{
    type Item = (EntityId, &'a mut T1, &'a T2, &'a T3, &'a T4, &'a T5);

    fn next(&mut self) -> Option<Self::Item> {
        let storage2 = self.storage2.as_ref()?;
        let storage3 = self.storage3.as_ref()?;
        let storage4 = self.storage4.as_ref()?;
        let storage5 = self.storage5.as_ref()?;
        loop {
            let (entity_id, component1) = self.iter1.next()?;
            if let Some(component2) = storage2.get(*entity_id) {
                if let Some(component3) = storage3.get(*entity_id) {
                    if let Some(component4) = storage4.get(*entity_id) {
                        if let Some(component5) = storage5.get(*entity_id) {
                            return Some((
                                *entity_id, component1, component2, component3, component4,
                                component5,
                            ));
                        }
                    }
                }
            }
        }
    }
}

// Implement QueryData for five immutable components
impl<
        T1: Component + 'static,
        T2: Component + 'static,
        T3: Component + 'static,
        T4: Component + 'static,
        T5: Component + 'static,
    > QueryData for (&T1, &T2, &T3, &T4, &T5)
{
    type Item<'a> = (EntityId, &'a T1, &'a T2, &'a T3, &'a T4, &'a T5);
    type Iter<'a> = QueryIter5<'a, T1, T2, T3, T4, T5>;

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
            TypeId::of::<T1>(),
            TypeId::of::<T5>(),
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
            TypeId::of::<T2>(),
            TypeId::of::<T5>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T3>(),
            TypeId::of::<T4>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T3>(),
            TypeId::of::<T5>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T4>(),
            TypeId::of::<T5>(),
            "Cannot query the same component type twice"
        );

        let storage1 = storage.get_storage::<T1>();
        let storage2 = storage.get_storage::<T2>();
        let storage3 = storage.get_storage::<T3>();
        let storage4 = storage.get_storage::<T4>();
        let storage5 = storage.get_storage::<T5>();

        if let (Some(s1), Some(s2), Some(s3), Some(s4), Some(s5)) =
            (storage1, storage2, storage3, storage4, storage5)
        {
            QueryIter5 {
                storage2: Some(s2),
                storage3: Some(s3),
                storage4: Some(s4),
                storage5: Some(s5),
                iter1: s1.components_vec().iter(),
            }
        } else {
            QueryIter5 {
                storage2: None,
                storage3: None,
                storage4: None,
                storage5: None,
                iter1: [].iter(),
            }
        }
    }
}

// Implement QueryData for (&mut T1, &T2, &T3, &T4, &T5)
impl<
        T1: Component + 'static,
        T2: Component + 'static,
        T3: Component + 'static,
        T4: Component + 'static,
        T5: Component + 'static,
    > QueryData for (&mut T1, &T2, &T3, &T4, &T5)
{
    type Item<'a> = (EntityId, &'a mut T1, &'a T2, &'a T3, &'a T4, &'a T5);
    type Iter<'a> = QueryIter5MutRefRefRefRef<'a, T1, T2, T3, T4, T5>;

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
            TypeId::of::<T1>(),
            TypeId::of::<T5>(),
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
            TypeId::of::<T2>(),
            TypeId::of::<T5>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T3>(),
            TypeId::of::<T4>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T3>(),
            TypeId::of::<T5>(),
            "Cannot query the same component type twice"
        );
        assert_ne!(
            TypeId::of::<T4>(),
            TypeId::of::<T5>(),
            "Cannot query the same component type twice"
        );

        unsafe {
            let ptr_mut = storage as *mut ComponentStorageManager;
            let ptr_const = storage as *const ComponentStorageManager;
            let storage1 = (*ptr_mut).get_storage_mut::<T1>();
            let storage2 = (*ptr_const).get_storage::<T2>();
            let storage3 = (*ptr_const).get_storage::<T3>();
            let storage4 = (*ptr_const).get_storage::<T4>();
            let storage5 = (*ptr_const).get_storage::<T5>();

            if let (Some(s1), Some(s2), Some(s3), Some(s4), Some(s5)) =
                (storage1, storage2, storage3, storage4, storage5)
            {
                QueryIter5MutRefRefRefRef {
                    storage2: Some(s2),
                    storage3: Some(s3),
                    storage4: Some(s4),
                    storage5: Some(s5),
                    iter1: s1.components_vec_mut().iter_mut(),
                }
            } else {
                QueryIter5MutRefRefRefRef {
                    storage2: None,
                    storage3: None,
                    storage4: None,
                    storage5: None,
                    iter1: [].iter_mut(),
                }
            }
        }
    }
}
