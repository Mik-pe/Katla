//! Collision shapes and physics components for the Katla engine.
//!
//! Provides shape types (`SphereShape`, `BoxShape`, `CapsuleShape`) and ECS
//! components (`ColliderShape`, `CollisionFilter`) used for broadphase and
//! narrowphase collision detection.

mod collider;
mod joint;
mod material;
mod physics_world;
mod rigid_body;
mod shape;
mod trigger;

pub use collider::{ColliderShape, CollisionFilter};
pub use joint::{Joint, JointLimits, JointType};
pub use material::PhysicsMaterial;
pub use physics_world::{PhysicsWorld, RayHit};
pub use rapier3d::dynamics::ImpulseJointHandle;
pub use rapier3d::dynamics::RigidBodyHandle;
pub use rapier3d::geometry::ColliderHandle;
pub use rigid_body::{BodyType, RigidBody};
pub use shape::{BoxShape, CapsuleShape, SphereShape};
pub use trigger::{TriggerEvent, TriggerVolume};

/// Resource that controls whether the physics simulation steps.
///
/// Insert into the ECS World to gate physics stepping behind play mode.
/// Defaults to `false` (simulation paused).
#[derive(Debug, Clone, Copy)]
pub struct PhysicsActive(pub bool);
