use super::components::Component;
use super::entity::EntityId;
use std::any::{Any, TypeId};
use std::collections::HashMap;

/// ComponentStorage stores components of a specific type in a vector.
///
/// Each component is associated with an EntityId. This provides better cache
/// locality than storing components in a HashMap per entity.
pub struct ComponentStorage<T: Component> {
    /// Vector of (EntityId, Component) pairs
    components: Vec<(EntityId, T)>,
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

    /// Gets a reference to a component for an entity.
    pub fn get_component<T: Component + 'static>(&self, entity_id: EntityId) -> Option<&T> {
        self.get_storage::<T>()
            .and_then(|storage| storage.get(entity_id))
    }

    /// Gets a mutable reference to a component for an entity.
    pub fn get_component_mut<T: Component + 'static>(
        &mut self,
        entity_id: EntityId,
    ) -> Option<&mut T> {
        self.get_storage_mut::<T>()
            .and_then(|storage| storage.get_mut(entity_id))
    }

    /// Checks if an entity has a specific component.
    pub fn has_component<T: Component + 'static>(&self, entity_id: EntityId) -> bool {
        self.get_storage::<T>()
            .map(|storage| storage.contains(entity_id))
            .unwrap_or(false)
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
        assert!(manager.has_component::<TestComponent>(entity));
        assert!(manager.get_component::<TestComponent>(entity).is_some());
    }

    #[test]
    fn test_storage_manager_remove_entity() {
        let mut manager = ComponentStorageManager::new();
        let entity = EntityId::new(1);

        manager.add_component(entity, TestComponent::default());
        manager.remove_entity(entity);
        assert!(!manager.has_component::<TestComponent>(entity));
    }
}
