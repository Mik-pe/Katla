//! Global resource access owned by the world.

use super::World;
use crate::Resource;

impl World {
    /// Insert a resource into the world.
    ///
    /// If a resource of this type already exists, it will be replaced.
    ///
    /// # Example
    ///
    /// ```
    /// use katla_ecs::{World, Resource};
    ///
    /// struct GameSettings { difficulty: f32 }
    ///
    /// let mut world = World::new();
    /// world.insert_resource(GameSettings { difficulty: 1.0 });
    ///
    /// assert!(world.contains_resource::<GameSettings>());
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

    /// Get a mutable reference to a resource, inserting a default if it doesn't exist.
    ///
    /// This is the equivalent of `entry().or_insert_with()` for resources.
    /// Prefer this over `contains_resource` + `get_resource_mut` + `unwrap`.
    pub fn get_resource_mut_or_insert_with<R: Resource + Default>(&mut self) -> &mut R {
        if !self.resources.contains::<R>() {
            self.resources.insert(R::default());
        }
        self.resources
            .get_mut::<R>()
            .expect("resource was just inserted above")
    }
}
