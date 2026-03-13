//! Particle emitter component for spawning particle effects.

use katla_ecs::Component;
use katla_gfx::EmitterHandle;

/// Configuration for a particle emitter.
///
/// This component references a GPU particle emitter via its handle.
#[derive(Clone, Debug, Component)]
pub struct ParticleEmitterComponent {
    /// Handle to the GPU particle emitter
    pub handle: EmitterHandle,
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
    /// Random seed for particle initialization
    pub random_seed: u32,
}

impl ParticleEmitterComponent {
    /// Create a new particle emitter component with a handle.
    pub fn new(handle: EmitterHandle) -> Self {
        Self {
            handle,
            ..Default::default()
        }
    }

    /// Create a fire-like particle emitter configuration.
    pub fn fire(handle: EmitterHandle) -> Self {
        Self {
            handle,
            emit_rate: 100.0,
            base_lifetime: 3.0,
            color: [1.0, 0.6, 0.2, 1.0],
            base_scale: 0.15,
            velocity_magnitude: 2.0,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_cone_angle: 0.3,
            ..Default::default()
        }
    }

    /// Create a smoke-like particle emitter configuration.
    pub fn smoke(handle: EmitterHandle) -> Self {
        Self {
            handle,
            emit_rate: 30.0,
            base_lifetime: 8.0,
            color: [0.7, 0.7, 0.7, 0.5],
            base_scale: 0.2,
            velocity_magnitude: 0.5,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_cone_angle: 0.4,
            ..Default::default()
        }
    }

    /// Create a magic sparkle particle emitter configuration.
    pub fn sparkle(handle: EmitterHandle) -> Self {
        Self {
            handle,
            emit_rate: 80.0,
            base_lifetime: 2.0,
            color: [0.6, 0.8, 1.0, 1.0],
            base_scale: 0.08,
            velocity_magnitude: 1.5,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_cone_angle: 0.8,
            ..Default::default()
        }
    }
}

impl Default for ParticleEmitterComponent {
    fn default() -> Self {
        Self {
            handle: EmitterHandle::NONE,
            position: [0.0, 0.0, 0.0],
            emit_rate: 50.0,
            base_lifetime: 5.0,
            color: [1.0, 0.8, 0.3, 1.0],
            base_scale: 0.1,
            velocity_magnitude: 1.0,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_cone_angle: 0.5,
            random_seed: 42,
        }
    }
}
