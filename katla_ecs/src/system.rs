use std::any::TypeId;

use crate::World;

/// Describes how a system accesses a component type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentAccess {
    /// System reads the component (immutable access).
    Read(TypeId),
    /// System writes the component (mutable access).
    Write(TypeId),
}

impl ComponentAccess {
    pub fn read<T: 'static>() -> Self {
        ComponentAccess::Read(TypeId::of::<T>())
    }

    pub fn write<T: 'static>() -> Self {
        ComponentAccess::Write(TypeId::of::<T>())
    }
}

/// Describes how a system accesses a resource type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceAccess {
    /// System reads the resource (immutable access).
    Read(TypeId),
    /// System writes the resource (mutable access).
    Write(TypeId),
}

impl ResourceAccess {
    pub fn read<T: 'static>() -> Self {
        ResourceAccess::Read(TypeId::of::<T>())
    }

    pub fn write<T: 'static>() -> Self {
        ResourceAccess::Write(TypeId::of::<T>())
    }
}

/// System trait for the ECS framework.
///
/// Systems contain the logic that operates on entities with specific components.
/// In this architecture, systems work directly with component storages for better
/// cache locality and performance.
///
/// # Parallel Safety
///
/// When using [`World::update_parallel`](crate::World::update_parallel), systems
/// that access components **MUST** override [`component_access()`](System::component_access)
/// and [`component_access_dyn()`](System::component_access_dyn) to declare their
/// read/write patterns. A system that forgets to override these methods defaults to
/// "no declared access" and the scheduler will assume it is safe to run in parallel
/// with any other system — which can cause data races if the system actually reads
/// or writes components.
///
/// # Examples
///
/// ```
/// use katla_ecs::{System, World};
///
/// struct PhysicsSystem;
///
/// impl System for PhysicsSystem {
///     fn update(&mut self, world: &mut World, delta_time: f32) {
///         // Update physics-related components...
///     }
/// }
/// ```
pub trait System {
    /// Update logic for this system.
    ///
    /// Called once per frame with access to the entire world.
    /// This allows systems to read input state and also access all component storages.
    ///
    /// # Arguments
    ///
    /// * `world` - Mutable reference to the world
    /// * `delta_time` - Time elapsed since the last frame in seconds
    fn update(&mut self, world: &mut World, delta_time: f32);

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

    /// Returns the component access patterns for this system.
    ///
    /// Override this to declare which components your system reads or writes.
    /// Used by the parallel scheduler to detect conflicts and run independent
    /// systems concurrently.
    ///
    /// # Example
    ///
    /// ```
    /// use katla_ecs::{System, World, ComponentAccess, SystemExecutionOrder};
    /// use katla_ecs::Component;
    ///
    /// #[derive(Component)]
    /// struct Position { x: f32, y: f32 }
    ///
    /// #[derive(Component)]
    /// struct Velocity { dx: f32, dy: f32 }
    ///
    /// struct MovementSystem;
    ///
    /// impl System for MovementSystem {
    ///     fn update(&mut self, world: &mut World, dt: f32) { /* ... */ }
    ///
    ///     fn component_access() -> Vec<ComponentAccess>
    ///     where Self: Sized
    ///     {
    ///         vec![
    ///             ComponentAccess::write::<Position>(),
    ///             ComponentAccess::read::<Velocity>(),
    ///         ]
    ///     }
    /// }
    /// ```
    fn component_access() -> Vec<ComponentAccess>
    where
        Self: Sized,
    {
        Vec::new()
    }

    /// Trait-object-compatible version of [`component_access`](System::component_access).
    ///
    /// Returns the access patterns for this system. Concrete types that override
    /// `component_access()` should also override this to return the same value.
    ///
    /// **Warning:** The default returns an empty vec (no declared access). Systems
    /// that access components MUST override both this method and `component_access()`
    /// for safe parallel execution — otherwise the scheduler will assume the system
    /// has no conflicts and may run it concurrently with systems that access the same
    /// components, causing data races.
    fn component_access_dyn(&self) -> Vec<ComponentAccess> {
        Vec::new()
    }

    /// Returns the resource access patterns for this system.
    ///
    /// Override this to declare which resources your system reads or writes.
    /// Used by the parallel scheduler to detect conflicts and run independent
    /// systems concurrently.
    fn resource_access() -> Vec<ResourceAccess>
    where
        Self: Sized,
    {
        Vec::new()
    }

    /// Trait-object-compatible version of [`resource_access`](System::resource_access).
    ///
    /// Returns the resource access patterns for this system. Concrete types that override
    /// `resource_access()` should also override this to return the same value.
    fn resource_access_dyn(&self) -> Vec<ResourceAccess> {
        Vec::new()
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
    pub access_patterns: Vec<ComponentAccess>,
}

impl OrderedSystem {
    pub fn new(system: Box<dyn System>, order: SystemExecutionOrder) -> Self {
        Self {
            system,
            order,
            access_patterns: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Component, EntityId};

    use super::*;

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
        fn update(&mut self, world: &mut World, _delta_time: f32) {
            self.update_count += 1;

            // Access component storage via the world
            if world
                .get_component::<TestComponent>(EntityId::test_new(0))
                .is_some()
            {
                let _count = 1;
            }
        }
    }

    #[test]
    fn test_system_update() {
        let mut system = TestSystem::new();
        let mut world = World::new();

        let entity = world.create_entity();
        world.add_component(entity, TestComponent::new());

        system.update(&mut world, 0.016);

        assert_eq!(system.update_count, 1);
    }

    #[test]
    fn test_component_access_read() {
        let access = ComponentAccess::read::<TestComponent>();
        assert_eq!(access, ComponentAccess::Read(TypeId::of::<TestComponent>()));
    }

    #[test]
    fn test_component_access_write() {
        let access = ComponentAccess::write::<TestComponent>();
        assert_eq!(
            access,
            ComponentAccess::Write(TypeId::of::<TestComponent>())
        );
    }

    #[test]
    fn test_default_component_access_is_empty() {
        let access = <TestSystem as System>::component_access();
        assert!(access.is_empty());
    }
}
