//! Rigid body component for physics simulation.

use katla_ecs::Component;
use rapier3d::dynamics::RigidBodyHandle;
use rapier3d::geometry::ColliderHandle;
use serde::{Deserialize, Serialize};

/// The type of a rigid body, determining how it participates in simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BodyType {
    /// Immovable object (walls, floors). Affected by gravity but cannot move.
    #[default]
    Static,
    /// Fully simulated body affected by forces, collisions, and gravity.
    Dynamic,
    /// Controlled directly by the game (platforms, doors). Not affected by forces.
    Kinematic,
}

/// Component that marks an entity as participating in Rapier physics simulation.
///
/// When added alongside a `ColliderShape`, the `RapierPhysicsSystem` will create
/// a corresponding Rapier rigid body and collider in the `PhysicsWorld`.
///
/// The Rapier handles are stored here after creation and used for sync.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct RigidBody {
    /// Whether this body is static, dynamic, or kinematic.
    pub body_type: BodyType,
    /// Additional gravity scale (1.0 = normal, 0.0 = no gravity, 2.0 = double).
    pub gravity_scale: f32,
    /// Linear velocity in world space (read from Rapier after step).
    #[serde(skip)]
    pub linear_velocity: katla_math::Vec3,
    /// Rapier rigid body handle (set by physics system, not serialized).
    #[serde(skip)]
    pub body_handle: Option<RigidBodyHandle>,
    /// Rapier collider handle (set by physics system, not serialized).
    #[serde(skip)]
    pub collider_handle: Option<ColliderHandle>,
}

impl RigidBody {
    pub fn dynamic() -> Self {
        Self {
            body_type: BodyType::Dynamic,
            gravity_scale: 1.0,
            linear_velocity: katla_math::Vec3::default(),
            body_handle: None,
            collider_handle: None,
        }
    }

    pub fn static_body() -> Self {
        Self {
            body_type: BodyType::Static,
            gravity_scale: 1.0,
            linear_velocity: katla_math::Vec3::default(),
            body_handle: None,
            collider_handle: None,
        }
    }

    pub fn kinematic() -> Self {
        Self {
            body_type: BodyType::Kinematic,
            gravity_scale: 1.0,
            linear_velocity: katla_math::Vec3::default(),
            body_handle: None,
            collider_handle: None,
        }
    }

    pub fn with_gravity_scale(mut self, scale: f32) -> Self {
        self.gravity_scale = scale;
        self
    }

    pub fn is_spawned(&self) -> bool {
        self.body_handle.is_some()
    }
}
