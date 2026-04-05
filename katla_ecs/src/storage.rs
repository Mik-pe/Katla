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
    /// Per-entity generation counters for change detection.
    /// Incremented on insert and get_mut. Compared against a "last seen" generation
    /// to determine if an entity's component has changed.
    generations: SparseSet<EntityId, u64>,
}

impl<T: Component> ComponentStorage<T> {
    /// Creates a new empty ComponentStorage.
    pub fn new() -> Self {
        Self {
            storage: SparseSet::new(),
            generations: SparseSet::new(),
        }
    }

    /// Adds a component for the given entity.
    ///
    /// If the entity already has this component type, it will be replaced.
    /// Marks the entity as changed for change detection.
    pub fn insert(&mut self, entity_id: EntityId, component: T) {
        self.storage.insert(entity_id, component);
        self.increment_generation(entity_id);
    }

    /// Removes a component for the given entity.
    ///
    /// Returns true if the component was removed, false if it didn't exist.
    pub fn remove(&mut self, entity_id: EntityId) -> bool {
        let removed = self.storage.remove(entity_id);
        if removed {
            self.generations.remove(entity_id);
        }
        removed
    }

    /// Gets a reference to a component for the given entity.
    pub fn get(&self, entity_id: EntityId) -> Option<&T> {
        self.storage.get(entity_id)
    }

    /// Gets a mutable reference to a component for the given entity.
    ///
    /// Marks the entity as changed for change detection, even if the
    /// component is not actually modified.
    pub fn get_mut(&mut self, entity_id: EntityId) -> Option<&mut T> {
        self.increment_generation(entity_id);
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

    /// Clears all components and generation tracking.
    pub fn clear(&mut self) {
        self.storage.clear();
        self.generations.clear();
    }

    /// Removes all components for entities not in the given set.
    pub fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        self.storage.retain_keys(valid_entities);
        self.generations.retain_keys(valid_entities);
    }

    /// Increments the generation counter for the given entity.
    /// Creates a new counter (starting at 1) if one doesn't exist.
    fn increment_generation(&mut self, entity_id: EntityId) {
        let current = self.generations.get(entity_id).copied().unwrap_or(0);
        self.generations.insert(entity_id, current + 1);
    }

    /// Returns the current generation counter for the given entity.
    pub(crate) fn generation(&self, entity_id: EntityId) -> u64 {
        self.generations.get(entity_id).copied().unwrap_or(0)
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

    /// Clears all components.
    fn clear(&mut self);

    /// Removes all components for entities not in the given set.
    fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>);

    /// Collects all entity IDs that have a component in this storage.
    fn collect_entity_ids(&self, out: &mut std::collections::HashSet<EntityId>);

    /// Returns the maximum generation counter across all entities in this storage.
    fn max_generation(&self) -> u64;

    /// Returns the generation counter for a specific entity.
    /// Returns 0 if the entity doesn't exist in this storage.
    fn generation_for_entity(&self, entity_id: EntityId) -> u64;

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

    fn clear(&mut self) {
        self.clear();
    }

    fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        self.retain_entities(valid_entities);
    }

    fn collect_entity_ids(&self, out: &mut std::collections::HashSet<EntityId>) {
        for (entity_id, _) in self.storage.iter() {
            out.insert(entity_id);
        }
    }

    fn max_generation(&self) -> u64 {
        self.generations.values().copied().max().unwrap_or(0)
    }

    fn generation_for_entity(&self, entity_id: EntityId) -> u64 {
        self.generation(entity_id)
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
    /// Per-type "last seen" generation snapshot for change detection.
    /// After `clear_changed()` is called, stores the generation counter value
    /// that was current at that point. Entities with generation > snapshot are "changed".
    changed_generations: std::collections::HashMap<std::any::TypeId, u64>,
}

impl ComponentStorageManager {
    /// Creates a new empty ComponentStorageManager.
    pub fn new() -> Self {
        Self {
            storages: std::collections::HashMap::new(),
            changed_generations: std::collections::HashMap::new(),
        }
    }

    fn get_or_create_storage<T: Component>(&mut self) -> &mut ComponentStorage<T> {
        let type_id = std::any::TypeId::of::<T>();
        let storages = &mut self.storages;

        storages
            .entry(type_id)
            .or_insert_with(|| Box::new(ComponentStorage::<T>::new()))
            .as_any_mut()
            .downcast_mut::<ComponentStorage<T>>()
            .expect("TypeId lookup ensures correct type, downcast cannot fail")
    }

    pub fn get_storage<T: Component>(&self) -> Option<&ComponentStorage<T>> {
        self.storages
            .get(&std::any::TypeId::of::<T>())
            .map(|storage| {
                storage
                    .as_any()
                    .downcast_ref::<ComponentStorage<T>>()
                    .expect("TypeId lookup ensures correct type, downcast cannot fail")
            })
    }

    pub fn get_storage_mut<T: Component>(&mut self) -> Option<&mut ComponentStorage<T>> {
        let type_id = std::any::TypeId::of::<T>();
        self.storages.get_mut(&type_id).map(|storage| {
            storage
                .as_any_mut()
                .downcast_mut::<ComponentStorage<T>>()
                .expect("TypeId lookup ensures correct type, downcast cannot fail")
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
    ///
    /// Returns the TypeIds of components that were actually present and removed.
    pub fn remove_entity(&mut self, entity_id: EntityId) -> Vec<std::any::TypeId> {
        let mut removed_types = Vec::new();
        for (&type_id, storage) in self.storages.iter_mut() {
            if storage.contains_entity(entity_id) {
                storage.remove_entity(entity_id);
                removed_types.push(type_id);
            }
        }
        removed_types
    }

    pub fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        for storage in self.storages.values_mut() {
            storage.retain_entities(valid_entities);
        }
    }

    pub fn clear(&mut self) {
        for storage in self.storages.values_mut() {
            storage.clear();
        }
    }

    pub fn storage_count(&self) -> usize {
        self.storages.len()
    }

    pub(crate) fn entities_with_components(&self) -> std::collections::HashSet<EntityId> {
        let mut ids = std::collections::HashSet::new();
        for storage in self.storages.values() {
            storage.collect_entity_ids(&mut ids);
        }
        ids
    }

    /// Snapshots the current maximum generation for each component type.
    ///
    /// After this call, `is_changed` will return false for all entities until
    /// their components are next mutated via `insert` or `get_mut`.
    pub(crate) fn clear_changed(&mut self) {
        for (&type_id, storage) in self.storages.iter() {
            let max_gen = storage.max_generation();
            self.changed_generations.insert(type_id, max_gen);
        }
    }

    pub(crate) fn collect_changed_entity_ids(
        &self,
        type_ids: &[std::any::TypeId],
    ) -> std::collections::HashSet<EntityId> {
        let mut changed = std::collections::HashSet::new();
        let all_entities = self.entities_with_components();
        for entity_id in all_entities {
            for &type_id in type_ids {
                if self.is_changed_by_type_id(entity_id, type_id) {
                    changed.insert(entity_id);
                    break;
                }
            }
        }
        changed
    }

    fn is_changed_by_type_id(&self, entity_id: EntityId, type_id: std::any::TypeId) -> bool {
        let storage = match self.storages.get(&type_id) {
            Some(s) => s,
            None => return false,
        };
        let current_gen = storage.generation_for_entity(entity_id);
        if current_gen == 0 {
            return false;
        }
        let last_seen = self.changed_generations.get(&type_id).copied().unwrap_or(0);
        current_gen > last_seen
    }

    /// Returns a raw pointer to `self` for use with the `get_two_storage_mut` /
    /// `get_storage_mut_and_ref` helpers.  This is the single sanctioned place
    /// where the `as *mut ComponentStorageManager` cast lives outside of tests.
    #[inline]
    pub(crate) fn borrow_ptr(&mut self) -> *mut ComponentStorageManager {
        self as *mut ComponentStorageManager
    }

    /// Obtains simultaneous mutable and immutable references to two distinct
    /// component storages from a raw pointer.
    ///
    /// This is the centralised helper that replaces open-coded raw-pointer casts
    /// throughout the query iterator modules.  All unsafe reasoning about why
    /// disjoint HashMap entries can be borrowed simultaneously lives here.
    ///
    /// # Safety
    ///
    /// * `ptr` must be a valid, properly-aligned pointer to a `ComponentStorageManager`
    ///   that outlives lifetime `'a`.
    /// * Callers **must** ensure `TypeId::of::<T1>() != TypeId::of::<T2>()`.
    ///   Violating this produces a mutable and immutable reference to the same
    ///   storage, which is UB.
    pub(crate) unsafe fn get_storage_mut_and_ref<'a, T1: Component, T2: Component>(
        ptr: *mut ComponentStorageManager,
    ) -> (
        Option<&'a mut ComponentStorage<T1>>,
        Option<&'a ComponentStorage<T2>>,
    ) {
        // SAFETY: Caller guarantees `ptr` is valid for lifetime `'a` and T1 ≠ T2
        // (disjoint HashMap entries), so the two lookups produce independent references.
        unsafe {
            let storage1 = (*ptr).get_storage_mut::<T1>();
            let storage2 = (*ptr).get_storage::<T2>();
            (storage1, storage2)
        }
    }

    /// Obtains simultaneous mutable references to two distinct component storages
    /// from a raw pointer.
    ///
    /// # Safety
    ///
    /// * `ptr` must be a valid, properly-aligned pointer to a `ComponentStorageManager`
    ///   that outlives lifetime `'a`.
    /// * Callers **must** ensure `TypeId::of::<T1>() != TypeId::of::<T2>()`.
    ///   Violating this produces two mutable references to the same storage, which is UB.
    pub(crate) unsafe fn get_two_storage_mut<'a, T1: Component, T2: Component>(
        ptr: *mut ComponentStorageManager,
    ) -> (
        Option<&'a mut ComponentStorage<T1>>,
        Option<&'a mut ComponentStorage<T2>>,
    ) {
        // SAFETY: Caller guarantees `ptr` is valid for lifetime `'a` and T1 ≠ T2
        // (disjoint HashMap entries), so the two lookups produce independent references.
        unsafe {
            let storage1 = (*ptr).get_storage_mut::<T1>();
            let storage2 = (*ptr).get_storage_mut::<T2>();
            (storage1, storage2)
        }
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

    #[derive(Component, Clone, Debug, PartialEq)]
    struct TestComponent2 {
        value: f32,
    }

    #[test]
    fn test_component_storage_replace() {
        let mut storage = ComponentStorage::<TestComponent>::new();
        let entity = EntityId::test_new(0);

        storage.insert(entity, TestComponent { value: 42 });
        storage.insert(entity, TestComponent { value: 100 });

        let component = storage.get(entity).unwrap();
        assert_eq!(component.value, 100);
        assert_eq!(storage.len(), 1);
    }

    #[test]
    fn test_query_partial_components() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::test_new(0);
        let entity2 = EntityId::test_new(1);
        let entity3 = EntityId::test_new(2);

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
    #[should_panic]
    fn test_query_three_same_type_panics() {
        let mut manager = ComponentStorageManager::new();
        let _: Vec<EntityId> = manager
            .query::<(&TestComponent, &TestComponent, &TestComponent)>()
            .map(|(id, _, _, _)| id)
            .collect();
    }

    #[test]
    fn test_query_returns_correct_component_values() {
        let mut manager = ComponentStorageManager::new();
        let entity1 = EntityId::test_new(0);
        let entity2 = EntityId::test_new(1);

        manager.add_component(entity1, TestComponent { value: 10 });
        manager.add_component(entity1, TestComponent2 { value: 1.5 });
        manager.add_component(entity2, TestComponent { value: 20 });
        manager.add_component(entity2, TestComponent2 { value: 2.5 });

        let mut results: Vec<(i32, f32)> = manager
            .query::<(&TestComponent, &TestComponent2)>()
            .map(|(_, a, b)| (a.value, b.value))
            .collect();
        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (10, 1.5));
        assert_eq!(results[1], (20, 2.5));
    }

    #[test]
    fn test_query_mut_modifies_values() {
        let mut manager = ComponentStorageManager::new();
        let entity = EntityId::test_new(0);

        manager.add_component(entity, TestComponent { value: 10 });

        for (_, comp) in manager.query::<&mut TestComponent>() {
            comp.value += 5;
        }

        assert_eq!(
            manager
                .get_component::<TestComponent>(entity)
                .unwrap()
                .value,
            15
        );
    }

    #[test]
    fn test_storage_remove_nonexistent_component_no_panic() {
        let mut manager = ComponentStorageManager::new();
        let entity = EntityId::test_new(0);

        assert!(!manager.remove_component::<TestComponent>(entity));
    }

    #[test]
    fn test_get_two_storage_mut() {
        let mut manager = ComponentStorageManager::new();
        let entity = EntityId::test_new(0);
        manager.add_component(entity, TestComponent { value: 42 });
        manager.add_component(entity, TestComponent2 { value: 1.5 });

        unsafe {
            let ptr = &mut manager as *mut ComponentStorageManager;
            let (s1, s2) =
                ComponentStorageManager::get_two_storage_mut::<TestComponent, TestComponent2>(ptr);
            assert!(s1.is_some());
            assert!(s2.is_some());
            assert_eq!(s1.unwrap().get(entity).unwrap().value, 42);
            assert_eq!(s2.unwrap().get(entity).unwrap().value, 1.5);
        }
    }

    #[test]
    fn test_get_two_storage_mut_missing_types() {
        let mut manager = ComponentStorageManager::new();

        unsafe {
            let ptr = &mut manager as *mut ComponentStorageManager;
            let (s1, s2) =
                ComponentStorageManager::get_two_storage_mut::<TestComponent, TestComponent2>(ptr);
            assert!(s1.is_none());
            assert!(s2.is_none());
        }
    }
}
