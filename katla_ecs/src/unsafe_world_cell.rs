use crate::entity_allocator::EntityAllocator;
use crate::storage::ComponentStorageManager;
use crate::world::World;
use std::cell::UnsafeCell;

/// Thin wrapper around `*mut World` for scoped unsafe access to World data.
///
/// This is similar to Bevy's `UnsafeWorldCell` — it provides methods for
/// reading and writing component storage through a raw pointer. The caller
/// is responsible for ensuring no aliasing violations (e.g., only one mutable
/// reference per component type at a time).
///
/// This is the building block for parallel system execution: the scheduler
/// can hand out `UnsafeWorldCell` references to systems that access disjoint
/// component types.
#[derive(Copy, Clone)]
pub(crate) struct UnsafeWorldCell(*mut World);

impl UnsafeWorldCell {
    /// Create from a raw World pointer.
    ///
    /// # Safety
    /// Caller must ensure `world` is valid, properly aligned, and no other
    /// `&mut World` reference exists for the duration of the returned cell's use.
    #[inline]
    pub unsafe fn new(world: *mut World) -> Self {
        Self(world)
    }

    /// Get the raw `*mut World` pointer.
    #[inline]
    pub fn as_ptr(&self) -> *mut World {
        self.0
    }

    /// Get an immutable reference to the component storage.
    ///
    /// # Safety
    /// Caller must ensure no mutable reference to the same storage exists
    /// at the same time.
    #[inline]
    #[allow(dead_code)]
    pub unsafe fn storage(&self) -> &ComponentStorageManager {
        // SAFETY: Caller guarantees no concurrent mutable reference to storage.
        unsafe { &*(*self.0).storage.get() }
    }

    /// Get a mutable reference to a specific component type's storage.
    ///
    /// # Safety
    /// Caller must ensure no other reference (mutable or immutable) to this
    /// component type's storage exists.
    #[allow(clippy::mut_from_ref)]
    #[inline]
    #[allow(dead_code)]
    pub unsafe fn storage_mut<T: crate::components::Component + 'static>(
        &self,
    ) -> Option<&mut crate::storage::ComponentStorage<T>> {
        // SAFETY: Caller guarantees exclusive access to this component type's storage.
        unsafe { (*(*self.0).storage.get()).get_storage_mut::<T>() }
    }

    /// Get an immutable reference to entity data.
    ///
    /// Entity data is never mutated during system execution, so this is safe.
    #[inline]
    #[allow(dead_code)]
    pub fn entities(&self) -> &EntityAllocator {
        // SAFETY: entities is never mutated during system execution.
        // It is only modified through &mut World methods like create_entity/destroy_entity.
        unsafe { &(*self.0).entities }
    }

    /// Get the world as a shared reference.
    ///
    /// # Safety
    /// Caller must ensure no mutable access to the world is happening concurrently.
    #[inline]
    #[allow(dead_code)]
    pub unsafe fn world(&self) -> &World {
        // SAFETY: Caller guarantees no concurrent mutable access.
        unsafe { &*self.0 }
    }

    /// Get a mutable reference to the `UnsafeCell<ComponentStorageManager>`.
    ///
    /// Returns the raw `UnsafeCell` so callers can derive both `&` and `&mut`
    /// storage references at the appropriate safety boundaries.
    ///
    /// # Safety
    /// Caller must ensure no aliasing violations when accessing through the
    /// returned cell.
    #[inline]
    #[allow(dead_code)]
    pub unsafe fn storage_cell(&self) -> &UnsafeCell<ComponentStorageManager> {
        // SAFETY: Caller guarantees proper access discipline.
        unsafe { &(*self.0).storage }
    }
}

// SAFETY: UnsafeWorldCell is explicitly designed for concurrent read access
// from multiple threads (each accessing different component types). The caller
// is responsible for ensuring no two threads mutate the same data simultaneously.
unsafe impl Send for UnsafeWorldCell {}
unsafe impl Sync for UnsafeWorldCell {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Component;

    #[derive(Component, Default, PartialEq, Debug)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Component, Default, PartialEq, Debug)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }

    #[test]
    fn test_create_from_world() {
        let mut world = World::new();
        let cell = unsafe { world.as_unsafe_world_cell() };
        // Just verify we can create one without panic
        let _ = cell;
    }

    #[test]
    fn test_read_entities() {
        let mut world = World::new();
        let id1 = world.create_entity();
        let id2 = world.create_entity();

        let cell = unsafe { world.as_unsafe_world_cell() };
        let entities = cell.entities();

        assert!(entities.is_valid(id1));
        assert!(entities.is_valid(id2));
        assert_eq!(entities.live_count(), 2);
    }

    #[test]
    fn test_read_component_storage() {
        let mut world = World::new();
        let id = world.create_entity();
        world.add_component(id, Position { x: 1.0, y: 2.0 });

        let cell = unsafe { world.as_unsafe_world_cell() };
        let storage = unsafe { cell.storage() };
        let pos = storage.get_component::<Position>(id);
        assert!(pos.is_some());
        assert_eq!(pos.unwrap().x, 1.0);
        assert_eq!(pos.unwrap().y, 2.0);
    }

    #[test]
    fn test_storage_mut_returns_correct_type() {
        let mut world = World::new();
        let id = world.create_entity();
        world.add_component(id, Position { x: 5.0, y: 10.0 });
        world.add_component(id, Velocity { dx: 1.0, dy: -1.0 });

        let cell = unsafe { world.as_unsafe_world_cell() };

        // Get mutable storage for Position
        {
            let pos_storage = unsafe { cell.storage_mut::<Position>() }.unwrap();
            let pos = pos_storage.get(id).unwrap();
            assert_eq!(pos.x, 5.0);
        }

        // Get mutable storage for Velocity
        {
            let vel_storage = unsafe { cell.storage_mut::<Velocity>() }.unwrap();
            let vel = vel_storage.get(id).unwrap();
            assert_eq!(vel.dx, 1.0);
        }
    }

    #[test]
    fn test_storage_mut_missing_type_returns_none() {
        let mut world = World::new();
        let id = world.create_entity();
        world.add_component(id, Position { x: 1.0, y: 2.0 });

        let cell = unsafe { world.as_unsafe_world_cell() };
        let vel_storage = unsafe { cell.storage_mut::<Velocity>() };
        assert!(vel_storage.is_none());
    }

    #[test]
    fn test_world_reference() {
        let mut world = World::new();
        let id = world.create_entity();
        world.add_component(id, Position { x: 3.0, y: 4.0 });

        let cell = unsafe { world.as_unsafe_world_cell() };
        let world_ref = unsafe { cell.world() };

        assert!(world_ref.entity_exists(id));
        assert_eq!(world_ref.entity_count(), 1);
    }

    #[test]
    fn test_storage_cell() {
        let mut world = World::new();
        let id = world.create_entity();
        world.add_component(id, Position { x: 7.0, y: 8.0 });

        let cell = unsafe { world.as_unsafe_world_cell() };
        let storage_cell = unsafe { cell.storage_cell() };

        // SAFETY: No other references exist, so we can safely read.
        let storage = unsafe { &*storage_cell.get() };
        let pos = storage.get_component::<Position>(id);
        assert!(pos.is_some());
        assert_eq!(pos.unwrap().x, 7.0);
    }
}
