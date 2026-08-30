//! Particle system for managing particle emitters via ECS.
//!
//! This system synchronizes particle emitter components with the global
//! particle system, ensuring that emitters are created, updated, and destroyed
//! based on ECS entity lifecycle.

use katla_gfx::particles::EmitterHandle;
use log::{info, warn};

use katla_ecs::{EntityId, World};
use std::collections::HashMap;

use crate::components::{ParticleEmitterComponent, WorldTransform};

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
    entity_emitters: HashMap<EntityId, EmitterHandle>,
    entity_kill_on_destroy: HashMap<EntityId, bool>,
}

impl ParticleSystem {
    /// Create a new particle system.
    pub fn new() -> Self {
        Self {
            entity_emitters: HashMap::new(),
            entity_kill_on_destroy: HashMap::new(),
        }
    }

    /// Update particle emitters from ECS components.
    ///
    /// This should be called each frame after the global particle system
    /// has been initialized.
    ///
    /// Update particle emitters from ECS components.
    ///
    /// Backend-agnostic: `particle_system` is any `ParticleEmitterDriver`
    /// (Vulkan's `GlobalParticleSystem` or Metal's `MetalParticleSubsystem`).
    ///
    /// * `delta_time` - Frame time in seconds (for timed emission)
    pub fn update(
        &mut self,
        world: &mut World,
        particle_system: &mut dyn katla_gfx::ParticleEmitterDriver,
        delta_time: f32,
    ) {
        // Clean up GPU emitters for entities that no longer exist
        // or no longer have a ParticleEmitterComponent
        let destroyed: Vec<EntityId> = self
            .entity_emitters
            .keys()
            .filter(|id| {
                !world.entity_exists(**id)
                    || world
                        .get_component::<ParticleEmitterComponent>(**id)
                        .is_none()
            })
            .copied()
            .collect();
        for entity_id in destroyed {
            if let Some(handle) = self.entity_emitters.remove(&entity_id) {
                let kill = self
                    .entity_kill_on_destroy
                    .remove(&entity_id)
                    .unwrap_or(false);
                particle_system.destroy_emitter(handle, kill);
                info!(
                    "Destroyed particle emitter for destroyed entity {:?}",
                    entity_id
                );
            }
        }

        // Collect world positions before mutable borrow
        let world_positions: HashMap<EntityId, [f32; 3]> = world
            .query::<&WorldTransform>()
            .map(|(id, wt)| {
                let p = wt.transform.position;
                (id, [p.x(), p.y(), p.z()])
            })
            .collect();

        // Query all particle emitter components
        for (entity_id, emitter) in world.query::<&mut ParticleEmitterComponent>() {
            if emitter.active {
                // Initialize emitter if not already done
                if emitter.emitter_handle.is_none() {
                    match particle_system.create_emitter(emitter.config) {
                        Ok(handle) => {
                            emitter.emitter_handle = Some(handle);
                            self.entity_emitters.insert(entity_id, handle);
                            self.entity_kill_on_destroy
                                .insert(entity_id, emitter.kill_on_destroy);
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
                        if let Some(pos) = world_positions.get(&entity_id) {
                            emitter.config.position = *pos;
                        }
                        particle_system.update_emitter(handle, emitter.config);

                        // Process burst queue
                        for burst_count in emitter.burst_queue.drain(..) {
                            if let Err(e) = particle_system.burst(handle, burst_count) {
                                warn!("Failed to burst particles: {}", e);
                            }
                        }

                        // Handle timed emission
                        if let Some(remaining) = emitter.timed_emission {
                            let new_remaining = remaining - delta_time;
                            if new_remaining <= 0.0 {
                                emitter.timed_emission = None;
                                emitter.active = false;
                                info!("Timed emission expired, deactivating emitter");
                            } else {
                                emitter.timed_emission = Some(new_remaining);
                            }
                        }
                    }
                }
            } else {
                // Destroy emitter if component is inactive
                if let Some(handle) = emitter.emitter_handle.take() {
                    self.entity_emitters.remove(&entity_id);
                    let kill = self
                        .entity_kill_on_destroy
                        .remove(&entity_id)
                        .unwrap_or(false);
                    particle_system.destroy_emitter(handle, kill);
                    info!("Destroyed particle emitter for inactive component");
                }
            }
        }
    }
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}
