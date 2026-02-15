//! Particle simulation system for GPU-based particle effects.
//!
//! This system updates ParticleEmitter components each frame, managing
//! particle emission timing and preparing push constants for the GPU.

use katla_ecs::{EntityId, System, World};

use crate::components::{ParticleEmitter, TransformComponent};

/// System for updating particle emitter state each frame.
///
/// This system handles:
/// - Emit accumulator updates based on emit rate and delta time
/// - Synchronizing emitter position with entity transform
/// - Preparing push constants for GPU compute dispatch
///
/// Note: The actual GPU compute dispatch happens in the render graph,
/// not in this system. This system only updates CPU-side state.
pub struct ParticleSimulationSystem;

impl Default for ParticleSimulationSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSimulationSystem {
    /// Create a new particle simulation system.
    pub fn new() -> Self {
        ParticleSimulationSystem
    }
}

impl System for ParticleSimulationSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        // Collect entities that need updates (to avoid borrow issues)
        let entities_to_update: Vec<EntityId> = world
            .query::<&ParticleEmitter>()
            .map(|(entity, _)| entity)
            .collect();

        // Update each particle emitter
        for entity in entities_to_update {
            // Get transform position if available
            let position = world
                .get_component::<TransformComponent>(entity)
                .map(|t| {
                    let pos = t.transform.position;
                    [pos.x(), pos.y(), pos.z()]
                });

            // Update emitter
            if let Some(mut emitter) = world.get_component_mut::<ParticleEmitter>(entity) {
                // Sync emitter position with transform if present
                if let Some(pos) = position {
                    emitter.set_position(pos);
                }

                // Update emitter (calculates emit count, updates accumulator)
                let _push_constants = emitter.update(delta_time);
            }
        }
    }

    fn name(&self) -> &str {
        "ParticleSimulationSystem"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_ecs::SystemExecutionOrder;
    use katla_math::{Transform, Vec3};

    #[test]
    fn test_particle_system_registration() {
        let mut world = World::new();
        world.register_system(
            Box::new(ParticleSimulationSystem::new()),
            SystemExecutionOrder::NORMAL,
        );

        // System should be registered without panic
        assert!(true);
    }

    #[test]
    fn test_particle_system_with_entity() {
        let mut world = World::new();
        world.register_system(
            Box::new(ParticleSimulationSystem::new()),
            SystemExecutionOrder::NORMAL,
        );

        // Create entity with transform (no particle emitter since we can't easily mock GPU resources)
        let entity = world.create_entity();
        world.add_component(
            entity,
            TransformComponent::new(Transform::from_position(Vec3::new(1.0, 2.0, 3.0))),
        );

        // Should not crash
        world.update(0.016);
    }

    #[test]
    fn test_particle_system_name() {
        let system = ParticleSimulationSystem::new();
        assert_eq!(system.name(), "ParticleSimulationSystem");
    }
}
