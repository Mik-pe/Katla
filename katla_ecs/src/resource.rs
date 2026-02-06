use std::any::{Any, TypeId};

/// Marker trait for types that can be stored as World resources.
///
/// Resources are global data shared across all systems, like configuration,
/// asset managers, or optimization settings.
///
/// # Example
///
/// ```ignore
/// use katla_ecs::Resource;
///
/// #[derive(Resource)]
/// struct GameSettings {
///     difficulty: f32,
///     player_count: usize,
/// }
/// ```
pub trait Resource: Any + 'static {}

impl<T: Any + 'static> Resource for T {}

/// Container for storing resources of different types.
#[derive(Default)]
pub struct ResourceStorage {
    resources: HashMap<TypeId, Box<dyn Any + 'static>>,
}

use std::collections::HashMap;

impl ResourceStorage {
    /// Create a new empty resource storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a resource.
    ///
    /// If a resource of this type already exists, it will be replaced.
    pub fn insert<R: Resource>(&mut self, resource: R) {
        self.resources.insert(TypeId::of::<R>(), Box::new(resource));
    }

    /// Get a reference to a resource.
    ///
    /// Returns `None` if the resource doesn't exist.
    pub fn get<R: Resource>(&self) -> Option<&R> {
        self.resources
            .get(&TypeId::of::<R>())
            .and_then(|r| r.downcast_ref::<R>())
    }

    /// Get a mutable reference to a resource.
    ///
    /// Returns `None` if the resource doesn't exist.
    pub fn get_mut<R: Resource>(&mut self) -> Option<&mut R> {
        self.resources
            .get_mut(&TypeId::of::<R>())
            .and_then(|r| r.downcast_mut::<R>())
    }

    /// Check if a resource exists.
    pub fn contains<R: Resource>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<R>())
    }

    /// Remove a resource.
    ///
    /// Returns `None` if the resource didn't exist.
    pub fn remove<R: Resource>(&mut self) -> Option<R> {
        self.resources
            .remove(&TypeId::of::<R>())
            .and_then(|r| r.downcast::<R>().ok())
            .map(|boxed| *boxed)
    }
}
