//! Particle operations implemented by an application-owned scene service.

use crate::particles::{EmitterConfig, EmitterHandle};

/// Operations the frame driver needs from a particle system.
pub trait ParticleEmitterDriver {
    fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String>;
    fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig);
    fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool);
    fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String>;
}
