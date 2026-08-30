//! Backend-agnostic particle-system driver.
//!
//! Lets engine consumers (the ECS emitter system, editor actions) operate the
//! Vulkan `GlobalParticleSystem` and the Metal `MetalParticleSubsystem` through
//! one interface. `MetalRenderer` implements this by delegating into its
//! optional particle subsystem.

use super::GlobalParticleSystem;
use crate::particles::{EmitterConfig, EmitterHandle};

/// Operations the frame driver needs from a particle system.
pub trait ParticleEmitterDriver {
    fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String>;
    fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig);
    fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool);
    fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String>;
}

impl ParticleEmitterDriver for crate::particles::GlobalParticleSystem {
    fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        GlobalParticleSystem::create_emitter(self, config).map_err(|e| e.to_string())
    }

    fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        GlobalParticleSystem::update_emitter(self, handle, config)
    }

    fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool) {
        GlobalParticleSystem::destroy_emitter(self, handle, kill_all)
    }

    fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String> {
        GlobalParticleSystem::burst(self, handle, count).map_err(|e| e.to_string())
    }
}

impl ParticleEmitterDriver for crate::MetalRenderer {
    fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        match self.particle_system.as_mut() {
            Some(ps) => ps.create_emitter(config),
            None => Err("Metal particle subsystem not initialized".to_string()),
        }
    }

    fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        if let Some(ps) = self.particle_system.as_mut() {
            ps.update_emitter(handle, config);
        }
    }

    fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool) {
        if let Some(ps) = self.particle_system.as_mut() {
            ps.destroy_emitter(handle, kill_all);
        }
    }

    fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String> {
        match self.particle_system.as_mut() {
            Some(ps) => ps.burst(handle, count),
            None => Err("Metal particle subsystem not initialized".to_string()),
        }
    }
}
