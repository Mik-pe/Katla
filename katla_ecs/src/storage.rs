//! Component storage implementation for the ECS framework.
//!
//! This module provides efficient storage for components using a struct-of-arrays approach.
//! Components of the same type are stored together in contiguous memory for better cache locality.
//!
//! For query-based access to components, see the [`query`](crate::query) module.

use super::components::Component;
use super::entity::EntityId;
use crate::query::QueryData;
use std::any::{Any, TypeId};
use std::collections::HashMap;

/// ComponentStorage stores components of a specific type in a vector.
///
/// Each component is associated with an EntityId. This provides better cache
/// locality than storing components in a HashMap per entity.
pub struct ComponentStorage<T: Component> {
    /// Vector of (EntityId, Component) pairs
    pub(crate) components: Vec<(EntityId, T)>,
}

impl<T: Component> ComponentStorage<T> {
    /// Creates a new empty ComponentStorage.
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// Adds a component for the given entity.
    ///
    /// If the entity already has this component type, it will be replaced.
    pub fn insert(&mut self, entity_id: EntityId, component: T) {
        // Check if entity already has this component
        if let Some(pos) = self.components.iter().position(|(id, _)| *id == entity_id) {
            self.components[pos].1 = component;
        } else {
            self.components.push((entity_id, component));
        }
    }

    /// Removes a component for the given entity.
    ///
    /// Returns true if the component was removed, false if it didn't exist.
    pub fn remove(&mut self, entity_id: EntityId) -> bool {
        if let Some(pos) = self.components.iter().position(|(id, _)| *id == entity_id) {
            self.components.swap_remove(pos);
            true
        } else {
            false
        }
    }

    /// Gets a reference to a component for the given entity.
    pub fn get(&self, entity_id: EntityId) -> Option<&T> {
        self.components
            .iter()
            .find(|(id, _)| *id == entity_id)
            .map(|(_, component)| component)
    }

    /// Gets a mutable reference to a component for the given entity.
    pub fn get_mut(&mut self, entity_id: EntityId) -> Option<&mut T> {
        self.components
            .iter_mut()
            .find(|(id, _)| *id == entity_id)
            .map(|(_, component)| component)
    }

    /// Returns true if the entity has this component.
    pub fn contains(&self, entity_id: EntityId) -> bool {
        self.components.iter().any(|(id, _)| *id == entity_id)
    }

    /// Returns an iterator over all (EntityId, &Component) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.components.iter().map(|(id, comp)| (*id, comp))
    }

    /// Returns a mutable iterator over all (EntityId, &mut Component) pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.components.iter_mut().map(|(id, comp)| (*id, comp))
    }

    /// Returns an iterator over just the components.
    pub fn components(&self) -> impl Iterator<Item = &T> {
        self.components.iter().map(|(_, comp)| comp)
    }

    /// Returns a mutable iterator over just the components.
    pub fn components_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.components.iter_mut().map(|(_, comp)| comp)
    }

    /// Returns an iterator over entity IDs that have this component.
    pub fn entity_ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.components.iter().map(|(id, _)| *id)
    }

    /// Returns the number of components stored.
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Returns true if no components are stored.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Clears all components.
    pub fn clear(&mut self) {
        self.components.clear();
    }

    /// Removes all components for entities not in the given set.
    pub fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        self.components
            .retain(|(id, _)| valid_entities.contains(id));
    }
}

impl<T: Component> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Type-erased component storage that can hold any component type.
///
/// This allows the World to store different component types in a single HashMap.
pub trait AnyComponentStorage: Any {
    /// Removes a component for the given entity.
    fn remove_entity(&mut self, entity_id: EntityId) -> bool;

    /// Checks if the entity has a component in this storage.
    fn contains_entity(&self, entity_id: EntityId) -> bool;

    /// Returns the number of components stored.
    fn len(&self) -> usize;

    /// Returns true if no components are stored.
    fn is_empty(&self) -> bool;

    /// Clears all components.
    fn clear(&mut self);

    /// Removes all components for entities not in the given set.
    fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>);

    /// Returns an iterator over entity IDs that have components in this storage.
    fn entity_ids(&self) -> Box<dyn Iterator<Item = EntityId> + '_>;

    /// Converts to Any for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Converts to mutable Any for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Component + 'static> AnyComponentStorage for ComponentStorage<T> {
    fn remove_entity(&mut self, entity_id: EntityId) -> bool {
        self.remove(entity_id)
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
        self.clear()
    }

    fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        self.retain_entities(valid_entities)
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

/// Manager for all component storages in the World.
///
/// Maintains a HashMap of TypeId -> ComponentStorage for efficient
/// component access.
pub struct ComponentStorageManager {
    storages: HashMap<TypeId, Box<dyn AnyComponentStorage>>,
}

impl ComponentStorageManager {
    /// Creates a new empty ComponentStorageManager.
    pub fn new() -> Self {
        Self {
            storages: HashMap::new(),
        }
    }

    /// Gets or creates a storage for the given component type.
    fn get_or_create_storage<T: Component + 'static>(&mut self) -> &mut ComponentStorage<T> {
        let type_id = TypeId::of::<T>();
        self.storages
            .entry(type_id)
            .or_insert_with(|| Box::new(ComponentStorage::<T>::new()))
            .as_any_mut()
            .downcast_mut::<ComponentStorage<T>>()
            .expect("Type mismatch in component storage")
    }

    /// Gets a storage for the given component type.
    pub fn get_storage<T: Component + 'static>(&self) -> Option<&ComponentStorage<T>> {
        let type_id = TypeId::of::<T>();
        self.storages
            .get(&type_id)
            .and_then(|storage| storage.as_any().downcast_ref::<ComponentStorage<T>>())
    }

    /// Gets a mutable storage for the given component type.
    pub fn get_storage_mut<T: Component + 'static>(&mut self) -> Option<&mut ComponentStorage<T>> {
        let type_id = TypeId::of::<T>();
        self.storages
            .get_mut(&type_id)
            .and_then(|storage| storage.as_any_mut().downcast_mut::<ComponentStorage<T>>())
    }

    /// Adds a component to an entity.
    pub fn add_component<T: Component + 'static>(&mut self, entity_id: EntityId, component: T) {
        let storage = self.get_or_create_storage::<T>();
        storage.insert(entity_id, component);
    }

    /// Removes a component from an entity.
    pub fn remove_component<T: Component + 'static>(&mut self, entity_id: EntityId) -> bool {
        if let Some(storage) = self.get_storage_mut::<T>() {
            storage.remove(entity_id)
        } else {
            false
        }
    }

    /// Gets a reference to a component for a specific entity.
    ///
    /// Use this for accessing individual entities by ID. For iterating over multiple entities
    /// with components, prefer using queries:
    ///
    /// ```ignore
    /// // Prefer queries for iteration:
    /// for (entity, transform) in storage.query::<&TransformComponent>() {
    ///     // ...
    /// }
    ///
    /// // Use get_component for specific entity access:
    /// if let Some(transform) = storage.get_component::<TransformComponent>(specific_entity) {
    ///     // ...
    /// }
    /// ```
    pub fn get_component<T: Component + 'static>(&self, entity_id: EntityId) -> Option<&T> {
        self.get_storage::<T>()
            .and_then(|storage| storage.get(entity_id))
    }

    /// Gets a mutable reference to a component for a specific entity.
    ///
    /// Use this for accessing individual entities by ID. For iterating over multiple entities
    /// with components, prefer using queries. See [`get_component`](Self::get_component) for details.
    pub fn get_component_mut<T: Component + 'static>(
        &mut self,
        entity_id: EntityId,
    ) -> Option<&mut T> {
        self.get_storage_mut::<T>()
            .and_then(|storage| storage.get_mut(entity_id))
    }

    /// Removes all components for the given entity.
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

    /// Clears all component storages.
    pub fn clear(&mut self) {
        self.storages.clear();
    }

    /// Returns the number of component storages.
    pub fn storage_count(&self) -> usize {
        self.storages.len()
    }

    /// Creates a query for iterating over entities with specific components.
    ///
    /// See the [`query`](crate::query) module for detailed documentation and examples.
    ///
    /// # Example
    ///
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
    use crate::Component;

    #[derive(Component, Default)]
    struct TestComponent {}

    #[test]
    fn test_component_storage_insert() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::new(1);

        storage.insert(entity, TestComponent::default());
        assert_eq!(storage.len(), 1);
        assert!(storage.contains(entity));
    }

    #[test]
    fn test_component_storage_get() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::new(1);

        storage.insert(entity, TestComponent::default());
        assert!(storage.get(entity).is_some());
        assert!(storage.get(EntityId::new(2)).is_none());
    }

    #[test]
    fn test_component_storage_remove() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::new(1);

        storage.insert(entity, TestComponent::default());
        assert!(storage.remove(entity));
        assert!(!storage.contains(entity));
        assert!(!storage.remove(entity));
    }

    #[test]
    fn test_component_storage_replace() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::new(1);

        storage.insert(entity, TestComponent::default());
        storage.insert(entity, TestComponent::default());
        assert_eq!(storage.len(), 1);
    }

    #[test]
    fn test_component_storage_iter() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        storage.insert(EntityId::new(1), TestComponent::default());
        storage.insert(EntityId::new(2), TestComponent::default());
        storage.insert(EntityId::new(3), TestComponent::default());

        let count = storage.iter().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_storage_manager() {
        let mut manager = ComponentStorageManager::new();
        let entity = EntityId::new(1);

        manager.add_component(entity, TestComponent::default());
        assert!(manager.get_component::<TestComponent>(entity).is_some());
    }

    #[test]
    fn test_storage_manager_remove_entity() {
        let mut manager = ComponentStorageManager::new();
        let entity = EntityId::new(1);

        manager.add_component(entity, TestComponent::default());
        manager.remove_entity(entity);
        assert!(manager.get_component::<TestComponent>(entity).is_none());
    }

    #[derive(Component, Default)]
    struct TestComponent2 {
        value: i32,
    }

    #[derive(Component, Default)]
    struct TestComponent3 {
        #[allow(dead_code)]
        name: String,
    }

    #[test]
    fn test_query_single_mutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);
        let entity2 = EntityId::new(2);

        manager.add_component(entity1, TestComponent2 { value: 10 });
        manager.add_component(entity2, TestComponent2 { value: 20 });

        let mut results = Vec::new();
        for (entity, component) in manager.query::<&mut TestComponent2>() {
            component.value += 5;
            results.push((entity, component.value));
        }

        assert_eq!(results.len(), 2);
        assert!(results.contains(&(entity1, 15)));
        assert!(results.contains(&(entity2, 25)));
    }

    #[test]
    fn test_query_single_immutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);
        let entity2 = EntityId::new(2);

        manager.add_component(entity1, TestComponent2 { value: 10 });
        manager.add_component(entity2, TestComponent2 { value: 20 });

        let mut results = Vec::new();
        for (entity, component) in manager.query::<&TestComponent2>() {
            results.push((entity, component.value));
        }

        assert_eq!(results.len(), 2);
        assert!(results.contains(&(entity1, 10)));
        assert!(results.contains(&(entity2, 20)));
    }

    #[test]
    fn test_query_two_mutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);
        let entity2 = EntityId::new(2);
        let entity3 = EntityId::new(3);

        manager.add_component(entity1, TestComponent::default());
        manager.add_component(entity1, TestComponent2 { value: 10 });

        manager.add_component(entity2, TestComponent::default());
        manager.add_component(entity2, TestComponent2 { value: 20 });

        manager.add_component(entity3, TestComponent::default());
        // entity3 doesn't have TestComponent2

        let mut count = 0;
        for (_entity, _comp1, comp2) in manager.query::<(&TestComponent, &mut TestComponent2)>() {
            comp2.value *= 2;
            count += 1;
        }

        assert_eq!(count, 2); // Only entity1 and entity2 have both components
        assert_eq!(
            manager
                .get_component::<TestComponent2>(entity1)
                .unwrap()
                .value,
            20
        );
        assert_eq!(
            manager
                .get_component::<TestComponent2>(entity2)
                .unwrap()
                .value,
            40
        );
    }

    #[test]
    fn test_query_mutable_and_immutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);
        let entity2 = EntityId::new(2);

        manager.add_component(entity1, TestComponent::default());
        manager.add_component(entity1, TestComponent2 { value: 10 });

        manager.add_component(entity2, TestComponent::default());
        manager.add_component(entity2, TestComponent2 { value: 20 });

        let mut sum = 0;
        for (_entity, _comp1, comp2) in manager.query::<(&mut TestComponent, &TestComponent2)>() {
            sum += comp2.value;
        }

        assert_eq!(sum, 30);
    }

    #[test]
    fn test_query_immutable_and_mutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);
        let entity2 = EntityId::new(2);

        manager.add_component(entity1, TestComponent::default());
        manager.add_component(entity1, TestComponent2 { value: 10 });

        manager.add_component(entity2, TestComponent::default());
        manager.add_component(entity2, TestComponent2 { value: 20 });

        for (_entity, _comp1, comp2) in manager.query::<(&TestComponent, &mut TestComponent2)>() {
            comp2.value += 1;
        }

        assert_eq!(
            manager
                .get_component::<TestComponent2>(entity1)
                .unwrap()
                .value,
            11
        );
        assert_eq!(
            manager
                .get_component::<TestComponent2>(entity2)
                .unwrap()
                .value,
            21
        );
    }

    #[test]
    fn test_query_empty_storage() {
        let mut manager = ComponentStorageManager::new();

        let mut count = 0;
        for _ in manager.query::<&mut TestComponent>() {
            count += 1;
        }

        assert_eq!(count, 0);
    }

    #[test]
    fn test_query_partial_components() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);
        let entity2 = EntityId::new(2);
        let entity3 = EntityId::new(3);

        // entity1 has both components
        manager.add_component(entity1, TestComponent::default());
        manager.add_component(entity1, TestComponent2 { value: 10 });

        // entity2 has only TestComponent
        manager.add_component(entity2, TestComponent::default());

        // entity3 has only TestComponent2
        manager.add_component(entity3, TestComponent2 { value: 30 });

        let mut results = Vec::new();
        for (entity, _comp1, _comp2) in manager.query::<(&TestComponent, &TestComponent2)>() {
            results.push(entity);
        }

        // Only entity1 should be returned
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], entity1);
    }

    #[test]
    #[should_panic(expected = "Cannot query the same component type twice")]
    fn test_query_same_type_twice_panics() {
        let mut manager = ComponentStorageManager::new();
        let _ = manager.query::<(&mut TestComponent, &mut TestComponent)>();
    }

    #[test]
    fn test_query_three_components_all_immutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);
        let entity2 = EntityId::new(2);

        manager.add_component(entity1, TestComponent::default());
        manager.add_component(entity1, TestComponent2 { value: 10 });
        manager.add_component(
            entity1,
            TestComponent3 {
                name: String::from("test1"),
            },
        );

        manager.add_component(entity2, TestComponent::default());
        manager.add_component(entity2, TestComponent2 { value: 20 });
        // entity2 missing TestComponent3

        let mut count = 0;
        for (entity, _c1, c2, _c3) in
            manager.query::<(&TestComponent, &TestComponent2, &TestComponent3)>()
        {
            count += 1;
            assert_eq!(entity, entity1);
            assert_eq!(c2.value, 10);
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn test_query_three_components_one_mutable() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);

        manager.add_component(entity1, TestComponent::default());
        manager.add_component(entity1, TestComponent2 { value: 10 });
        manager.add_component(
            entity1,
            TestComponent3 {
                name: String::from("test"),
            },
        );

        for (_entity, _c1, c2, _c3) in
            manager.query::<(&TestComponent, &mut TestComponent2, &TestComponent3)>()
        {
            c2.value += 5;
        }

        assert_eq!(
            manager
                .get_component::<TestComponent2>(entity1)
                .unwrap()
                .value,
            15
        );
    }

    #[test]
    fn test_query_three_components_mutable_at_end() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::new(1);

        manager.add_component(entity1, TestComponent::default());
        manager.add_component(entity1, TestComponent2 { value: 10 });
        manager.add_component(
            entity1,
            TestComponent3 {
                name: String::from("test"),
            },
        );

        for (_entity, _c1, _c2, c3) in
            manager.query::<(&TestComponent, &TestComponent2, &mut TestComponent3)>()
        {
            c3.name.push_str("_modified");
        }

        assert_eq!(
            manager
                .get_component::<TestComponent3>(entity1)
                .unwrap()
                .name,
            "test_modified"
        );
    }

    #[test]
    #[should_panic(expected = "Cannot query the same component type twice")]
    fn test_query_three_same_type_panics() {
        let mut manager = ComponentStorageManager::new();
        let _ = manager.query::<(&TestComponent, &TestComponent2, &TestComponent)>();
    }
}
