//! Physics material component.

use katla_ecs::Component;
use serde::{Deserialize, Serialize};

/// Physical material properties for a collider.
///
/// Mapped to Rapier's `CoefficientCombineRule` and material coefficients
/// when the collider is created by `RapierPhysicsSystem`.
#[derive(Component, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PhysicsMaterial {
    /// Friction coefficient (0.0 = ice, 1.0 = rubber).
    pub friction: f32,
    /// Restitution (bounciness, 0.0 = no bounce, 1.0 = perfect bounce).
    pub restitution: f32,
    /// Density in kg/m^3. Used to compute mass for dynamic bodies.
    pub density: f32,
}

impl Default for PhysicsMaterial {
    fn default() -> Self {
        Self {
            friction: 0.5,
            restitution: 0.0,
            density: 1.0,
        }
    }
}

impl PhysicsMaterial {
    pub fn new(friction: f32, restitution: f32, density: f32) -> Self {
        Self {
            friction,
            restitution,
            density,
        }
    }
}
