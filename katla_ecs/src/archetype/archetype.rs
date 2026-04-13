use std::any::TypeId;
use std::collections::HashMap;

use crate::entity::EntityId;

use super::column::ComponentColumn;
use super::signature::ArchetypeId;

pub struct Archetype {
    columns: HashMap<TypeId, ComponentColumn>,
    entity_ids: Vec<EntityId>,
    archetype_id: ArchetypeId,
}

impl Archetype {
    pub fn new(archetype_id: ArchetypeId) -> Self {
        Self {
            columns: HashMap::new(),
            entity_ids: Vec::new(),
            archetype_id,
        }
    }

    pub fn archetype_id(&self) -> ArchetypeId {
        self.archetype_id
    }

    pub fn ensure_column<T: Clone + 'static>(&mut self) {
        let type_id = TypeId::of::<T>();
        self.columns
            .entry(type_id)
            .or_insert_with(ComponentColumn::new::<T>);
    }

    pub fn push_component<T: Clone + 'static>(&mut self, component: T) {
        let type_id = TypeId::of::<T>();
        let col = self
            .columns
            .entry(type_id)
            .or_insert_with(ComponentColumn::new::<T>);
        col.push(component);
    }

    pub fn push_entity(&mut self, entity_id: EntityId) -> usize {
        let index = self.entity_ids.len();
        self.entity_ids.push(entity_id);
        index
    }

    pub fn remove_entity_swap(&mut self, index: usize) {
        assert!(
            index < self.entity_ids.len(),
            "remove_entity_swap index out of bounds"
        );
        self.entity_ids.swap_remove(index);
        for column in self.columns.values_mut() {
            column.remove_swap(index);
        }
    }

    pub fn get_column(&self, type_id: TypeId) -> Option<&ComponentColumn> {
        self.columns.get(&type_id)
    }

    pub fn get_column_mut(&mut self, type_id: TypeId) -> Option<&mut ComponentColumn> {
        self.columns.get_mut(&type_id)
    }

    pub fn has_component(&self, type_id: TypeId) -> bool {
        self.columns.contains_key(&type_id)
    }

    pub fn len(&self) -> usize {
        self.entity_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entity_ids.is_empty()
    }

    pub fn entity_ids(&self) -> &[EntityId] {
        &self.entity_ids
    }

    pub fn column_slice<T: 'static>(&self) -> Option<&[T]> {
        self.columns.get(&TypeId::of::<T>())?.as_slice()
    }

    pub fn column_slice_mut<T: 'static>(&mut self) -> Option<&mut [T]> {
        self.columns.get_mut(&TypeId::of::<T>())?.as_slice_mut()
    }

    pub fn iter_1<A: 'static>(&self) -> Option<ArchetypeIter1<'_, A>> {
        let slice = self.column_slice::<A>()?;
        Some(ArchetypeIter1 {
            inner: self.entity_ids.iter().zip(slice.iter()),
        })
    }

    pub fn iter_2<A: 'static, B: 'static>(&self) -> Option<ArchetypeIter2<'_, A, B>> {
        let a = self.column_slice::<A>()?;
        let b = self.column_slice::<B>()?;
        Some(ArchetypeIter2 {
            inner: self.entity_ids.iter().zip(a.iter()).zip(b.iter()),
        })
    }

    pub fn iter_3<A: 'static, B: 'static, C: 'static>(
        &self,
    ) -> Option<ArchetypeIter3<'_, A, B, C>> {
        let a = self.column_slice::<A>()?;
        let b = self.column_slice::<B>()?;
        let c = self.column_slice::<C>()?;
        Some(ArchetypeIter3 {
            inner: self
                .entity_ids
                .iter()
                .zip(a.iter())
                .zip(b.iter())
                .zip(c.iter()),
        })
    }
}

pub struct ArchetypeIter1<'a, A> {
    inner: std::iter::Zip<std::slice::Iter<'a, EntityId>, std::slice::Iter<'a, A>>,
}

impl<'a, A> Iterator for ArchetypeIter1<'a, A> {
    type Item = (EntityId, &'a A);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(eid, a)| (*eid, a))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

pub struct ArchetypeIter2<'a, A, B> {
    inner: std::iter::Zip<
        std::iter::Zip<std::slice::Iter<'a, EntityId>, std::slice::Iter<'a, A>>,
        std::slice::Iter<'a, B>,
    >,
}

impl<'a, A, B> Iterator for ArchetypeIter2<'a, A, B> {
    type Item = (EntityId, &'a A, &'a B);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|((eid, a), b)| (*eid, a, b))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

pub struct ArchetypeIter3<'a, A, B, C> {
    inner: std::iter::Zip<
        std::iter::Zip<
            std::iter::Zip<std::slice::Iter<'a, EntityId>, std::slice::Iter<'a, A>>,
            std::slice::Iter<'a, B>,
        >,
        std::slice::Iter<'a, C>,
    >,
}

impl<'a, A, B, C> Iterator for ArchetypeIter3<'a, A, B, C> {
    type Item = (EntityId, &'a A, &'a B, &'a C);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(((eid, a), b), c)| (*eid, a, b, c))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eid(index: u32) -> EntityId {
        EntityId::test_new(index)
    }

    #[test]
    fn test_new_archetype() {
        let arch = Archetype::new(ArchetypeId(1));
        assert!(arch.is_empty());
        assert_eq!(arch.archetype_id(), ArchetypeId(1));
        assert!(!arch.has_component(TypeId::of::<i32>()));
    }

    #[test]
    fn test_push_entity_and_components() {
        let mut arch = Archetype::new(ArchetypeId(1));

        arch.push_component(10i32);
        arch.push_entity(eid(0));
        arch.push_component(20i32);
        arch.push_entity(eid(1));

        assert_eq!(arch.len(), 2);
        assert_eq!(arch.entity_ids(), &[eid(0), eid(1)]);
        assert!(arch.has_component(TypeId::of::<i32>()));
    }

    #[test]
    fn test_remove_entity_swap() {
        let mut arch = Archetype::new(ArchetypeId(1));

        arch.push_component(10i32);
        arch.push_entity(eid(0));
        arch.push_component(20i32);
        arch.push_entity(eid(1));
        arch.push_component(30i32);
        arch.push_entity(eid(2));

        arch.remove_entity_swap(0);

        assert_eq!(arch.len(), 2);
        assert_eq!(arch.entity_ids(), &[eid(2), eid(1)]);
        let col = arch.get_column(TypeId::of::<i32>()).unwrap();
        assert_eq!(*col.get::<i32>(0).unwrap(), 30);
        assert_eq!(*col.get::<i32>(1).unwrap(), 20);
    }

    #[test]
    fn test_get_column() {
        let mut arch = Archetype::new(ArchetypeId(1));
        arch.push_component(42i32);
        arch.push_entity(eid(0));

        let col = arch.get_column(TypeId::of::<i32>()).unwrap();
        assert_eq!(*col.get::<i32>(0).unwrap(), 42);
        assert!(arch.get_column(TypeId::of::<u64>()).is_none());
    }

    #[test]
    fn test_multiple_component_types() {
        let mut arch = Archetype::new(ArchetypeId(1));

        arch.push_component(100i32);
        arch.push_component(1.5f32);
        arch.push_entity(eid(0));

        assert!(arch.has_component(TypeId::of::<i32>()));
        assert!(arch.has_component(TypeId::of::<f32>()));

        let i32_col = arch.get_column(TypeId::of::<i32>()).unwrap();
        let f32_col = arch.get_column(TypeId::of::<f32>()).unwrap();
        assert_eq!(*i32_col.get::<i32>(0).unwrap(), 100);
        assert_eq!(*f32_col.get::<f32>(0).unwrap(), 1.5);

        arch.remove_entity_swap(0);
        assert!(arch.is_empty());
    }

    #[test]
    fn test_iter_1() {
        let mut arch = Archetype::new(ArchetypeId(1));
        arch.push_component(10i32);
        arch.push_entity(eid(0));
        arch.push_component(20i32);
        arch.push_entity(eid(1));
        arch.push_component(30i32);
        arch.push_entity(eid(2));

        let results: Vec<_> = arch.iter_1::<i32>().unwrap().collect();
        assert_eq!(
            results,
            &[(eid(0), &10i32), (eid(1), &20i32), (eid(2), &30i32),]
        );
    }

    #[test]
    fn test_iter_1_missing_column() {
        let mut arch = Archetype::new(ArchetypeId(1));
        arch.push_component(10i32);
        arch.push_entity(eid(0));

        assert!(arch.iter_1::<f32>().is_none());
    }

    #[test]
    fn test_iter_2() {
        let mut arch = Archetype::new(ArchetypeId(1));
        arch.push_component(10i32);
        arch.push_component(1.0f32);
        arch.push_entity(eid(0));
        arch.push_component(20i32);
        arch.push_component(2.0f32);
        arch.push_entity(eid(1));

        let results: Vec<_> = arch.iter_2::<i32, f32>().unwrap().collect();
        assert_eq!(
            results,
            &[(eid(0), &10i32, &1.0f32), (eid(1), &20i32, &2.0f32),]
        );
    }

    #[test]
    fn test_iter_2_missing_column() {
        let mut arch = Archetype::new(ArchetypeId(1));
        arch.push_component(10i32);
        arch.push_entity(eid(0));

        assert!(arch.iter_2::<i32, f32>().is_none());
    }

    #[test]
    fn test_column_slice() {
        let mut arch = Archetype::new(ArchetypeId(1));
        arch.push_component(10i32);
        arch.push_entity(eid(0));
        arch.push_component(20i32);
        arch.push_entity(eid(1));

        let slice = arch.column_slice::<i32>().unwrap();
        assert_eq!(slice, &[10, 20]);
        assert!(arch.column_slice::<f32>().is_none());
    }

    #[test]
    fn test_column_slice_mut() {
        let mut arch = Archetype::new(ArchetypeId(1));
        arch.push_component(10i32);
        arch.push_entity(eid(0));

        let slice = arch.column_slice_mut::<i32>().unwrap();
        slice[0] = 99;
        assert_eq!(arch.column_slice::<i32>().unwrap(), &[99]);
    }
}
