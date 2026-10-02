//! Validated authoring shared by MCP and the editor agent.

use super::TriggerRules;
use crate::components::{NameComponent, TransformComponent};
use katla_agent::events::{EventAction, EventTarget, TriggerOp, TriggerRule};
use katla_ecs::{EntityId, World};
use katla_math::Vec3;
use katla_physics::{BoxShape, ColliderShape, RigidBody, TriggerVolume};
use serde_json::{Value, json};

pub(crate) fn execute(world: &mut World, op: TriggerOp) -> Result<Value, String> {
    let entity = match op {
        TriggerOp::Inspect { entity_id } => EntityId::from_raw(entity_id),
        TriggerOp::SetRules { entity_id, rules } => {
            let entity = EntityId::from_raw(entity_id);
            if world.get_component::<TriggerVolume>(entity).is_none() {
                return Err("Entity is not a live trigger".into());
            }
            if world.get_component::<RigidBody>(entity).is_none()
                || world.get_component::<ColliderShape>(entity).is_none()
                || world.get_component::<TransformComponent>(entity).is_none()
            {
                return Err(
                    "Trigger rules require RigidBody, ColliderShape and TransformComponent".into(),
                );
            }
            validate_references(world, &rules)?;
            world.add_component(entity, TriggerRules::new(rules)?);
            entity
        }
        TriggerOp::CreateBox {
            name,
            position,
            half_extents,
            rules,
        } => {
            if name.trim().is_empty() || name.len() > 128 {
                return Err("Trigger name requires 1..128 bytes".into());
            }
            if world
                .query_ref::<&NameComponent>()
                .any(|(_, n)| n.name == name)
            {
                return Err("Trigger name already exists".into());
            }
            if position.iter().any(|v| !v.is_finite())
                || half_extents.iter().any(|v| !v.is_finite() || *v <= 0.0)
            {
                return Err("Box needs finite position and positive half_extents".into());
            }
            validate_references(world, &rules)?;
            let rules = TriggerRules::new(rules)?;
            world.spawn((
                NameComponent::new(name),
                TransformComponent::from_position(Vec3::new(position[0], position[1], position[2])),
                crate::scene::EntitySource::Trigger,
                ColliderShape::Box(BoxShape::new(Vec3::new(
                    half_extents[0],
                    half_extents[1],
                    half_extents[2],
                ))),
                RigidBody::kinematic(),
                TriggerVolume::new(),
                rules,
            ))
        }
    };
    let volume = world
        .get_component::<TriggerVolume>(entity)
        .ok_or("Entity is not a live trigger")?;
    let rules = world.get_component::<TriggerRules>(entity);
    let wire_rules = match rules
        .map(|r| r.rules.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|rule| rule.map_entities(|id| Ok::<_, std::convert::Infallible>(id.to_string())))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(rules) => rules,
        Err(never) => match never {},
    };
    Ok(json!({"entity_id":entity.id().to_string(),
        "name":world.get_component::<NameComponent>(entity).map(|name| &name.name),
        "position":world.get_component::<TransformComponent>(entity).map(|transform| transform.transform.position.to_array()),
        "shape":world.get_component::<ColliderShape>(entity),
        "simulation_active":world.get_resource::<katla_physics::PhysicsActive>().is_some_and(|active| active.0),
        "overlapping_entities":volume.overlapping_entities.iter().map(u64::to_string).collect::<Vec<_>>(),
        "rules": wire_rules,
        "fired_once_rules": rules.map(|r| r.fired.as_slice()).unwrap_or_default(),
        "last_errors": rules.map(|r| r.last_errors.as_slice()).unwrap_or_default()}))
}

fn validate_references(world: &World, rules: &[TriggerRule]) -> Result<(), String> {
    super::validate_rules(rules)?;
    for rule in rules {
        rule.map_entities(|id| {
            if world.entity_ids().any(|entity| entity.id() == *id) {
                Ok(*id)
            } else {
                Err(format!("Entity {id} is stale or missing"))
            }
        })?;
        for action in &rule.actions {
            if let EventAction::PlayAnimation {
                target: EventTarget::Entity { entity },
                clip,
                ..
            } = action
            {
                let model = world
                    .get_component::<crate::animation::AnimatedModel>(EntityId::from_raw(*entity))
                    .ok_or_else(|| format!("Entity {entity} has no animated model"))?;
                if !model.animations.contains_key(clip) {
                    return Err(format!("Unknown clip '{clip}' on entity {entity}"));
                }
            }
        }
    }
    Ok(())
}
