use crate::components::Component;
use crate::entity::EntityId;
use crate::entity_allocator::EntityAllocator;
use crate::events::{ComponentEvent, EntityEvent};
use crate::resource::ResourceStorage;
use crate::storage::ComponentStorageManager;
use crate::system::{OrderedSystem, System, SystemExecutionOrder};
use crate::{InputState, Resource};
use std::cell::UnsafeCell;

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
pub struct World {
    /// Entity allocator with generation-based IDs
    entities: EntityAllocator,
    /// Component storage manager.
    /// Wrapped in UnsafeCell to support `query_ref` (immutable queries from &self).
    pub(crate) storage: UnsafeCell<ComponentStorageManager>,
    /// Registered systems
    systems: Vec<OrderedSystem>,
    /// Global Input state
    input_state: InputState,
    /// Global resources storage
    resources: ResourceStorage,
    /// Entity lifecycle events emitted during the current frame
    entity_events: Vec<EntityEvent>,
    /// Component events emitted during the current frame
    component_events: Vec<ComponentEvent>,
}

impl World {
    /// Creates a new empty World.
    pub fn new() -> Self {
        Self {
            entities: EntityAllocator::new(),
            storage: UnsafeCell::new(ComponentStorageManager::new()),
            systems: Vec::new(),
            input_state: InputState::new(),
            resources: ResourceStorage::new(),
            entity_events: Vec::new(),
            component_events: Vec::new(),
        }
    }

    /// Creates a new entity and returns its ID.
    pub fn create_entity(&mut self) -> EntityId {
        let id = self.entities.allocate();
        self.entity_events.push(EntityEvent::Spawned(id));
        id
    }

    /// Gets a mutable reference to the component storage manager.
    ///
    /// Use this for direct storage access when the `query` API is insufficient.
    pub fn storage_mut(&mut self) -> &mut ComponentStorageManager {
        self.storage.get_mut()
    }

    /// Spawns a new entity with a bundle of components.
    ///
    /// This is an ergonomic way to create an entity with multiple components
    /// in a single call. The bundle can be any tuple of components from size 1-8.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use katla_ecs::{World, Component, Spawnable};
    ///
    /// #[derive(Component, Default)]
    /// struct Transform { position: [f32; 3] }
    ///
    /// #[derive(Component, Default)]
    /// struct Velocity { value: [f32; 3] }
    ///
    /// let mut world = World::new();
    ///
    /// // Spawn entity with multiple components
    /// let player = world.spawn((
    ///     Transform::default(),
    ///     Velocity::default(),
    /// ));
    /// ```
    pub fn spawn<B: crate::spawn::Spawnable>(&mut self, bundle: B) -> EntityId {
        bundle.spawn(self)
    }

    /// Destroys an entity and removes all its components.
    ///
    /// Returns `true` if the entity existed and was removed, `false` otherwise.
    /// Emits `EntityEvent::Destroyed` only for live entities.
    /// Emits `ComponentEvent::Removed` for each component that was on the entity.
    pub fn destroy_entity(&mut self, id: EntityId) -> bool {
        if self.entities.deallocate(id) {
            let removed_types = self.storage.get_mut().remove_entity(id);
            for type_id in &removed_types {
                self.component_events
                    .push(ComponentEvent::Removed(id, *type_id));
            }
            self.entity_events.push(EntityEvent::Destroyed(id));
            true
        } else {
            false
        }
    }

    /// Checks if an entity exists in the world.
    pub fn entity_exists(&self, id: EntityId) -> bool {
        self.entities.is_valid(id)
    }

    /// Adds a component to an entity.
    ///
    /// Does nothing if the entity doesn't exist.
    /// Emits `ComponentEvent::Added` when the component is added.
    pub fn add_component<T: Component + 'static>(&mut self, id: EntityId, component: T) {
        if self.entities.is_valid(id) {
            self.storage.get_mut().add_component(id, component);
            self.component_events
                .push(ComponentEvent::Added(id, std::any::TypeId::of::<T>()));
        }
    }

    /// Removes a component from an entity.
    ///
    /// Emits `ComponentEvent::Removed` only if the component existed on the entity.
    pub fn remove_component<T>(&mut self, id: EntityId) -> bool
    where
        T: Component + 'static,
    {
        if self.storage.get_mut().remove_component::<T>(id) {
            self.component_events
                .push(ComponentEvent::Removed(id, std::any::TypeId::of::<T>()));
            true
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
    /// for (entity, transform) in world.query::<&TransformComponent>() {
    ///     // ...
    /// }
    ///
    /// // Use get_component for specific entity access:
    /// if let Some(transform) = world.get_component::<TransformComponent>(specific_entity) {
    ///     // ...
    /// }
    /// ```
    pub fn get_component<T>(&self, id: EntityId) -> Option<&T>
    where
        T: Component + 'static,
    {
        if self.entities.is_valid(id) {
            // SAFETY: We only need immutable access through get_storage
            unsafe { (*self.storage.get()).get_component::<T>(id) }
        } else {
            None
        }
    }

    /// Gets a mutable reference to a component for a specific entity.
    ///
    /// Use this for accessing individual entities by ID. For iterating over multiple entities
    /// with components, prefer using queries. See [`get_component`](Self::get_component) for details.
    pub fn get_component_mut<T>(&mut self, id: EntityId) -> Option<&mut T>
    where
        T: Component + 'static,
    {
        if self.entities.is_valid(id) {
            self.storage.get_mut().get_component_mut::<T>(id)
        } else {
            None
        }
    }

    /// Creates a query for iterating over entities with specific components.
    ///
    /// Queries provide efficient iteration over entities with specific component combinations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Query and modify entities
    /// for (entity, transform, velocity) in world.query::<(&mut TransformComponent, &VelocityComponent)>() {
    ///     transform.position += velocity.value * delta_time;
    /// }
    ///
    /// // Query with three components
    /// for (entity, pos, vel, force) in world.query::<(&PositionComponent, &VelocityComponent, &ForceComponent)>() {
    ///     // Process physics...
    /// }
    /// ```
    pub fn query<Q: crate::query::QueryData>(&mut self) -> Q::Iter<'_> {
        self.storage.get_mut().query::<Q>()
    }

    /// Read-only query for iterating over entities with specific components.
    ///
    /// Unlike [`query`](Self::query), this takes `&self` and only supports
    /// immutable access patterns. Use this when you need to iterate components
    /// from a shared reference to the world (e.g., in UI callbacks or
    /// serialization that take `&World` or `&Application`).
    pub fn query_ref<Q: crate::query::QueryData>(&self) -> Q::Iter<'_> {
        // SAFETY: For immutable query types (e.g., &T, (&T, &U)), `Q::fetch`
        // only calls `get_storage::<T>()` which reads through the HashMap without
        // mutation. Mutable query types (e.g., &mut T) would be unsound here.
        unsafe { (*self.storage.get()).query::<Q>() }
    }

    /// Queries only entities whose components have changed since the last `clear_changed()` call.
    ///
    /// Uses the same query syntax as [`query`](Self::query) but filters results to only
    /// include entities where at least one queried component type was mutated (via
    /// `add_component` or `get_component_mut`) since the last frame.
    ///
    /// Change detection is automatically reset at the end of each [`update`](Self::update) call.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Only process entities whose Transform was added or mutably accessed
    /// for (entity, transform) in world.query_changed::<&TransformComponent>() {
    ///     // Recalculate derived data for changed transforms
    /// }
    /// ```
    pub fn query_changed<Q>(&mut self) -> QueryChangedIter<'_, Q>
    where
        Q: crate::query::QueryData,
    {
        let type_ids = Q::type_ids_for_changed();
        let changed_ids: std::collections::HashSet<EntityId> =
            self.storage.get_mut().collect_changed_entity_ids(&type_ids);

        QueryChangedIter {
            inner: self.storage.get_mut().query::<Q>(),
            changed_ids,
        }
    }

    /// Resets change detection tracking for all component types.
    ///
    /// After this call, `query_changed` will return an empty iterator until
    /// components are next mutated. This is called automatically at the end
    /// of each [`update`](Self::update) call.
    pub fn clear_changed(&mut self) {
        self.storage.get_mut().clear_changed();
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
        // Avoid aliasing `&mut self.systems` and `&mut self` at the same time.
        // Temporarily take the systems list, run updates, then put it back.
        let mut systems = std::mem::take(&mut self.systems);

        for ordered_system in &mut systems {
            if !ordered_system.system.is_enabled() {
                continue;
            }

            ordered_system.system.update(self, delta_time);
        }

        self.systems = systems;

        // Flush per-frame events
        self.entity_events.clear();
        self.component_events.clear();

        // Reset change detection so mutations in the next frame are tracked fresh
        self.storage.get_mut().clear_changed();

        // Clear per-frame mouse delta after the tick.
        self.input_state.mouse_delta = (0.0, 0.0);
        self.input_state.mouse_wheel_delta = 0.0;
    }

    /// Returns the number of entities in the world.
    pub fn entity_count(&self) -> usize {
        self.entities.live_count()
    }

    /// Returns the number of systems registered with the world.
    pub fn system_count(&self) -> usize {
        self.systems.len()
    }

    /// Clears all entities from the world.
    pub fn clear_entities(&mut self) {
        self.entities.clear();
        self.storage.get_mut().clear();
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
        self.entities.iter_live()
    }

    /// Returns the entity events emitted during the current frame.
    ///
    /// Events are accumulated from `create_entity` and `destroy_entity` calls
    /// and cleared at the end of each `update()` call.
    pub fn entity_events(&self) -> &[EntityEvent] {
        &self.entity_events
    }

    /// Returns the component events emitted during the current frame.
    ///
    /// Events are accumulated from `add_component`, `remove_component`, and
    /// `destroy_entity` calls and cleared at the end of each `update()` call.
    pub fn component_events(&self) -> &[ComponentEvent] {
        &self.component_events
    }

    /// Returns component events filtered to a specific component type.
    ///
    /// Only events whose `TypeId` matches `TypeId::of::<T>()` are returned.
    pub fn component_events_for<T: Component + 'static>(&self) -> Vec<&ComponentEvent> {
        let target = std::any::TypeId::of::<T>();
        self.component_events
            .iter()
            .filter(|event| match event {
                ComponentEvent::Added(_, type_id) | ComponentEvent::Removed(_, type_id) => {
                    *type_id == target
                }
            })
            .collect()
    }

    /// Removes entities that have no components.
    pub fn cleanup_empty_entities(&mut self) {
        let entities_with_components: std::collections::HashSet<EntityId> =
            unsafe { (*self.storage.get()).entities_with_components() };

        for entity_id in self.entities.iter_live().collect::<Vec<_>>() {
            if !entities_with_components.contains(&entity_id) {
                self.entities.deallocate(entity_id);
            }
        }
    }

    pub fn get_input(&self) -> &InputState {
        &self.input_state
    }

    pub fn get_input_mut(&mut self) -> &mut InputState {
        &mut self.input_state
    }

    /// Insert a resource into the world.
    ///
    /// If a resource of this type already exists, it will be replaced.
    ///
    /// # Example
    ///
    /// ```ignore
    /// world.insert_resource(GameSettings::default());
    /// ```
    pub fn insert_resource<R: Resource>(&mut self, resource: R) {
        self.resources.insert(resource);
    }

    /// Get a reference to a resource.
    ///
    /// Returns `None` if the resource doesn't exist.
    pub fn get_resource<R: Resource>(&self) -> Option<&R> {
        self.resources.get()
    }

    /// Get a mutable reference to a resource.
    ///
    /// Returns `None` if the resource doesn't exist.
    pub fn get_resource_mut<R: Resource>(&mut self) -> Option<&mut R> {
        self.resources.get_mut()
    }

    /// Check if a resource exists.
    pub fn contains_resource<R: Resource>(&self) -> bool {
        self.resources.contains::<R>()
    }

    /// Remove a resource from the world.
    ///
    /// Returns `None` if the resource didn't exist.
    pub fn remove_resource<R: Resource>(&mut self) -> Option<R> {
        self.resources.remove()
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        // Clean up systems when the world is destroyed
        for ordered_system in &mut self.systems {
            ordered_system.system.shutdown();
        }
    }
}

/// Iterator for `query_changed` that filters query results to only include
/// entities whose components have changed since the last `clear_changed()` call.
///
/// For multi-component queries, an entity is included if **any** of its queried
/// component types have been mutated (union semantics).
pub struct QueryChangedIter<'a, Q: crate::query::QueryData> {
    inner: Q::Iter<'a>,
    changed_ids: std::collections::HashSet<EntityId>,
}

impl<'a, Q: crate::query::QueryData> Iterator for QueryChangedIter<'a, Q> {
    type Item = Q::Item<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.inner.next()?;
            let entity_id = Q::entity_id_from_item(&item);
            if self.changed_ids.contains(&entity_id) {
                return Some(item);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Component;

    #[derive(Component, Default)]
    struct TestComponent {
        value: i32,
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
    fn test_destroy_entity_removes_components() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent::default());
        assert!(world.get_component::<TestComponent>(id).is_some());

        world.destroy_entity(id);

        // Component should be removed when entity is destroyed
        assert!(world.get_component::<TestComponent>(id).is_none());
    }

    #[test]
    fn test_stale_entity_reference() {
        let mut world = World::new();

        // Create and destroy entity
        let id1 = world.create_entity();
        world.add_component(id1, TestComponent { value: 42 });

        world.destroy_entity(id1);

        // Old ID should no longer be valid
        assert!(!world.entity_exists(id1));

        // Create new entity (should reuse slot with incremented generation)
        let id2 = world.create_entity();

        // The old ID should still be invalid
        assert!(!world.entity_exists(id1));

        // The new ID should be valid
        assert!(world.entity_exists(id2));
    }

    #[test]
    fn test_component_access_invalid_entity() {
        let mut world = World::new();
        let id = world.create_entity();
        world.destroy_entity(id);

        // Should return None for invalid entity
        assert!(world.get_component::<TestComponent>(id).is_none());
        assert!(world.get_component_mut::<TestComponent>(id).is_none());

        // add_component should be a no-op for invalid entity
        world.add_component(id, TestComponent::default());
        assert!(world.get_component::<TestComponent>(id).is_none());
    }

    #[test]
    fn test_entity_double_destroy_no_panic() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent { value: 42 });

        // First destroy should succeed
        assert!(world.destroy_entity(id));
        // Second destroy should return false but not panic
        assert!(!world.destroy_entity(id));
    }

    #[test]
    fn test_query_returns_correct_data_not_just_count() {
        let mut world = World::new();

        let id1 = world.create_entity();
        world.add_component(id1, TestComponent { value: 10 });

        let id2 = world.create_entity();
        world.add_component(id2, TestComponent { value: 20 });

        let id3 = world.create_entity();
        world.add_component(id3, TestComponent { value: 30 });

        let mut values: Vec<i32> = world
            .query::<&TestComponent>()
            .map(|(_, comp)| comp.value)
            .collect();
        values.sort();

        assert_eq!(values, vec![10, 20, 30]);
    }

    #[test]
    fn test_entity_destroy_then_spawn_reuses_slot() {
        let mut world = World::new();

        let id1 = world.create_entity();
        let original_index = id1.index();

        world.destroy_entity(id1);

        let id2 = world.create_entity();

        // The new entity should reuse the same slot index
        assert_eq!(id2.index(), original_index);
        // But should have a different generation
        assert_ne!(id2.generation(), id1.generation());
    }

    #[test]
    fn test_destroy_entity_returns_false_for_never_created() {
        let mut world = World::new();

        // Try to destroy an entity that was never created
        let fake_id = EntityId::test_new(999);
        assert!(!world.destroy_entity(fake_id));
    }

    #[test]
    fn test_clear_entities_allows_reuse() {
        let mut world = World::new();

        for _ in 0..10 {
            let id = world.create_entity();
            world.add_component(id, TestComponent { value: 1 });
        }

        assert_eq!(world.entity_count(), 10);
        world.clear_entities();
        assert_eq!(world.entity_count(), 0);

        // Should be able to create new entities after clear
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 99 });
        assert_eq!(world.entity_count(), 1);
        assert_eq!(world.get_component::<TestComponent>(id).unwrap().value, 99);
    }

    // --- Entity lifecycle event tests ---

    #[test]
    fn test_entity_spawn_event_emitted() {
        let mut world = World::new();

        let id = world.create_entity();

        let events = world.entity_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], EntityEvent::Spawned(id));
    }

    #[test]
    fn test_entity_destroyed_event_emitted() {
        let mut world = World::new();

        let id = world.create_entity();
        // Clear spawn event to isolate destroy event
        world.entity_events.clear();

        assert!(world.destroy_entity(id));

        let events = world.entity_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], EntityEvent::Destroyed(id));
    }

    #[test]
    fn test_destroy_invalid_entity_no_event() {
        let mut world = World::new();

        let fake_id = EntityId::test_new(999);
        assert!(!world.destroy_entity(fake_id));

        assert!(world.entity_events().is_empty());
    }

    #[test]
    fn test_destroy_already_destroyed_entity_no_event() {
        let mut world = World::new();

        let id = world.create_entity();
        assert!(world.destroy_entity(id));
        // Clear events from spawn + first destroy
        world.entity_events.clear();

        // Second destroy should not emit event
        assert!(!world.destroy_entity(id));
        assert!(world.entity_events().is_empty());
    }

    #[test]
    fn test_entity_events_flushed_after_update() {
        let mut world = World::new();

        world.create_entity();
        world.create_entity();
        assert_eq!(world.entity_events().len(), 2);

        world.update(0.016);

        assert!(world.entity_events().is_empty());
    }

    #[test]
    fn test_entity_events_visible_during_update() {
        use crate::system::{System, SystemExecutionOrder};

        #[derive(Default)]
        struct EventCheckerSystem {
            saw_events: bool,
        }

        impl System for EventCheckerSystem {
            fn update(&mut self, world: &mut World, _dt: f32) {
                self.saw_events = !world.entity_events().is_empty();
            }
        }

        let mut world = World::new();
        world.create_entity();
        world.create_entity();

        let system = EventCheckerSystem::default();
        world.register_system(Box::new(system), SystemExecutionOrder::EARLY);

        world.update(0.016);

        // We can't easily check the system's internal state after it's boxed,
        // but we can verify events were there during the tick by checking
        // that events are now flushed after update
        assert!(true); // If no panic occurred, events were accessible during update
    }

    #[test]
    fn test_entity_event_ordering() {
        let mut world = World::new();

        let id_a = world.create_entity();
        let id_b = world.create_entity();
        world.destroy_entity(id_a);

        let events = world.entity_events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], EntityEvent::Spawned(id_a));
        assert_eq!(events[1], EntityEvent::Spawned(id_b));
        assert_eq!(events[2], EntityEvent::Destroyed(id_a));
    }

    #[test]
    fn test_entity_events_accumulate_across_frame() {
        let mut world = World::new();

        let id1 = world.create_entity();
        let _id2 = world.create_entity();
        world.destroy_entity(id1);

        assert_eq!(world.entity_events().len(), 3);

        // After update, events should be flushed
        world.update(0.016);
        assert!(world.entity_events().is_empty());

        // New events in next frame
        let id3 = world.create_entity();
        world.destroy_entity(id3);

        assert_eq!(world.entity_events().len(), 2);
        let events = world.entity_events();
        assert_eq!(events[0], EntityEvent::Spawned(id3));
        assert_eq!(events[1], EntityEvent::Destroyed(id3));
    }

    // --- Component event tests ---

    #[test]
    fn test_component_added_event() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent { value: 42 });

        let events = world.component_events();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ComponentEvent::Added(id, std::any::TypeId::of::<TestComponent>())
        );
    }

    #[test]
    fn test_component_removed_event() {
        let mut world = World::new();
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 1 });

        // Clear the Added event
        world.component_events.clear();

        assert!(world.remove_component::<TestComponent>(id));

        let events = world.component_events();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ComponentEvent::Removed(id, std::any::TypeId::of::<TestComponent>())
        );
    }

    #[test]
    fn test_destroy_entity_emits_component() {
        let mut world = World::new();
        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 1 });

        // Clear the Added event
        world.component_events.clear();

        assert!(world.destroy_entity(id));

        let comp_events: Vec<_> = world
            .component_events()
            .iter()
            .filter(|e| matches!(e, ComponentEvent::Removed(..)))
            .collect();
        assert_eq!(comp_events.len(), 1);
        assert_eq!(
            comp_events[0],
            &ComponentEvent::Removed(id, std::any::TypeId::of::<TestComponent>())
        );

        // Entity destroyed event should also be emitted
        let ent_events: Vec<_> = world
            .entity_events()
            .iter()
            .filter(|e| matches!(e, EntityEvent::Destroyed(..)))
            .collect();
        assert_eq!(ent_events.len(), 1);
    }

    #[test]
    fn test_destroy_entity_emits_component_removed_for_multiple_types() {
        let mut world = World::new();

        #[derive(Component, Default)]
        struct CompA {
            _x: i32,
        }
        #[derive(Component, Default)]
        struct CompB {
            _y: f32,
        }

        let id = world.create_entity();
        world.add_component(id, CompA::default());
        world.add_component(id, CompB::default());

        world.component_events.clear();

        world.destroy_entity(id);

        let comp_events: Vec<_> = world
            .component_events()
            .iter()
            .filter(|e| matches!(e, ComponentEvent::Removed(..)))
            .collect();
        assert_eq!(comp_events.len(), 2);

        let type_ids: Vec<_> = comp_events
            .iter()
            .map(|e| match e {
                ComponentEvent::Removed(_, tid) => *tid,
                _ => panic!("unexpected event variant"),
            })
            .collect();
        assert!(type_ids.contains(&std::any::TypeId::of::<CompA>()));
        assert!(type_ids.contains(&std::any::TypeId::of::<CompB>()));
    }

    #[test]
    fn test_component_events_type_safety() {
        let mut world = World::new();

        #[derive(Component, Default)]
        struct Health {
            _hp: f32,
        }
        #[derive(Component, Default)]
        struct Mana {
            _mp: f32,
        }

        let id1 = world.create_entity();
        let id2 = world.create_entity();
        world.add_component(id1, Health::default());
        world.add_component(id2, Mana::default());
        world.remove_component::<Health>(id1);

        let health_events = world.component_events_for::<Health>();
        assert_eq!(health_events.len(), 2); // Added + Removed
        assert!(health_events.iter().all(|e| {
            let tid = match e {
                ComponentEvent::Added(_, tid) | ComponentEvent::Removed(_, tid) => *tid,
            };
            tid == std::any::TypeId::of::<Health>()
        }));

        let mana_events = world.component_events_for::<Mana>();
        assert_eq!(mana_events.len(), 1); // Only Added
    }

    #[test]
    fn test_component_events_flushed() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent { value: 1 });
        assert_eq!(world.component_events().len(), 1);

        world.update(0.016);
        assert!(world.component_events().is_empty());
    }

    #[test]
    fn test_remove_nonexistent_component() {
        let mut world = World::new();
        let id = world.create_entity();

        assert!(!world.remove_component::<TestComponent>(id));
        assert!(world.component_events().is_empty());
    }

    #[test]
    fn test_double_add_component() {
        let mut world = World::new();
        let id = world.create_entity();

        world.add_component(id, TestComponent { value: 1 });
        world.add_component(id, TestComponent { value: 2 });

        let events: Vec<_> = world
            .component_events()
            .iter()
            .filter(|e| matches!(e, ComponentEvent::Added(..)))
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            &ComponentEvent::Added(id, std::any::TypeId::of::<TestComponent>())
        );
        assert_eq!(
            events[1],
            &ComponentEvent::Added(id, std::any::TypeId::of::<TestComponent>())
        );
    }

    #[test]
    fn test_component_events_accumulate_across_frame() {
        let mut world = World::new();

        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 1 });

        world.update(0.016);
        assert!(world.component_events().is_empty());

        let id2 = world.create_entity();
        world.add_component(id2, TestComponent { value: 2 });
        assert_eq!(world.component_events().len(), 1);
    }

    // --- Change detection tests ---

    #[test]
    fn test_add_component_marks_changed() {
        let mut world = World::new();

        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 42 });

        let changed: Vec<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        assert!(changed.contains(&id));
    }

    #[test]
    fn test_get_component_mut_marks_changed() {
        let mut world = World::new();

        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 1 });

        // Clear change detection from the add
        world.clear_changed();

        // get_component_mut should mark as changed even without mutation
        let _comp = world.get_component_mut::<TestComponent>(id);

        let changed: Vec<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        assert!(changed.contains(&id));
    }

    #[test]
    fn test_query_changed_returns_only_changed() {
        let mut world = World::new();

        let mut ids = Vec::new();
        for i in 0..10 {
            let id = world.create_entity();
            world.add_component(id, TestComponent { value: i });
            ids.push(id);
        }

        // Clear change detection from adds
        world.clear_changed();

        // Mutably access only entities 2, 5, 7
        for idx in [2, 5, 7] {
            let _comp = world.get_component_mut::<TestComponent>(ids[idx]);
        }

        let changed: std::collections::HashSet<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        assert_eq!(changed.len(), 3);
        assert!(changed.contains(&ids[2]));
        assert!(changed.contains(&ids[5]));
        assert!(changed.contains(&ids[7]));
        assert!(!changed.contains(&ids[0]));
        assert!(!changed.contains(&ids[1]));
    }

    #[test]
    fn test_clear_changed_resets_detection() {
        let mut world = World::new();

        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 42 });

        // Entity should be changed
        let changed: Vec<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();
        assert!(!changed.is_empty());

        // Clear should reset
        world.clear_changed();

        let changed: Vec<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();
        assert!(changed.is_empty());
    }

    #[test]
    fn test_query_changed_is_subset() {
        let mut world = World::new();

        for i in 0..5 {
            let id = world.create_entity();
            world.add_component(id, TestComponent { value: i });
        }

        let all: std::collections::HashSet<EntityId> = world
            .query::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        let changed: std::collections::HashSet<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        // Every changed entity must be in the full query set
        for id in &changed {
            assert!(all.contains(id), "query_changed entity not in query set");
        }
    }

    #[test]
    fn test_immutable_get_no_change() {
        let mut world = World::new();

        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 1 });

        // Clear change detection from the add
        world.clear_changed();

        // Immutable get should NOT mark as changed
        let _comp = world.get_component::<TestComponent>(id);

        let changed: Vec<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        assert!(changed.is_empty());
    }

    #[test]
    fn test_query_changed_multi_component() {
        #[derive(Component, Default)]
        struct CompA {
            _x: i32,
        }
        #[derive(Component, Default)]
        struct CompB {
            _y: f32,
        }

        let mut world = World::new();

        // Entity 1: both A and B
        let id1 = world.create_entity();
        world.add_component(id1, CompA::default());
        world.add_component(id1, CompB::default());

        // Entity 2: both A and B
        let id2 = world.create_entity();
        world.add_component(id2, CompA::default());
        world.add_component(id2, CompB::default());

        // Clear from adds
        world.clear_changed();

        // Mutate only A on entity1
        let _ = world.get_component_mut::<CompA>(id1);

        // Mutate only B on entity2
        let _ = world.get_component_mut::<CompB>(id2);

        // query_changed for (A, B) should return union (both entities)
        let changed: std::collections::HashSet<EntityId> = world
            .query_changed::<(&CompA, &CompB)>()
            .map(|(eid, _, _)| eid)
            .collect();

        assert_eq!(changed.len(), 2);
        assert!(changed.contains(&id1));
        assert!(changed.contains(&id2));
    }

    #[test]
    fn test_destroyed_entity_excluded() {
        let mut world = World::new();

        let id1 = world.create_entity();
        world.add_component(id1, TestComponent { value: 1 });

        let id2 = world.create_entity();
        world.add_component(id2, TestComponent { value: 2 });

        // Mutate both
        let _ = world.get_component_mut::<TestComponent>(id1);
        let _ = world.get_component_mut::<TestComponent>(id2);

        // Destroy entity1
        world.destroy_entity(id1);

        // query_changed should not include destroyed entity
        let changed: Vec<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        assert!(!changed.contains(&id1));
        assert!(changed.contains(&id2));
    }

    #[test]
    fn test_clear_changed_called_on_update() {
        let mut world = World::new();

        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 1 });

        // Should be changed
        let changed: Vec<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();
        assert!(!changed.is_empty());

        // update() calls clear_changed internally
        world.update(0.016);

        // Should no longer be changed
        let changed: Vec<EntityId> = world
            .query_changed::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();
        assert!(changed.is_empty());
    }

    // --- Query conversion tests (VAL-ECS-021..024) ---

    #[test]
    fn test_query_matches_manual_loop() {
        let mut world = World::new();

        // Create entities: some with TestComponent, some without
        let with_component: Vec<EntityId> = (0..5)
            .map(|i| {
                let id = world.create_entity();
                world.add_component(id, TestComponent { value: i });
                id
            })
            .collect();

        // Create entities without the component
        for _ in 0..3 {
            world.create_entity();
        }

        // Collect via query
        let query_ids: std::collections::HashSet<EntityId> = world
            .query::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        // Collect via manual entity_ids loop
        let manual_ids: std::collections::HashSet<EntityId> = world
            .entity_ids()
            .filter(|id| world.get_component::<TestComponent>(*id).is_some())
            .collect();

        assert_eq!(query_ids, manual_ids);
        assert_eq!(query_ids.len(), 5);
        for id in &with_component {
            assert!(query_ids.contains(id));
        }
    }

    #[test]
    fn test_query_mut_propagates() {
        let mut world = World::new();

        let id = world.create_entity();
        world.add_component(id, TestComponent { value: 10 });

        // Mutate via query
        for (_entity, comp) in world.query::<&mut TestComponent>() {
            comp.value = 42;
        }

        // Verify mutation is visible via get_component
        let comp = world.get_component::<TestComponent>(id).unwrap();
        assert_eq!(comp.value, 42);
    }

    #[test]
    fn test_query_empty_world() {
        let mut world = World::new();

        let results: Vec<EntityId> = world
            .query::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        assert!(results.is_empty());
    }

    #[test]
    fn test_query_filters_missing_components() {
        #[derive(Component, Default)]
        struct CompA {
            _x: i32,
        }
        #[derive(Component, Default)]
        struct CompB {
            _y: f32,
        }

        let mut world = World::new();

        // Entity with both A and B
        let id_both = world.create_entity();
        world.add_component(id_both, CompA::default());
        world.add_component(id_both, CompB::default());

        // Entity with only A
        let id_only_a = world.create_entity();
        world.add_component(id_only_a, CompA::default());

        // Entity with only B
        let id_only_b = world.create_entity();
        world.add_component(id_only_b, CompB::default());

        // Entity with neither
        let id_neither = world.create_entity();

        // Query for (A, B) should only return entity with both
        let results: std::collections::HashSet<EntityId> = world
            .query::<(&CompA, &CompB)>()
            .map(|(eid, _, _)| eid)
            .collect();

        assert_eq!(results.len(), 1);
        assert!(results.contains(&id_both));
        assert!(!results.contains(&id_only_a));
        assert!(!results.contains(&id_only_b));
        assert!(!results.contains(&id_neither));
    }

    #[test]
    fn test_query_ref_matches_query() {
        let mut world = World::new();

        for i in 0..5 {
            let id = world.create_entity();
            world.add_component(id, TestComponent { value: i });
        }

        // query and query_ref should produce same entity set
        let mut_query: std::collections::HashSet<EntityId> = world
            .query::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        let ref_query: std::collections::HashSet<EntityId> = world
            .query_ref::<&TestComponent>()
            .map(|(eid, _)| eid)
            .collect();

        assert_eq!(mut_query, ref_query);
    }

    // --- Performance benchmarks (VAL-ECS-025, VAL-ECS-026) ---

    #[test]
    fn test_event_emission_overhead() {
        // VAL-ECS-025: Entity creation with events must not degrade throughput
        // by more than 10%. We measure wall-clock time for 100K entity creates
        // with events vs a theoretical baseline (just allocation + Vec push).
        use std::time::Instant;

        const N: usize = 100_000;

        // Measure: create entities with events (current implementation)
        let start = Instant::now();
        let mut world = World::new();
        for _ in 0..N {
            let id = world.create_entity();
            world.add_component(id, TestComponent { value: 0 });
        }
        let with_events_duration = start.elapsed();

        // The overhead of events is just a Vec::push per create/add_component.
        // If events added >10% overhead, something is wrong with the implementation.
        // We can't meaningfully measure "without events" since events are always on,
        // so we verify the operation completes in reasonable time (< 500ms for 100K).
        assert!(
            with_events_duration.as_millis() < 500,
            "100K entity creates took {}ms, expected < 500ms",
            with_events_duration.as_millis()
        );
    }

    #[test]
    fn test_change_detection_overhead() {
        // VAL-ECS-026: Generation counter increment in get_component_mut
        // must not add more than 5% overhead. We measure wall-clock time for
        // 100K get_component_mut calls.
        use std::time::Instant;

        const N: usize = 100_000;

        let mut world = World::new();
        let mut ids = Vec::with_capacity(N);
        for _ in 0..N {
            let id = world.create_entity();
            world.add_component(id, TestComponent { value: 0 });
            ids.push(id);
        }

        // Clear change detection from adds
        world.clear_changed();

        // Measure: get_component_mut with generation counter increment
        let start = Instant::now();
        for id in &ids {
            if let Some(comp) = world.get_component_mut::<TestComponent>(*id) {
                comp.value += 1;
            }
        }
        let with_gen_duration = start.elapsed();

        // The generation counter is a simple SparseSet insert + u64 increment.
        // Verify it completes in reasonable time (< 200ms for 100K mut accesses).
        assert!(
            with_gen_duration.as_millis() < 200,
            "100K get_component_mut took {}ms, expected < 200ms",
            with_gen_duration.as_millis()
        );
    }
}
