//! Component storage for ECS (Entity Component System).
//!
//! This module provides storage for components with O(1) lookup, insert, and remove operations
//! while maintaining contiguous storage for fast iteration.

use crate::components::Component;
use crate::entity::EntityId;
use crate::query::QueryData;
use crate::sparse_set::SparseSet;
use std::any::Any;

/// Storage for components of a specific type.
///
/// Uses a sparse set internally for O(1) lookups while maintaining contiguous
/// storage for fast iteration over all components of a given type.
pub struct ComponentStorage<T: Component> {
    /// Internal sparse set for O(1) lookups
    storage: SparseSet<EntityId, T>,
}

impl<T: Component> ComponentStorage<T> {
    /// Creates a new empty ComponentStorage.
    pub fn new() -> Self {
        Self {
            storage: SparseSet::new(),
        }
    }

    /// Adds a component for the given entity.
    ///
    /// If the entity already has this component type, it will be replaced.
    pub fn insert(&mut self, entity_id: EntityId, component: T) {
        self.storage.insert(entity_id, component);
    }

    /// Removes a component for the given entity.
    ///
    /// Returns true if the component was removed, false if it didn't exist.
    pub fn remove(&mut self, entity_id: EntityId) -> bool {
        self.storage.remove(entity_id)
    }

    /// Gets a reference to a component for the given entity.
    pub fn get(&self, entity_id: EntityId) -> Option<&T> {
        self.storage.get(entity_id)
    }

    /// Gets a mutable reference to a component for the given entity.
    pub fn get_mut(&mut self, entity_id: EntityId) -> Option<&mut T> {
        self.storage.get_mut(entity_id)
    }

    /// Returns true if the entity has this component.
    pub fn contains(&self, entity_id: EntityId) -> bool {
        self.storage.contains(entity_id)
    }

    /// Returns an iterator over all (EntityId, &Component) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.storage.iter()
    }

    /// Returns a mutable iterator over all (EntityId, &mut Component) pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.storage.iter_mut()
    }

    /// Returns an iterator over just the components.
    pub fn components(&self) -> impl Iterator<Item = &T> {
        self.storage.values()
    }

    /// Returns a mutable iterator over just the components.
    pub fn components_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.storage.values_mut()
    }

    /// Returns a reference to the internal component storage (for query module).
    pub(crate) fn components_vec(&self) -> &Vec<(EntityId, T)> {
        self.storage.dense()
    }

    /// Returns a mutable reference to the internal component storage (for query module).
    pub(crate) fn components_vec_mut(&mut self) -> &mut Vec<(EntityId, T)> {
        self.storage.dense_mut()
    }

    /// Returns an iterator over entity IDs that have this component.
    pub fn entity_ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.storage.keys()
    }

    /// Returns the number of components stored.
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Returns true if no components are stored.
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    /// Clears all components.
    pub fn clear(&mut self) {
        self.storage.clear();
    }

    /// Removes all components for entities not in the given set.
    pub fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        self.storage.retain_keys(valid_entities);
    }
}

impl<T: Component> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for type-erased component storage operations.
pub trait AnyComponentStorage: Any {
    /// Removes a component for the given entity.
    fn remove_entity(&mut self, entity_id: EntityId);

    /// Returns true if the entity has a component in this storage.
    fn contains_entity(&self, entity_id: EntityId) -> bool;

    /// Returns the number of components stored.
    fn len(&self) -> usize;

    /// Returns true if no components are stored.
    fn is_empty(&self) -> bool;

    /// Clears all components.
    fn clear(&mut self);

    /// Removes all components for entities not in the given set.
    fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>);

    /// Returns an iterator over entity IDs that have this component.
    fn entity_ids(&self) -> Box<dyn Iterator<Item = EntityId> + '_>;

    /// Returns a reference to self as `Any` for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Returns a mutable reference to self as `Any` for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Component> AnyComponentStorage for ComponentStorage<T> {
    fn remove_entity(&mut self, entity_id: EntityId) {
        self.remove(entity_id);
    }

    fn contains_entity(&self, entity_id: EntityId) -> bool {
        self.contains(entity_id)
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn clear(&mut self) {
        self.clear();
    }

    fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        self.retain_entities(valid_entities);
    }

    fn entity_ids(&self) -> Box<dyn Iterator<Item = EntityId> + '_> {
        Box::new(self.entity_ids())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Manages component storages for different component types.
///
/// Uses type erasure via `AnyComponentStorage` to store heterogeneous component
/// storages in a single collection, indexed by component type ID.
pub struct ComponentStorageManager {
    /// Maps type IDs to component storages
    storages: std::collections::HashMap<std::any::TypeId, Box<dyn AnyComponentStorage>>,
}

impl ComponentStorageManager {
    /// Creates a new empty ComponentStorageManager.
    pub fn new() -> Self {
        Self {
            storages: std::collections::HashMap::new(),
        }
    }

    /// Gets or creates a component storage for the given component type.
    fn get_or_create_storage<T: Component>(&mut self) -> &mut ComponentStorage<T> {
        let type_id = std::any::TypeId::of::<T>();
        let storages = &mut self.storages;

        if !storages.contains_key(&type_id) {
            storages.insert(type_id, Box::new(ComponentStorage::<T>::new()));
        }

        storages
            .get_mut(&type_id)
            .expect("Storage should exist after insertion")
            .as_any_mut()
            .downcast_mut::<ComponentStorage<T>>()
            .expect("Downcast should succeed")
    }

    /// Gets a reference to the component storage for the given component type.
    pub fn get_storage<T: Component>(&self) -> Option<&ComponentStorage<T>> {
        self.storages
            .get(&std::any::TypeId::of::<T>())
            .map(|storage| {
                storage
                    .as_any()
                    .downcast_ref::<ComponentStorage<T>>()
                    .expect("Downcast should succeed")
            })
    }

    /// Gets a mutable reference to the component storage for the given component type.
    pub fn get_storage_mut<T: Component>(&mut self) -> Option<&mut ComponentStorage<T>> {
        let type_id = std::any::TypeId::of::<T>();
        self.storages.get_mut(&type_id).map(|storage| {
            storage
                .as_any_mut()
                .downcast_mut::<ComponentStorage<T>>()
                .expect("Downcast should succeed")
        })
    }

    /// Adds a component for the given entity.
    pub fn add_component<T: Component>(&mut self, entity_id: EntityId, component: T) {
        self.get_or_create_storage::<T>()
            .insert(entity_id, component);
    }

    /// Removes a component for the given entity.
    ///
    /// Returns true if the component was removed, false if it didn't exist.
    pub fn remove_component<T: Component>(&mut self, entity_id: EntityId) -> bool {
        if let Some(storage) = self.get_storage_mut::<T>() {
            storage.remove(entity_id)
        } else {
            false
        }
    }

    /// Gets a reference to a component for the given entity.
    pub fn get_component<T: Component>(&self, entity_id: EntityId) -> Option<&T> {
        self.get_storage::<T>()
            .and_then(|storage| storage.get(entity_id))
    }

    /// Gets a mutable reference to a component for the given entity.
    ///
    /// # Panics
    /// Panics if there's a mutable borrow conflict, typically when trying to borrow
    /// the same component type mutably more than once in a query.
    pub fn get_component_mut<T: Component>(&mut self, entity_id: EntityId) -> Option<&mut T> {
        self.get_storage_mut::<T>()
            .and_then(|storage| storage.get_mut(entity_id))
    }

    /// Removes all components for the given entity across all component types.
    pub fn remove_entity(&mut self, entity_id: EntityId) {
        for storage in self.storages.values_mut() {
            storage.remove_entity(entity_id);
        }
    }

    /// Removes all components for entities not in the given set.
    pub fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        for storage in self.storages.values_mut() {
            storage.retain_entities(valid_entities);
        }
    }

    /// Clears all components from all storages.
    pub fn clear(&mut self) {
        for storage in self.storages.values_mut() {
            storage.clear();
        }
    }

    /// Returns the number of component types stored.
    pub fn storage_count(&self) -> usize {
        self.storages.len()
    }

    /// Creates a query for iterating over entities with specific components.
    ///
    /// See the [`query`](crate::query) module for detailed documentation and examples.
    ///
    /// # Example
    /// ```ignore
    /// // Query with mutable and immutable access
    /// for (entity, velocity, force) in storage.query::<(&mut VelocityComponent, &ForceComponent)>() {
    ///     velocity.acceleration = force.force;
    /// }
    /// ```
    pub fn query<Q: QueryData>(&mut self) -> Q::Iter<'_> {
        Q::fetch(self)
    }
}

impl Default for ComponentStorageManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Component;

    #[derive(Component, Clone, Debug, PartialEq)]
    struct TestComponent {
        value: i32,
    }

    #[test]
    fn test_component_storage_insert() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::new(0);

        storage.insert(entity, TestComponent { value: 42 });

        assert_eq!(storage.len(), 1);
        assert!(storage.contains(entity));
    }

    #[test]
    fn test_component_storage_get() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::new(0);

        storage.insert(entity, TestComponent { value: 42 });

        let component = storage.get(entity).unwrap();
        assert_eq!(component.value, 42);
    }

    #[test]
    fn test_component_storage_remove() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::new(0);

        storage.insert(entity, TestComponent { value: 42 });
        assert!(storage.remove(entity));
        assert!(!storage.contains(entity));
        assert_eq!(storage.len(), 0);
    }

    #[test]
    fn test_component_storage_replace() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::new(0);

        storage.insert(entity, TestComponent { value: 42 });
        storage.insert(entity, TestComponent { value: 100 });

        let component = storage.get(entity).unwrap();
        assert_eq!(component.value, 100);
        assert_eq!(storage.len(), 1);
    }

    #[test]
    fn test_component_storage_iter() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        storage.insert(entity1, TestComponent { value: 42 });
        storage.insert(entity2, TestComponent { value: 100 });

        let components: Vec<_> = storage.iter().collect();
        assert_eq!(components.len(), 2);
        assert!(components.contains(&(entity1, &TestComponent { value: 42 })));
        assert!(components.contains(&(entity2, &TestComponent { value: 100 })));
    }

    #[test]
    fn test_storage_manager() {
        let mut manager = ComponentStorageManager::new();
        let entity = EntityId::new(0);

        manager.add_component(entity, TestComponent { value: 42 });

        assert_eq!(manager.storage_count(), 1);

        let component = manager.get_component::<TestComponent>(entity).unwrap();
        assert_eq!(component.value, 42);
    }

    #[test]
    fn test_storage_manager_remove_entity() {
        let mut manager = ComponentStorageManager::new();
        let entity = EntityId::new(0);

        manager.add_component(entity, TestComponent { value: 42 });
        manager.remove_entity(entity);

        assert!(manager.get_component::<TestComponent>(entity).is_none());
    }

    #[derive(Component, Clone, Debug, PartialEq)]
    struct TestComponent2 {
        value: f32,
    }

    #[derive(Component, Clone, Debug, PartialEq)]
    struct TestComponent3 {
        name: String,
    }

    #[test]
    fn test_query_single_mutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity2, TestComponent { value: 20 });

        let results: Vec<EntityId> = manager
            .query::<&mut TestComponent>()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&entity1));
        assert!(results.contains(&entity2));
    }

    #[test]
    fn test_query_single_immutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity2, TestComponent { value: 20 });

        let results: Vec<EntityId> = manager
            .query::<&TestComponent>()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&entity1));
        assert!(results.contains(&entity2));
    }

    #[test]
    fn test_query_two_mutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity1, TestComponent2 { value: 1.5 });
        manager.add_component(entity2, TestComponent { value: 20 });
        manager.add_component(entity2, TestComponent2 { value: 2.5 });

        let results: Vec<EntityId> = manager
            .query::<(&mut TestComponent, &mut TestComponent2)>()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&entity1));
        assert!(results.contains(&entity2));
    }

    #[test]
    fn test_query_mutable_and_immutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity1, TestComponent2 { value: 1.5 });
        manager.add_component(entity2, TestComponent { value: 20 });
        manager.add_component(entity2, TestComponent2 { value: 2.5 });

        let results: Vec<EntityId> = manager
            .query::<(&mut TestComponent, &TestComponent2)>()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&entity1));
        assert!(results.contains(&entity2));
    }

    #[test]
    fn test_query_immutable_and_mutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity1, TestComponent2 { value: 1.5 });
        manager.add_component(entity2, TestComponent { value: 20 });
        manager.add_component(entity2, TestComponent2 { value: 2.5 });

        let results: Vec<EntityId> = manager
            .query::<(&TestComponent, &mut TestComponent2)>()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&entity1));
        assert!(results.contains(&entity2));
    }

    #[test]
    fn test_query_empty_storage() {
        let mut manager = ComponentStorageManager::new();

        let results: Vec<EntityId> = manager
            .query::<&TestComponent>()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_partial_components() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);
        let entity3 = EntityId::new(2);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity1, TestComponent2 { value: 1.5 });
        manager.add_component(entity2, TestComponent { value: 20 });
        manager.add_component(entity3, TestComponent2 { value: 2.5 });

        let results: Vec<EntityId> = manager
            .query::<(&TestComponent, &TestComponent2)>()
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(results.len(), 1);
        assert!(results.contains(&entity1));
    }

    #[test]
    #[should_panic]
    fn test_query_same_type_twice_panics() {
        let mut manager = ComponentStorageManager::new();
        let _: Vec<EntityId> = manager
            .query::<(&TestComponent, &mut TestComponent)>()
            .map(|(id, _, _)| id)
            .collect();
    }

    #[test]
    fn test_query_three_components_all_immutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity1, TestComponent2 { value: 1.5 });
        manager.add_component(
            entity1,
            TestComponent3 {
                name: "Entity 1".to_string(),
            },
        );
        manager.add_component(entity2, TestComponent { value: 20 });
        manager.add_component(entity2, TestComponent2 { value: 2.5 });
        manager.add_component(
            entity2,
            TestComponent3 {
                name: "Entity 2".to_string(),
            },
        );

        let results: Vec<EntityId> = manager
            .query::<(&TestComponent, &TestComponent2, &TestComponent3)>()
            .map(|(id, _, _, _)| id)
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&entity1));
        assert!(results.contains(&entity2));
    }

    #[test]
    fn test_query_three_components_one_mutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity1, TestComponent2 { value: 1.5 });
        manager.add_component(
            entity1,
            TestComponent3 {
                name: "Entity 1".to_string(),
            },
        );
        manager.add_component(entity2, TestComponent { value: 20 });
        manager.add_component(entity2, TestComponent2 { value: 2.5 });
        manager.add_component(
            entity2,
            TestComponent3 {
                name: "Entity 2".to_string(),
            },
        );

        let results: Vec<EntityId> = manager
            .query::<(&TestComponent, &mut TestComponent2, &TestComponent3)>()
            .map(|(id, _, _, _)| id)
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&entity1));
        assert!(results.contains(&entity2));
    }

    #[test]
    fn test_query_three_components_mutable_at_end() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(0);
        let entity2 = EntityId::new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity1, TestComponent2 { value: 1.5 });
        manager.add_component(
            entity1,
            TestComponent3 {
                name: "Entity 1".to_string(),
            },
        );
        manager.add_component(entity2, TestComponent { value: 20 });
        manager.add_component(entity2, TestComponent2 { value: 2.5 });
        manager.add_component(
            entity2,
            TestComponent3 {
                name: "Entity 2".to_string(),
            },
        );

        let results: Vec<EntityId> = manager
            .query::<(&TestComponent, &TestComponent2, &mut TestComponent3)>()
            .map(|(id, _, _, _)| id)
            .collect();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&entity1));
        assert!(results.contains(&entity2));
    }

    #[test]
    #[should_panic]
    fn test_query_three_same_type_panics() {
        let mut manager = ComponentStorageManager::new();
        let _: Vec<EntityId> = manager
            .query::<(&TestComponent, &TestComponent, &TestComponent)>()
            .map(|(id, _, _, _)| id)
            .collect();
    }
}
