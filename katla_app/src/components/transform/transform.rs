use katla_ecs::Component;
use katla_math::{Transform, Vec3};

/// Local-space transform relative to parent
#[derive(Component, Default)]
pub struct TransformComponent {
    pub transform: Transform,
}

impl TransformComponent {
    pub fn new(transform: Transform) -> Self {
        TransformComponent { transform }
    }

    pub fn from_position(position: Vec3) -> Self {
        Self {
            transform: Transform::new_from_position(position),
        }
    }
}

/// World-space transform (computed by TransformHierarchySystem)
#[derive(Component, Default)]
pub struct WorldTransform {
    pub transform: Transform,
}

impl WorldTransform {
    pub fn new(transform: Transform) -> Self {
        WorldTransform { transform }
    }
}

/// Dirty flag for transform hierarchy optimization.
///
/// When present on an entity, indicates that this entity's local transform
/// changed and the hierarchy needs to be re-propagated.
///
/// Automatically cleared by TransformHierarchySystem after propagation.
#[derive(Component, Default)]
pub struct TransformDirty;

impl TransformDirty {
    pub fn new() -> Self {
        TransformDirty
    }
}
