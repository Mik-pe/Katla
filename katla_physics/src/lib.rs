//! Collision shapes and physics components for the Katla engine.
//!
//! Provides shape types (`SphereShape`, `BoxShape`, `CapsuleShape`) and ECS
//! components (`ColliderShape`, `ColliderState`) used for broadphase and
//! narrowphase collision detection.

mod collider;
mod shape;

pub use collider::{ColliderShape, ColliderState};
pub use shape::{BoxShape, CapsuleShape, SphereShape};
