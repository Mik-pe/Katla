use crate::components::Component;
use crate::entity::EntityId;
use crate::storage::ComponentStorageManager;
use crate::system::{OrderedSystem, System, SystemExecutionOrder};
use std::collections::HashSet;

/// World is the central manager for the ECS framework.
///
/// It maintains all entities and systems, handles entity creation/deletion,
/// and coordinates system execution. Components are stored in separate vectors
/// for better cache locality and performance.
///
/// # Examples
///
/// ```
/// use katla_ecs::{World, Component};
///
/// #[derive(Component, Default)]
/// struct TransformComponent {
///     position: [f32; 3],
///     rotation: [f32; 4],
///     scale: [f32; 3],
/// }
///
/// let mut world = World::new();
/// let entity_id = world.create_entity();
/// world.add_component(entity_id, TransformComponent::default());
/// world.update(0.016);
/// ```
pub struct World<'a> {
    /// Set of active entity IDs
    entities: HashSet<EntityId>,
    /// Component storage manager
    storage: ComponentStorageManager<'a>,
    /// Registered systems
    systems: Vec<OrderedSystem>,
    /// Next entity ID to assign
    next_entity_id: u64,
}

impl<'a> World<'a> {
    /// Creates a new empty World.
    pub fn new() -> Self {
        Self {
            entities: HashSet::new(),
            storage: ComponentStorageManager::new(),
            systems: Vec::new(),
            next_entity_id: 0,
        }
    }

    /// Creates a new entity and returns its ID.
    pub fn create_entity(&mut self) -> EntityId {
        let id = EntityId::new(self.next_entity_id);
        self.next_entity_id += 1;
        self.entities.insert(id);
        id
    }

    /// Creates a new entity with a specific ID.
    ///
    /// Use with caution - if the ID already exists, this will do nothing.
    pub fn create_entity_with_id(&mut self, id: EntityId) -> EntityId {
        self.entities.insert(id);

        // Update next_entity_id if necessary
        if id.0 >= self.next_entity_id {
            self.next_entity_id = id.0 + 1;
        }

        id
    }

    /// Destroys an entity and removes all its components.
    ///
    /// Returns `true` if the entity existed and was removed, `false` otherwise.
    pub fn destroy_entity(&mut self, id: EntityId) -> bool {
        if self.entities.remove(&id) {
            self.storage.remove_entity(id);
            true
        } else {
            false
        }
    }

    /// Checks if an entity exists in the world.
    pub fn entity_exists(&self, id: EntityId) -> bool {
        self.entities.contains(&id)
    }

    /// Adds a component to an entity.
    pub fn add_component(&mut self, id: EntityId, component: impl Component + 'static) {
        if self.entities.contains(&id) {
            self.storage.add_component(id, component);
        }
    }

    /// Removes a component from an entity.
    pub fn remove_component<T>(&mut self, id: EntityId) -> bool
    where
        T: Component + 'static,
    {
        self.storage.remove_component::<T>(id)
    }

    /// Gets a reference to a component for an entity.
    pub fn get_component<T>(&self, id: EntityId) -> Option<&T>
    where
        T: Component + 'static,
    {
        self.storage.get_component::<T>(id)
    }

    /// Gets a mutable reference to a component for an entity.
    pub fn get_component_mut<T>(&mut self, id: EntityId) -> Option<&mut T>
    where
        T: Component + 'static,
    {
        self.storage.get_component_mut::<T>(id)
    }

    /// Checks if an entity has a specific component.
    pub fn has_component<T>(&self, id: EntityId) -> bool
    where
        T: Component + 'static,
    {
        self.storage.has_component::<T>(id)
    }

    /// Gets a reference to the component storage manager.
    ///
    /// This allows systems and external code to work directly with component storages.
    pub fn storage(&self) -> &ComponentStorageManager {
        &self.storage
    }

    /// Gets a mutable reference to the component storage manager.
    ///
    /// This allows systems and external code to work directly with component storages.
    pub fn storage_mut(&mut self) -> &mut ComponentStorageManager<'a> {
        &mut self.storage
    }

    /// Registers a system with the world.
    ///
    /// Systems will be executed in order based on their SystemExecutionOrder.
    ///
    pub fn register_system(&mut self, system: Box<dyn System>, order: SystemExecutionOrder) {
        let mut ordered_system = OrderedSystem::new(system, order);
        ordered_system.system.initialize();
        self.systems.push(ordered_system);
        self.sort_systems();
    }

    /// Sorts systems by their execution order.
    fn sort_systems(&mut self) {
        self.systems.sort_by(|a, b| a.order.cmp(&b.order));
    }

    /// Updates all systems.
    ///
    /// This is the main update loop for the ECS. It should be called once per frame.
    /// Systems have direct access to component storages for efficient iteration.
    ///
    /// # Arguments
    ///
    /// * `delta_time` - Time elapsed since the last frame in seconds
    pub fn update(&mut self, delta_time: f32) {
        for ordered_system in &mut self.systems {
            if !ordered_system.system.is_enabled() {
                continue;
            }

            ordered_system.system.update(&mut self.storage, delta_time);
        }
    }

    /// Returns the number of entities in the world.
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Returns the number of systems registered with the world.
    pub fn system_count(&self) -> usize {
        self.systems.len()
    }

    /// Clears all entities from the world.
    pub fn clear_entities(&mut self) {
        self.entities.clear();
        self.storage.clear();
    }

    /// Removes all systems from the world.
    pub fn clear_systems(&mut self) {
        for ordered_system in &mut self.systems {
            ordered_system.system.shutdown();
        }
        self.systems.clear();
    }

    /// Returns an iterator over all entity IDs in the world.
    pub fn entity_ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.iter().copied()
    }

    /// Removes entities that have no components.
    pub fn cleanup_empty_entities(&mut self) {
        let entities_to_keep: HashSet<EntityId> = self.entities.clone();
        self.storage.retain_entities(&entities_to_keep);
    }
}

impl<'a> Default for World<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Drop for World<'a> {
    fn drop(&mut self) {
        // Clean up systems when the world is destroyed
        for ordered_system in &mut self.systems {
            ordered_system.system.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Component;
    use crate::system::System;

    #[derive(Component, Default)]
    struct TestComponent {
        value: i32,
    }

    struct TestSystem {
        processed_count: usize,
    }

    impl TestSystem {
        fn new() -> Self {
            Self { processed_count: 0 }
        }
    }

    impl System for TestSystem {
        fn update(&mut self, storage: &mut ComponentStorageManager, _delta_time: f32) {
            if let Some(transforms) = storage.get_storage::<TestComponent>() {
                self.processed_count = transforms.len();
            }
        }

        fn name(&self) -> &str {
            "TestSystem"
        }
    }

    #[test]
    fn test_world_creation() {
        let world = World::new();
        assert_eq!(world.entity_count(), 0);
        assert_eq!(world.system_count(), 0);
    }

    #[test]
    fn test_create_entity() {
        let mut world = World::new();
        let id1 = world.create_entity();
        let id2 = world.create_entity();

        assert_eq!(world.entity_count(), 2);
        assert_ne!(id1, id2);
        assert!(world.entity_exists(id1));
        assert!(world.entity_exists(id2));
    }

    #[test]
    fn test_destroy_entity() {
        let mut world = World::new();
        let id = world.create_entity();

        assert_eq!(world.entity_count(), 1);
        assert!(world.destroy_entity(id));
        assert_eq!(world.entity_count(), 0);
        assert!(!world.entity_exists(id));
    }

    #[test]
    fn test_add_component() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent::default());
        assert!(world.has_component::<TestComponent>(id));
    }

    #[test]
    fn test_remove_component() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent::default());
        assert!(world.remove_component::<TestComponent>(id));
        assert!(!world.has_component::<TestComponent>(id));
    }

    #[test]
    fn test_get_component() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent::default());
        assert!(world.get_component::<TestComponent>(id).is_some());
    }

    #[test]
    fn test_get_component_mut() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent::default());

        if let Some(test) = world.get_component_mut::<TestComponent>(id) {
            test.value = 5;
        }

        let transform = world.get_component::<TestComponent>(id).unwrap();
        assert_eq!(transform.value, 5);
    }

    #[test]
    fn test_register_system() {
        let mut world = World::new();
        let system = Box::new(TestSystem::new());

        world.register_system(system, SystemExecutionOrder::NORMAL);
        assert_eq!(world.system_count(), 1);
    }

    #[test]
    fn test_system_update() {
        let mut world = World::new();

        // Create entities with components
        let id1 = world.create_entity();
        world.add_component(id1, TestComponent::default());

        let id2 = world.create_entity();
        world.add_component(id2, TestComponent::default());

        // Register system
        let system = Box::new(TestSystem::new());
        world.register_system(system, SystemExecutionOrder::NORMAL);

        // Update world
        world.update(0.016);
    }

    #[test]
    fn test_clear_entities() {
        let mut world = World::new();
        world.create_entity();
        world.create_entity();
        world.create_entity();

        assert_eq!(world.entity_count(), 3);
        world.clear_entities();
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn test_entity_iteration() {
        let mut world = World::new();
        let id1 = world.create_entity();
        let id2 = world.create_entity();

        let ids: Vec<EntityId> = world.entity_ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn test_destroy_entity_removes_components() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent::default());
        assert!(world.has_component::<TestComponent>(id));

        world.destroy_entity(id);

        // Component should be removed when entity is destroyed
        assert!(!world.has_component::<TestComponent>(id));
    }
}
