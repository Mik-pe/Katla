use super::storage::ComponentStorageManager;

/// System trait for the ECS framework.
///
/// Systems contain the logic that operates on entities with specific components.
/// In this architecture, systems work directly with component storages for better
/// cache locality and performance.
///
/// # Examples
///
/// ```
/// use katla_ecs::{System, ComponentStorageManager};
///
/// struct PhysicsSystem;
///
/// impl System for PhysicsSystem {
///     fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
///         // Update physics-related components...
///     }
/// }
/// ```
pub trait System {
    /// Update logic for this system.
    ///
    /// Called once per frame with access to all component storages.
    ///
    /// # Arguments
    ///
    /// * `storage` - Mutable reference to the component storage manager
    /// * `delta_time` - Time elapsed since the last frame in seconds
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32);

    /// Optional initialization logic.
    ///
    /// Called once when the system is registered with the world.
    fn initialize(&mut self) {}

    /// Optional cleanup logic.
    ///
    /// Called when the system is removed or the world is destroyed.
    fn shutdown(&mut self) {}

    /// Returns whether this system should be updated.
    ///
    /// Can be used to enable/disable systems at runtime.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Returns the name of this system for debugging purposes.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

/// SystemExecutionOrder defines the relative order in which systems should execute.
///
/// Systems with lower order values execute before systems with higher order values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SystemExecutionOrder(pub i32);

impl SystemExecutionOrder {
    pub const FIRST: SystemExecutionOrder = SystemExecutionOrder(i32::MIN);
    pub const EARLY: SystemExecutionOrder = SystemExecutionOrder(-1000);
    pub const NORMAL: SystemExecutionOrder = SystemExecutionOrder(0);
    pub const LATE: SystemExecutionOrder = SystemExecutionOrder(1000);
    pub const LAST: SystemExecutionOrder = SystemExecutionOrder(i32::MAX);
}

impl Default for SystemExecutionOrder {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// A wrapper that associates a System with its execution order.
pub struct OrderedSystem {
    pub system: Box<dyn System>,
    pub order: SystemExecutionOrder,
}

impl OrderedSystem {
    pub fn new(system: Box<dyn System>, order: SystemExecutionOrder) -> Self {
        Self { system, order }
    }
}

#[cfg(test)]
mod tests {
    use crate::Component;

    use super::*;
    use crate::entity::EntityId;
    use crate::storage::ComponentStorageManager;

    #[derive(Component)]
    struct TestComponent {}

    impl TestComponent {
        fn new() -> Self {
            Self {}
        }
    }

    struct TestSystem {
        update_count: u32,
    }

    impl TestSystem {
        fn new() -> Self {
            Self { update_count: 0 }
        }
    }

    impl System for TestSystem {
        fn update(&mut self, storage: &mut ComponentStorageManager, _delta_time: f32) {
            self.update_count += 1;

            // Access transform storage
            if let Some(transforms) = storage.get_storage::<TestComponent>() {
                let _count = transforms.len();
            }
        }
    }

    #[test]
    fn test_system_creation() {
        let system = TestSystem::new();
        assert_eq!(system.update_count, 0);
        assert!(system.is_enabled());
    }

    #[test]
    fn test_system_update() {
        let mut system = TestSystem::new();
        let mut storage = ComponentStorageManager::new();

        storage.add_component(EntityId::new(1), TestComponent::new());

        system.update(&mut storage, 0.016);

        assert_eq!(system.update_count, 1);
    }

    #[test]
    fn test_execution_order() {
        assert!(SystemExecutionOrder::FIRST < SystemExecutionOrder::EARLY);
        assert!(SystemExecutionOrder::EARLY < SystemExecutionOrder::NORMAL);
        assert!(SystemExecutionOrder::NORMAL < SystemExecutionOrder::LATE);
        assert!(SystemExecutionOrder::LATE < SystemExecutionOrder::LAST);
    }

    #[test]
    fn test_ordered_system() {
        let system = Box::new(TestSystem::new());
        let ordered = OrderedSystem::new(system, SystemExecutionOrder::EARLY);
        assert_eq!(ordered.order, SystemExecutionOrder::EARLY);
    }
}
