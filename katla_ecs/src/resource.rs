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

#[cfg(test)]
mod tests {
    use super::*;

    // Test resource types
    #[derive(Debug, PartialEq)]
    struct TestResource {
        value: i32,
    }

    #[derive(Debug, PartialEq, Clone)]
    struct AnotherTestResource {
        name: String,
    }

    #[derive(Debug, PartialEq)]
    struct ComplexResource {
        numbers: Vec<i32>,
        config: (bool, f32),
    }

    #[test]
    fn test_resource_storage_new() {
        let storage = ResourceStorage::new();
        assert!(storage.resources.is_empty());
    }

    #[test]
    fn test_resource_storage_default() {
        let storage = ResourceStorage::default();
        assert!(storage.resources.is_empty());
    }

    #[test]
    fn test_insert_and_get() {
        let mut storage = ResourceStorage::new();
        let resource = TestResource { value: 42 };

        storage.insert(resource);
        let retrieved = storage.get::<TestResource>();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &TestResource { value: 42 });
    }

    #[test]
    fn test_insert_replaces_existing() {
        let mut storage = ResourceStorage::new();

        storage.insert(TestResource { value: 10 });
        storage.insert(TestResource { value: 20 });

        let retrieved = storage.get::<TestResource>();
        assert_eq!(retrieved.unwrap().value, 20);
    }

    #[test]
    fn test_get_none_when_not_inserted() {
        let storage = ResourceStorage::new();
        assert!(storage.get::<TestResource>().is_none());
    }

    #[test]
    fn test_get_mut_and_modify() {
        let mut storage = ResourceStorage::new();
        storage.insert(TestResource { value: 10 });

        {
            let resource = storage.get_mut::<TestResource>();
            assert!(resource.is_some());
            resource.unwrap().value = 999;
        }

        let retrieved = storage.get::<TestResource>();
        assert_eq!(retrieved.unwrap().value, 999);
    }

    #[test]
    fn test_get_mut_none_when_not_inserted() {
        let mut storage = ResourceStorage::new();
        assert!(storage.get_mut::<TestResource>().is_none());
    }

    #[test]
    fn test_contains_true_when_present() {
        let mut storage = ResourceStorage::new();
        storage.insert(TestResource { value: 42 });
        assert!(storage.contains::<TestResource>());
    }

    #[test]
    fn test_contains_false_when_absent() {
        let storage = ResourceStorage::new();
        assert!(!storage.contains::<TestResource>());
    }

    #[test]
    fn test_remove_returns_resource() {
        let mut storage = ResourceStorage::new();
        storage.insert(TestResource { value: 42 });

        let removed = storage.remove::<TestResource>();
        assert_eq!(removed, Some(TestResource { value: 42 }));
        assert!(!storage.contains::<TestResource>());
    }

    #[test]
    fn test_remove_none_when_absent() {
        let mut storage = ResourceStorage::new();
        assert!(storage.remove::<TestResource>().is_none());
    }

    #[test]
    fn test_multiple_resources_different_types() {
        let mut storage = ResourceStorage::new();

        storage.insert(TestResource { value: 42 });
        storage.insert(AnotherTestResource {
            name: "test".to_string(),
        });

        assert_eq!(
            storage.get::<TestResource>().unwrap(),
            &TestResource { value: 42 }
        );
        assert_eq!(
            storage.get::<AnotherTestResource>().unwrap().name,
            "test"
        );
    }

    #[test]
    fn test_multiple_resources_independent() {
        let mut storage = ResourceStorage::new();

        storage.insert(TestResource { value: 10 });
        storage.insert(AnotherTestResource {
            name: "resource1".to_string(),
        });

        // Modify one resource
        if let Some(res) = storage.get_mut::<TestResource>() {
            res.value = 20;
        }

        // Verify other resource is unchanged
        assert_eq!(
            storage.get::<AnotherTestResource>().unwrap().name,
            "resource1"
        );
        assert_eq!(storage.get::<TestResource>().unwrap().value, 20);
    }

    #[test]
    fn test_complex_resource() {
        let mut storage = ResourceStorage::new();

        let resource = ComplexResource {
            numbers: vec![1, 2, 3],
            config: (true, 3.14),
        };

        storage.insert(resource);

        let retrieved = storage.get::<ComplexResource>();
        assert!(retrieved.is_some());
        let res = retrieved.unwrap();
        assert_eq!(res.numbers, vec![1, 2, 3]);
        assert_eq!(res.config, (true, 3.14));
    }

    #[test]
    fn test_clone_resource_before_insert() {
        let mut storage = ResourceStorage::new();

        let resource = AnotherTestResource {
            name: "original".to_string(),
        };

        storage.insert(resource.clone());

        // Modify original
        let mut original = resource;
        original.name = "modified".to_string();

        // Storage should have original value
        assert_eq!(
            storage.get::<AnotherTestResource>().unwrap().name,
            "original"
        );
    }

    #[test]
    fn test_multiple_remove_and_reinsert() {
        let mut storage = ResourceStorage::new();

        // First insert
        storage.insert(TestResource { value: 10 });
        assert!(storage.contains::<TestResource>());

        // Remove
        storage.remove::<TestResource>();
        assert!(!storage.contains::<TestResource>());

        // Re-insert
        storage.insert(TestResource { value: 20 });
        assert!(storage.contains::<TestResource>());
        assert_eq!(storage.get::<TestResource>().unwrap().value, 20);
    }

    #[test]
    fn test_string_resource() {
        let mut storage = ResourceStorage::new();

        storage.insert("Hello, World!".to_string());

        let retrieved = storage.get::<String>();
        assert_eq!(retrieved.unwrap(), "Hello, World!");
    }

    #[test]
    fn test_vec_resource() {
        let mut storage = ResourceStorage::new();

        storage.insert(vec![1, 2, 3, 4, 5]);

        let retrieved = storage.get::<Vec<i32>>();
        assert_eq!(retrieved.unwrap(), &vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_option_resource() {
        let mut storage = ResourceStorage::new();

        storage.insert(Some(42));

        let retrieved = storage.get::<Option<i32>>();
        assert_eq!(retrieved.unwrap(), &Some(42));
    }
}
