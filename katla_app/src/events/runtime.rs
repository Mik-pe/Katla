//! Execute rule matches after a completed physics step, without recursive dispatch.

use super::{EventAction, EventTarget, TriggerPhase, TriggerRules};
use katla_ecs::{EntityId, World};
use katla_physics::TriggerEvent;
use katla_script::{PendingPhysicsEvents, PhysicsCollisionEvent, PhysicsCollisionEventType};

pub(crate) fn dispatch(world: &mut World, event: TriggerEvent) {
    let (phase, trigger, other) = match event {
        TriggerEvent::Enter {
            trigger_entity,
            other_entity,
        } => (TriggerPhase::Enter, trigger_entity, other_entity),
        TriggerEvent::Exit {
            trigger_entity,
            other_entity,
        } => (TriggerPhase::Exit, trigger_entity, other_entity),
    };
    let entity = EntityId::from_raw(trigger);
    let Some(rules) = world.get_component_mut::<TriggerRules>(entity) else {
        return;
    };
    let mut actions = Vec::new();
    for (index, rule) in rules.rules.iter().enumerate() {
        if rule.event != phase
            || rule.other_entity.is_some_and(|id| id != other)
            || (rule.once && rules.fired.contains(&index))
        {
            continue;
        }
        if rule.once {
            rules.fired.push(index);
        }
        actions.extend(rule.actions.iter().cloned());
    }
    if actions.is_empty() {
        return;
    }
    rules.last_errors.clear();
    let mut errors = Vec::new();
    for action in actions {
        let result = match action {
            EventAction::PlayAnimation {
                target,
                clip,
                fade_seconds,
                looping,
                speed,
            } => {
                let target = match target {
                    EventTarget::Trigger => trigger,
                    EventTarget::Other => other,
                    EventTarget::Entity { entity } => entity,
                };
                crate::animation::control::play(
                    world,
                    EntityId::from_raw(target),
                    clip,
                    fade_seconds,
                    looping,
                    speed,
                )
            }
            EventAction::BurstParticles { target, count } => {
                crate::particle_control::burst(world, recipient(target, trigger, other), count)
            }
            EventAction::SetParticlesActive { target, active } => {
                crate::particle_control::set_active(
                    world,
                    recipient(target, trigger, other),
                    active,
                )
            }
            EventAction::Emit { name } => match world.get_resource_mut::<PendingPhysicsEvents>() {
                Some(pending) => {
                    pending.0.push(PhysicsCollisionEvent {
                        event_type: PhysicsCollisionEventType::TriggerSignal(name),
                        entity_a: trigger,
                        entity_b: other,
                    });
                    Ok(())
                }
                None => {
                    Err("Script event resource is missing; initialize PendingPhysicsEvents".into())
                }
            },
        };
        if let Err(error) = result {
            log::warn!("Trigger {trigger} action failed: {error}");
            errors.push(error);
        }
    }
    if let Some(rules) = world.get_component_mut::<TriggerRules>(entity) {
        rules.last_errors = errors;
    }
}

pub(crate) fn reset(world: &mut World) {
    for (_, rules) in world.query::<&mut TriggerRules>() {
        rules.fired.clear();
        rules.last_errors.clear();
    }
    for (_, volume) in world.query::<&mut katla_physics::TriggerVolume>() {
        volume.overlapping_entities.clear();
    }
    if let Some(pending) = world.get_resource_mut::<PendingPhysicsEvents>() {
        pending.0.clear();
    }
}

fn recipient(target: EventTarget, trigger: u64, other: u64) -> EntityId {
    EntityId::from_raw(match target {
        EventTarget::Trigger => trigger,
        EventTarget::Other => other,
        EventTarget::Entity { entity } => entity,
    })
}
