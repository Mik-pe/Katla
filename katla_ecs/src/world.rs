use crate::components::Component;
use crate::entity::EntityId;
use crate::entity_allocator::EntityAllocator;
use crate::events::{ComponentEvent, EntityEvent};
use crate::resource::ResourceStorage;
use crate::scheduler::SystemScheduler;
use crate::storage::ComponentStorageManager;
use crate::system::OrderedSystem;
use std::cell::UnsafeCell;

mod queries;
mod resources;
mod systems;
mod validation;

pub use queries::QueryChangedIter;

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
    pub(crate) entities: EntityAllocator,
    /// Registry cell keeps filtered iterator pointers valid across column borrows.
    pub(crate) storage: UnsafeCell<ComponentStorageManager>,
    /// Registered systems
    systems: Vec<OrderedSystem>,
    /// Cached parallel scheduler, rebuilt when systems change
    scheduler_cache: Option<SystemScheduler>,
    /// Global resources storage
    resources: ResourceStorage,
    /// Entity lifecycle events emitted during the current frame
    entity_events: Vec<EntityEvent>,
    /// Component events emitted during the current frame
    component_events: Vec<ComponentEvent>,
    structural_epoch: u64,
    next_registration: usize,
    parallel_work_threshold: usize,
    execution_active: bool,
    clear_systems_requested: bool,
}

impl World {
    /// Creates a new empty World.
    pub fn new() -> Self {
        Self {
            entities: EntityAllocator::new(),
            storage: UnsafeCell::new(ComponentStorageManager::new()),
            systems: Vec::new(),
            scheduler_cache: None,
            resources: ResourceStorage::new(),
            entity_events: Vec::new(),
            component_events: Vec::new(),
            structural_epoch: 0,
            next_registration: 0,
            parallel_work_threshold: 32768,
            execution_active: false,
            clear_systems_requested: false,
        }
    }

    /// Creates a new entity and returns its ID.
    pub fn create_entity(&mut self) -> EntityId {
        self.advance_structural_epoch();
        let id = self.entities.allocate();
        self.entity_events.push(EntityEvent::Spawned(id));
        id
    }

    /// Spawns a new entity with a bundle of components.
    ///
    /// This is an ergonomic way to create an entity with multiple components
    /// in a single call. The bundle can be any tuple of components from size 1-8.
    ///
    /// # Example
    ///
    /// ```
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
    ///
    /// assert!(world.entity_exists(player));
    /// assert!(world.get_component::<Transform>(player).is_some());
    /// assert!(world.get_component::<Velocity>(player).is_some());
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
            self.advance_structural_epoch();
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
            self.advance_structural_epoch();
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
        if self.entities.is_valid(id) && self.storage.get_mut().remove_component::<T>(id) {
            self.advance_structural_epoch();
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
    /// ```
    /// use katla_ecs::{World, Component, EntityId};
    ///
    /// #[derive(Component)]
    /// struct TransformComponent { x: f32 }
    ///
    /// let mut world = World::new();
    /// let entity = world.spawn((TransformComponent { x: 1.0 },));
    ///
    /// // Prefer queries for iteration:
    /// for (_entity, transform) in world.query::<&TransformComponent>() {
    ///     assert_eq!(transform.x, 1.0);
    /// }
    ///
    /// // Use get_component for specific entity access:
    /// if let Some(transform) = world.get_component::<TransformComponent>(entity) {
    ///     assert_eq!(transform.x, 1.0);
    /// }
    /// ```
    pub fn get_component<T>(&self, id: EntityId) -> Option<&T>
    where
        T: Component + 'static,
    {
        if self.entities.is_valid(id) {
            // SAFETY: A shared World borrow excludes mutable component access.
            unsafe { (&*self.storage.get()).get_component::<T>(id) }
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

    /// Returns the number of entities in the world.
    pub fn entity_count(&self) -> usize {
        self.entities.live_count()
    }

    /// Clears all entities from the world.
    pub fn clear_entities(&mut self) {
        self.advance_structural_epoch();
        self.entities.clear();
        self.storage.get_mut().clear();
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
    ///
    /// Emits `EntityEvent::Destroyed` for each removed entity.
    pub fn cleanup_empty_entities(&mut self) {
        let entities_with_components: std::collections::HashSet<EntityId> =
            self.storage.get_mut().entities_with_components();

        for entity_id in self.entities.iter_live().collect::<Vec<_>>() {
            if !entities_with_components.contains(&entity_id) {
                self.advance_structural_epoch();
                self.entities.deallocate(entity_id);
                self.entity_events.push(EntityEvent::Destroyed(entity_id));
            }
        }
    }

    /// Returns the structural epoch used to invalidate cached query rows.
    pub fn structural_epoch(&self) -> u64 {
        self.structural_epoch
    }

    fn advance_structural_epoch(&mut self) {
        self.structural_epoch = self
            .structural_epoch
            .checked_add(1)
            .expect("world structural epoch exhausted");
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
