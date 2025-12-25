//! Single component query iterators.

use super::QueryData;
use crate::{Component, ComponentStorageManager, EntityId};

/// Iterator for querying a single mutable component.
pub struct QueryIter1Mut<'a, T: Component> {
    iter: std::slice::IterMut<'a, (EntityId, T)>,
}

impl<'a, T: Component> Iterator for QueryIter1Mut<'a, T> {
    type Item = (EntityId, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(id, component)| (*id, component))
    }
}

/// Iterator for querying a single immutable component.
pub struct QueryIter1<'a, T: Component> {
    iter: std::slice::Iter<'a, (EntityId, T)>,
}

impl<'a, T: Component> Iterator for QueryIter1<'a, T> {
    type Item = (EntityId, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(id, component)| (*id, component))
    }
}

// Implement QueryData for single mutable component
impl<T: Component + 'static> QueryData for &mut T {
    type Item<'a> = (EntityId, &'a mut T);
    type Iter<'a> = QueryIter1Mut<'a, T>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
        if let Some(store) = storage.get_storage_mut::<T>() {
            QueryIter1Mut {
                iter: store.components.iter_mut(),
            }
        } else {
            QueryIter1Mut {
                iter: [].iter_mut(),
            }
        }
    }
}

// Implement QueryData for single immutable component
impl<T: Component + 'static> QueryData for &T {
    type Item<'a> = (EntityId, &'a T);
    type Iter<'a> = QueryIter1<'a, T>;

    fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
        if let Some(store) = storage.get_storage::<T>() {
            QueryIter1 {
                iter: store.components.iter(),
            }
        } else {
            QueryIter1 { iter: [].iter() }
        }
    }
}
