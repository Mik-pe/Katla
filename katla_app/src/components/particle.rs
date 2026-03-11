//! Particle emitter component for spawning particle effects.

use katla_ecs::Component;

/// Configuration for a particle emitter.
///
/// This component defines the properties of a particle emitter
/// that can be attached to any entity in the scene.
#[derive(Clone, Debug, Component)]
pub struct ParticleEmitterComponent {
    /// Position of the emitter in world space
    pub position: [f32; 3],
    /// Particles to emit per second
    pub emit_rate: f32,
    /// Base lifetime for particles in seconds
    pub base_lifetime: f32,
    /// Base color for particles (RGBA)
    pub color: [f32; 4],
    /// Base scale for particles
    pub base_scale: f32,
    /// Velocity magnitude
    pub velocity_magnitude: f32,
    /// Velocity direction (normalized)
    pub velocity_direction: [f32; 3],
    /// Velocity spread angle (0 = straight, PI/2 = hemisphere)
    pub velocity_cone_angle: f32,
}

impl Default for ParticleEmitterComponent {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            emit_rate: 50.0,
            base_lifetime: 5.0,
            color: [1.0, 0.8, 0.3, 1.0], // Orange/yellow fire color
            base_scale: 0.1,
            velocity_magnitude: 1.0,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_cone_angle: 0.5,
        }
    }
}

impl ParticleEmitterComponent {
    /// Create a new particle emitter with the given position.
    pub fn new(position: [f32; 3]) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }

    /// Create a fire-like particle emitter.
    pub fn fire(position: [f32; 3]) -> Self {
        Self {
            position,
            emit_rate: 100.0,
            base_lifetime: 3.0,
            color: [1.0, 0.6, 0.2, 1.0],
            base_scale: 0.15,
            velocity_magnitude: 2.0,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_cone_angle: 0.3,
        }
    }

    /// Create a smoke-like particle emitter.
    pub fn smoke(position: [f32; 3]) -> Self {
        Self {
            position,
            emit_rate: 30.0,
            base_lifetime: 8.0,
            color: [0.7, 0.7, 0.7, 0.5],
            base_scale: 0.2,
            velocity_magnitude: 0.5,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_cone_angle: 0.4,
        }
    }

    /// Create a magic sparkle particle emitter.
    pub fn sparkle(position: [f32; 3]) -> Self {
        Self {
            position,
            emit_rate: 80.0,
            base_lifetime: 2.0,
            color: [0.6, 0.8, 1.0, 1.0],
            base_scale: 0.08,
            velocity_magnitude: 1.5,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_cone_angle: 0.8,
        }
    }
}
