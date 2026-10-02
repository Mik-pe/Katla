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

/// An exclusive system running on the caller thread with the entire World.
///
/// Use [`TypedSystem`] for systems that may execute concurrently.
pub trait System {
    /// Runs with exclusive access to the world.
    fn update(&mut self, world: &mut World, delta_time: f32);
    /// Runs once at registration.
    fn initialize(&mut self) {}
    /// Runs when removed or the world is destroyed.
    fn shutdown(&mut self) {}
    /// Controls whether this system runs this frame.
    fn is_enabled(&self) -> bool {
        true
    }
    /// Returns the diagnostic name.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

/// A system whose access claims are derived from its sealed parameters.
///
/// Worker systems cannot contain caller-thread-only state:
///
/// ```compile_fail
/// use std::rc::Rc;
/// use katla_ecs::TypedSystem;
/// struct ThreadLocalSystem(Rc<()>);
/// impl TypedSystem for ThreadLocalSystem {
///     type Params = ();
///     fn run(&mut self, _: (), _: f32) {}
/// }
/// ```
pub trait TypedSystem: Send + 'static {
    /// Query, resource and local parameters prepared for this system.
    type Params: crate::params::SystemParam;
    /// Runs with access only to the prepared parameters.
    fn run(
        &mut self,
        params: <Self::Params as crate::params::SystemParam>::Item<'_>,
        delta_time: f32,
    );
    /// Runs once at registration.
    fn initialize(&mut self) {}
    /// Runs when removed or the world is destroyed.
    fn shutdown(&mut self) {}
    /// Controls whether this system runs this frame.
    fn is_enabled(&self) -> bool {
        true
    }
    /// Returns the diagnostic name.
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

pub(crate) type PreparedJob<'a> = Box<dyn FnOnce() + Send + 'a>;

pub(crate) trait ErasedTypedSystem: Send {
    fn initialize(&mut self);
    fn shutdown(&mut self);
    fn is_enabled(&self) -> bool;
    unsafe fn prepare<'a>(
        &'a mut self,
        context: &crate::params::ParamContext<'a>,
        dt: f32,
    ) -> PreparedJob<'a>;
    fn drain_commands(&mut self) -> Vec<crate::params::DeferredCommand>;
}

pub(crate) struct TypedAdapter<S: TypedSystem> {
    pub system: S,
    pub state: <S::Params as crate::params::SystemParam>::State,
}

impl<S: TypedSystem> ErasedTypedSystem for TypedAdapter<S> {
    fn initialize(&mut self) {
        self.system.initialize();
    }
    fn shutdown(&mut self) {
        self.system.shutdown();
    }
    fn is_enabled(&self) -> bool {
        self.system.is_enabled()
    }
    unsafe fn prepare<'a>(
        &'a mut self,
        context: &crate::params::ParamContext<'a>,
        dt: f32,
    ) -> PreparedJob<'a> {
        use crate::params::SystemParam;
        // SAFETY: The scheduler validates and holds all parameter access claims.
        let params = unsafe { S::Params::prepare(context, &mut self.state) };
        let system = &mut self.system;
        Box::new(move || system.run(params, dt))
    }
    fn drain_commands(&mut self) -> Vec<crate::params::DeferredCommand> {
        <S::Params as crate::params::SystemParam>::drain_commands(&mut self.state)
    }
}

pub(crate) enum SystemKind {
    Exclusive(Box<dyn System>),
    Typed(Box<dyn ErasedTypedSystem>),
}

pub(crate) struct OrderedSystem {
    pub system: SystemKind,
    pub order: SystemExecutionOrder,
    pub registration: usize,
    pub access: crate::params::ParamAccess,
}

impl OrderedSystem {
    pub fn shutdown(&mut self) {
        match &mut self.system {
            SystemKind::Exclusive(s) => s.shutdown(),
            SystemKind::Typed(s) => s.shutdown(),
        }
    }
    pub fn drain_commands(&mut self) -> Vec<crate::params::DeferredCommand> {
        match &mut self.system {
            SystemKind::Exclusive(_) => Vec::new(),
            SystemKind::Typed(s) => s.drain_commands(),
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
}
