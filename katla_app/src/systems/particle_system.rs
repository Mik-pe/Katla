//! Particle system for managing particle emitters via ECS.
//!
//! This system synchronizes particle emitter components with the global
//! particle system, ensuring that emitters are created, updated, and destroyed
//! based on ECS entity lifecycle.

use katla_ecs::World;
use log::{debug, info, warn};

use crate::components::ParticleEmitterComponent;

/// System that manages particle emitters in the ECS.
///
/// This system:
/// - Creates emitters in the global particle system for entities with ParticleEmitterComponent
/// - Updates emitter configurations from component data
/// - Destroys emitters when components are removed
///
/// # Usage
/// ```ignore
/// let mut particle_system = ParticleSystem::new();
///
/// // Run each frame to sync emitters
/// particle_system.update(&mut world, &mut renderer.particle_system);
/// ```
pub struct ParticleSystem {
    /// Track which entities have initialized their emitters
    initialized_emitters: Vec<u32>,
}

impl ParticleSystem {
    /// Create a new particle system.
    pub fn new() -> Self {
        Self {
            initialized_emitters: Vec::new(),
        }
    }

    /// Update particle emitters from ECS components.
    ///
    /// This should be called each frame after the global particle system
    /// has been initialized.
    ///
    /// # Arguments
    /// * `world` - The ECS world
    /// * `particle_system` - The global particle system (mutably borrowed)
    pub fn update(
        &mut self,
        world: &mut World,
        particle_system: &mut Option<katla_gfx::particles::GlobalParticleSystem>,
    ) {
        let Some(ps) = particle_system else {
            debug!("Particle system not available, skipping emitter update");
            return;
        };

        // Query all particle emitter components
        for (_entity_id, emitter) in world.query::<&mut ParticleEmitterComponent>() {
            if emitter.active {
                // Initialize emitter if not already done
                if emitter.emitter_handle.is_none() {
                    match ps.create_emitter(emitter.config) {
                        Ok(handle) => {
                            emitter.emitter_handle = Some(handle);
                            info!(
                                "Created particle emitter at position {:?}",
                                emitter.config.position
                            );
                        }
                        Err(e) => {
                            warn!("Failed to create particle emitter: {}", e);
                        }
                    }
                } else {
                    // Update existing emitter configuration
                    if let Some(handle) = emitter.emitter_handle {
                        ps.update_emitter(handle, emitter.config);
                        debug!(
                            "Updated particle emitter at position {:?}",
                            emitter.config.position
                        );
                    }
                }
            } else {
                // Destroy emitter if component is inactive
                if let Some(handle) = emitter.emitter_handle.take() {
                    ps.destroy_emitter(handle);
                    info!("Destroyed particle emitter for inactive component");
                }
            }
        }

        // Clean up emitters for entities that no longer exist
        // (This is handled automatically when components are removed,
        // but we could track it here if needed)
    }
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_gfx::particles::EmitterConfig;

    #[test]
    fn test_particle_system_creation() {
        let system = ParticleSystem::new();
        assert!(system.initialized_emitters.is_empty());
    }

    #[test]
    fn test_emitter_config_defaults() {
        let config = EmitterConfig::default();
        assert_eq!(config.position, [0.0; 3]);
        assert!(config.emit_rate > 0.0);
    }
}
