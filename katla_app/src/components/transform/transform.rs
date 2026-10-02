use katla_ecs::Component;
use katla_math::{Mat4, Transform, Vec3};

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
#[derive(Component, Clone, Copy)]
pub struct WorldTransform {
    /// World position, accumulated rotation and scale. Shear is stored in `matrix`.
    pub transform: Transform,
    /// Exact parent-to-child matrix composition, including nonuniform scale and shear.
    pub matrix: Mat4,
}

impl WorldTransform {
    pub fn new(transform: Transform) -> Self {
        WorldTransform {
            matrix: transform.make_mat4(),
            transform,
        }
    }
}

impl Default for WorldTransform {
    fn default() -> Self {
        Self::new(Transform::default())
    }
}

/// Dirty flag requesting a world-transform refresh.
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
