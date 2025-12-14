use std::any::Any;

// Re-export the derive macro for convenience
pub use katla_derive::Component;

/// Core Component trait for the ECS framework.
///
/// Components are pure data containers that can be attached to entities.
/// Systems operate on entities that have specific combinations of components.
///
/// # Examples
///
/// ```
/// use katla_ecs::Component;
///
/// #[derive(Component)]
/// struct HealthComponent {
///     current: f32,
///     max: f32,
/// }
/// ```
pub trait Component: Any {
    /// Converts the component to an Any reference for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Converts the component to a mutable Any reference for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
