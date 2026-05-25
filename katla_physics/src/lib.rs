//! Collision shapes and physics components for the Katla engine.
//!
//! Provides shape types (`SphereShape`, `BoxShape`, `CapsuleShape`) and ECS
//! components (`ColliderShape`, `ColliderState`) used for broadphase and
//! narrowphase collision detection.

mod collider;
mod material;
mod physics_world;
mod rigid_body;
mod shape;

pub use collider::{ColliderShape, ColliderState, CollisionFilter};
pub use material::PhysicsMaterial;
pub use physics_world::{PhysicsWorld, RayHit};
pub use rapier3d::dynamics::RigidBodyHandle;
pub use rapier3d::geometry::ColliderHandle;
pub use rigid_body::{BodyType, RigidBody};
pub use shape::{BoxShape, CapsuleShape, SphereShape};
