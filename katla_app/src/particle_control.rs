//! Bounded particle commands shared by agents, trigger rules and deferred scripts.
use crate::components::ParticleEmitterComponent;
use katla_ecs::{EntityId, World};

pub(crate) fn burst(world: &mut World, entity: EntityId, count: u32) -> Result<(), String> {
    if count == 0 || count > 100_000 {
        return Err("Particle burst requires 1..100000 particles".into());
    }
    let emitter = world
        .get_component_mut::<ParticleEmitterComponent>(entity)
        .ok_or("Target has no live particle emitter")?;
    if !emitter.active {
        return Err("Particle emitter is inactive; enable it before bursting".into());
    }
    if emitter.burst_queue.len() >= 1024 {
        return Err("Particle burst queue is full".into());
    }
    emitter.burst(count);
    Ok(())
}
pub(crate) fn set_active(world: &mut World, entity: EntityId, active: bool) -> Result<(), String> {
    let emitter = world
        .get_component_mut::<ParticleEmitterComponent>(entity)
        .ok_or("Target has no live particle emitter")?;
    emitter.active = active;
    Ok(())
}

pub(crate) fn process_script_commands(world: &mut World) {
    let commands = world
        .get_resource_mut::<katla_script::PendingParticleCommands>()
        .map(|r| std::mem::take(&mut r.0))
        .unwrap_or_default();
    for command in commands {
        let result = match command {
            katla_script::ScriptCommand::BurstParticles { entity, count } => {
                burst(world, entity, count)
            }
            katla_script::ScriptCommand::SetParticlesActive { entity, active } => {
                set_active(world, entity, active)
            }
            _ => continue,
        };
        if let Err(error) = result {
            log::warn!("Script particle command failed: {error}");
        }
    }
}
