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
}
