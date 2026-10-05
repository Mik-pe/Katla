//! Component storage for ECS (Entity Component System).
//!
//! This module provides storage for components with O(1) lookup, insert, and remove operations
//! while maintaining contiguous storage for fast iteration.

use crate::components::Component;
use crate::entity::EntityId;
use crate::query::QueryData;
use crate::sparse_set::{SparseSet, SparseView};
use std::any::Any;
use std::cell::UnsafeCell;

/// Storage for components of a specific type.
///
/// Uses a sparse set internally for O(1) lookups while maintaining contiguous
/// storage for fast iteration over all components of a given type.
pub struct ComponentStorage<T: Component> {
    /// Internal sparse set for O(1) lookups
    storage: SparseSet<EntityId, T>,
    /// Per-type dirty entity tracking for O(dirty_entities) change detection.
    /// Populated on insert and get_mut. Cleared by clear_changed().
    dirty: SparseSet<EntityId, ()>,
    all_changed: bool,
}

impl<T: Component> ComponentStorage<T> {
    /// Creates a new empty ComponentStorage.
    pub fn new() -> Self {
        Self {
            storage: SparseSet::new(),
            dirty: SparseSet::new(),
            all_changed: false,
        }
    }

    /// Adds a component for the given entity.
    ///
    /// If the entity already has this component type, it will be replaced.
    /// Marks the entity as changed for change detection.
    #[inline]
    pub fn insert(&mut self, entity_id: EntityId, component: T) {
        self.storage.insert(entity_id, component);
        self.dirty.insert(entity_id, ());
    }

    /// Removes a component for the given entity.
    ///
    /// Returns true if the component was removed, false if it didn't exist.
    #[inline]
    pub fn remove(&mut self, entity_id: EntityId) -> bool {
        let removed = self.storage.remove(entity_id);
        if removed {
            self.dirty.remove(entity_id);
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
        if self.storage.contains(entity_id) {
            self.dirty.insert(entity_id, ());
        }
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
        self.mark_all_changed();
        self.storage.iter_mut()
    }

    /// Returns an iterator over just the components.
    pub fn components(&self) -> impl Iterator<Item = &T> {
        self.storage.values()
    }

    /// Returns a mutable iterator over just the components.
    pub fn components_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.mark_all_changed();
        self.storage.values_mut()
    }

    /// Returns a reference to the internal component storage (for query module).
    pub(crate) fn components_vec(&self) -> &Vec<(EntityId, T)> {
        self.storage.dense()
    }

    pub(crate) fn mark_changed(&mut self, ids: &[EntityId]) {
        if ids.len() == self.storage.len() {
            self.all_changed = true;
        }
        if !self.all_changed {
            for &id in ids {
                self.dirty.insert(id, ());
            }
        }
    }

    fn mark_all_changed(&mut self) {
        self.all_changed = true;
    }

    pub(crate) fn query_view(&self) -> SparseView<'_, EntityId, T> {
        self.storage.view()
    }

    pub(crate) fn query_view_mut(&mut self) -> SparseView<'_, EntityId, T> {
        self.mark_all_changed();
        self.storage.view_mut()
    }

    pub(crate) fn selected_indices(&self, ids: &[EntityId]) -> Vec<usize> {
        self.storage.selected_indices(ids)
    }

    pub(crate) fn dense_base(&self) -> *const (EntityId, T) {
        self.storage.dense_base()
    }

    pub(crate) fn selected_mut_base(&mut self, ids: &[EntityId]) -> *mut (EntityId, T) {
        self.mark_changed(ids);
        self.storage.dense_base_mut()
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

    /// Clears all components and dirty tracking.
    pub fn clear(&mut self) {
        self.storage.clear();
        self.dirty.clear();
        self.all_changed = false;
    }

    /// Removes all components for entities not in the given set.
    pub fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        self.storage.retain_keys(valid_entities);
        self.dirty.retain_keys(valid_entities);
    }
}

impl<T: Component> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for type-erased component storage operations.
pub trait AnyComponentStorage: Any {
    /// Number of entities in this column.
    fn component_count(&self) -> usize;

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

    /// Appends entity IDs in dense storage order.
    fn append_entity_ids(&self, out: &mut Vec<EntityId>);

    /// Collects dirty entity IDs (entities modified since last clear_changed).
    fn collect_dirty_entity_ids(&self, out: &mut std::collections::HashSet<EntityId>);

    /// Clears the dirty set for this storage.
    fn clear_dirty(&mut self);

    /// Returns a reference to self as `Any` for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Returns a mutable reference to self as `Any` for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Component> AnyComponentStorage for ComponentStorage<T> {
    fn component_count(&self) -> usize {
        self.storage.len()
    }
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

    fn append_entity_ids(&self, out: &mut Vec<EntityId>) {
        out.extend(self.storage.keys());
    }

    fn collect_dirty_entity_ids(&self, out: &mut std::collections::HashSet<EntityId>) {
        if self.all_changed {
            out.extend(self.storage.keys());
        } else {
            out.extend(self.dirty.keys());
        }
    }

    fn clear_dirty(&mut self) {
        self.dirty.clear();
        self.all_changed = false;
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
    storages: std::collections::HashMap<std::any::TypeId, Box<UnsafeCell<dyn AnyComponentStorage>>>,
}

impl ComponentStorageManager {
    /// Creates a new empty ComponentStorageManager.
    pub fn new() -> Self {
        Self {
            storages: std::collections::HashMap::new(),
        }
    }

    fn get_or_create_storage<T: Component>(&mut self) -> &mut ComponentStorage<T> {
        let type_id = std::any::TypeId::of::<T>();
        let storages = &mut self.storages;

        storages
            .entry(type_id)
            .or_insert_with(|| Box::new(UnsafeCell::new(ComponentStorage::<T>::new())))
            .get_mut()
            .as_any_mut()
            .downcast_mut::<ComponentStorage<T>>()
            .expect("TypeId lookup ensures correct type, downcast cannot fail")
    }

    pub fn get_storage<T: Component>(&self) -> Option<&ComponentStorage<T>> {
        self.storages
            .get(&std::any::TypeId::of::<T>())
            .map(|storage| {
                unsafe { &*storage.get() }
                    .as_any()
                    .downcast_ref::<ComponentStorage<T>>()
                    .expect("TypeId lookup ensures correct type, downcast cannot fail")
            })
    }

    pub fn get_storage_mut<T: Component>(&mut self) -> Option<&mut ComponentStorage<T>> {
        let type_id = std::any::TypeId::of::<T>();
        self.storages.get_mut(&type_id).map(|storage| {
            storage
                .get_mut()
                .as_any_mut()
                .downcast_mut::<ComponentStorage<T>>()
                .expect("TypeId lookup ensures correct type, downcast cannot fail")
        })
    }

    /// Adds a component for the given entity.
    #[inline]
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
    #[inline]
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
            let storage = storage.get_mut();
            if storage.contains_entity(entity_id) {
                storage.remove_entity(entity_id);
                removed_types.push(type_id);
            }
        }
        removed_types
    }

    pub fn retain_entities(&mut self, valid_entities: &std::collections::HashSet<EntityId>) {
        for storage in self.storages.values_mut() {
            storage.get_mut().retain_entities(valid_entities);
        }
    }

    pub fn clear(&mut self) {
        for storage in self.storages.values_mut() {
            storage.get_mut().clear();
        }
    }

    pub fn storage_count(&self) -> usize {
        self.storages.len()
    }

    pub(crate) fn entities_with_components(&self) -> std::collections::HashSet<EntityId> {
        let mut ids = std::collections::HashSet::new();
        for storage in self.storages.values() {
            unsafe { &*storage.get() }.collect_entity_ids(&mut ids);
        }
        ids
    }

    /// Snapshots the current maximum generation for each component type.
    ///
    /// After this call, `is_changed` will return false for all entities until
    /// their components are next mutated via `insert` or `get_mut`.
    pub(crate) fn clear_changed(&mut self) {
        for storage in self.storages.values_mut() {
            storage.get_mut().clear_dirty();
        }
    }

    /// Collects entity IDs that have been modified since the last `clear_changed()` call
    /// into an existing `HashSet`, clearing it first.
    pub(crate) fn collect_changed_entity_ids_into(
        &self,
        type_ids: &[std::any::TypeId],
        out: &mut std::collections::HashSet<EntityId>,
    ) {
        out.clear();
        for &type_id in type_ids {
            if let Some(storage) = self.storages.get(&type_id) {
                unsafe { &*storage.get() }.collect_dirty_entity_ids(out);
            }
        }
    }

    /// Resolves a storage through its stable interior-mutable allocation.
    ///
    /// # Safety
    /// The caller must own exclusive access to this component type for the
    /// returned borrow, and prevent structural changes for that duration.
    pub(crate) unsafe fn storage_mut_unchecked<T: Component>(
        &self,
    ) -> Option<*mut ComponentStorage<T>> {
        self.storages
            .get(&std::any::TypeId::of::<T>())
            .map(|storage| {
                unsafe { &mut *storage.get() }
                    .as_any_mut()
                    .downcast_mut::<ComponentStorage<T>>()
                    .expect("TypeId lookup ensures correct type")
                    as *mut ComponentStorage<T>
            })
    }

    /// Tests component membership without borrowing any other component storage.
    pub(crate) fn contains_type(&self, type_id: std::any::TypeId, entity: EntityId) -> bool {
        self.storages
            .get(&type_id)
            .is_some_and(|storage| unsafe { &*storage.get() }.contains_entity(entity))
    }

    pub(crate) fn type_len(&self, type_id: std::any::TypeId) -> usize {
        self.storages
            .get(&type_id)
            .map_or(0, |storage| unsafe { &*storage.get() }.component_count())
    }

    pub(crate) fn ids_for_type(&self, type_id: std::any::TypeId) -> Vec<EntityId> {
        let mut ids = Vec::new();
        if let Some(storage) = self.storages.get(&type_id) {
            unsafe { &*storage.get() }.append_entity_ids(&mut ids);
        }
        ids
    }

    /// Creates a query for iterating over entities with specific components.
    ///
    /// See the [`query`](crate::query) module for detailed documentation and examples.
    ///
    /// # Example
    /// ```
    /// use katla_ecs::{World, Component};
    ///
    /// #[derive(Component)]
    /// struct HealthComponent { value: f32 }
    ///
    /// #[derive(Component)]
    /// struct DamageComponent { amount: f32 }
    ///
    /// let mut world = World::new();
    /// let e = world.spawn((
    ///     HealthComponent { value: 100.0 },
    ///     DamageComponent { amount: 10.0 },
    /// ));
    ///
    /// // Query with mutable and immutable access
    /// for (_entity, health, damage) in world.query::<(&mut HealthComponent, &DamageComponent)>() {
    ///     health.value -= damage.amount;
    /// }
    ///
    /// let v = world.get_component::<HealthComponent>(e).unwrap();
    /// assert_eq!(v.value, 90.0);
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
}
