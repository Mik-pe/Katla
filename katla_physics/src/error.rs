use rapier3d::dynamics::{ImpulseJointHandle, RigidBodyHandle};
use rapier3d::geometry::ColliderHandle;

/// Errors returned by `PhysicsWorld` operations.
#[derive(Debug)]
pub enum PhysicsError {
    BodyNotFound(RigidBodyHandle),
    ColliderNotFound(ColliderHandle),
    JointNotFound(ImpulseJointHandle),
    InvalidHandle(String),
    Other(String),
}

impl std::fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhysicsError::BodyNotFound(handle) => {
                write!(f, "rigid body not found (handle: {handle:?})")
            }
            PhysicsError::ColliderNotFound(handle) => {
                write!(f, "collider not found (handle: {handle:?})")
            }
            PhysicsError::JointNotFound(handle) => {
                write!(f, "joint not found (handle: {handle:?})")
            }
            PhysicsError::InvalidHandle(msg) => write!(f, "invalid handle: {msg}"),
            PhysicsError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for PhysicsError {}
