//! App-owned component dependencies and complete collider removal history.

use katla_ecs::scene_tool::{
    ComponentRegistry, SceneCommand, SceneOp, SceneToolError, SceneToolExecutor, ToolResult,
    UndoGroup,
};
use katla_ecs::{EntityId, World};
use katla_physics::{ColliderShape, CollisionFilter, PhysicsMaterial, TriggerVolume};

pub(in crate::application) fn execute(
    op: SceneOp,
    world: &mut World,
    registry: &ComponentRegistry,
) -> Result<(ToolResult, UndoGroup), SceneToolError> {
    let SceneOp::RemoveComponent {
        entity,
        ref component,
    } = op
    else {
        return SceneToolExecutor::execute(op, world, registry);
    };
    if component != "ColliderShape" {
        return SceneToolExecutor::execute(op, world, registry);
    }
    let shape = world
        .get_component::<ColliderShape>(entity)
        .cloned()
        .ok_or_else(|| SceneToolError::ComponentNotFound {
            entity,
            component: component.clone(),
        })?;
    let entry = registry
        .get(component)
        .ok_or_else(|| SceneToolError::ComponentNotFound {
            entity,
            component: component.clone(),
        })?;
    let mut command = RemoveCollider {
        entity,
        shape,
        filter: world.get_component::<CollisionFilter>(entity).copied(),
        material: world.get_component::<PhysicsMaterial>(entity).copied(),
        trigger: world.get_component::<TriggerVolume>(entity).cloned(),
        remove: entry.remove_component,
    };
    command.execute(world)?;
    let message = command.description();
    let mut group = UndoGroup::new(message.clone());
    group.commands.push(Box::new(command));
    Ok((
        ToolResult {
            success: true,
            message,
            affected_entities: vec![entity],
            data: None,
        },
        group,
    ))
}

struct RemoveCollider {
    entity: EntityId,
    shape: ColliderShape,
    filter: Option<CollisionFilter>,
    material: Option<PhysicsMaterial>,
    trigger: Option<TriggerVolume>,
    remove: fn(&mut World, EntityId),
}
impl SceneCommand for RemoveCollider {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if !world.entity_exists(self.entity) {
            return Err(SceneToolError::EntityNotFound(self.entity));
        }
        (self.remove)(world, self.entity);
        world.remove_component::<CollisionFilter>(self.entity);
        world.remove_component::<PhysicsMaterial>(self.entity);
        world.remove_component::<TriggerVolume>(self.entity);
        Ok(())
    }
    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if !world.entity_exists(self.entity) {
            return Err(SceneToolError::EntityNotFound(self.entity));
        }
        world.add_component(self.entity, self.shape.clone());
        if let Some(filter) = self.filter {
            world.add_component(self.entity, filter);
        }
        if let Some(material) = self.material {
            world.add_component(self.entity, material);
        }
        if let Some(trigger) = &self.trigger {
            world.add_component(self.entity, trigger.clone());
        }
        Ok(())
    }
    fn description(&self) -> String {
        format!(
            "Remove collider and collision settings from {}",
            self.entity
        )
    }
    fn affected_entities(&self) -> Vec<EntityId> {
        vec![self.entity]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_remove_collider_restores_exact_shape_and_settings() {
        let mut world = World::new();
        let entity = world.spawn((crate::components::NameComponent::new("Collider"),));
        world.add_component(
            entity,
            ColliderShape::Box(katla_physics::BoxShape::new([1., 2., 3.].into())),
        );
        let material = PhysicsMaterial {
            friction: 0.7,
            restitution: 0.3,
            density: 2.,
        };
        world.add_component(entity, material);
        world.add_component(entity, CollisionFilter::default());
        world.add_component(entity, TriggerVolume::default());
        let registry = super::super::component_registry::build_editor_component_registry();
        let (_, mut group) = execute(
            SceneOp::RemoveComponent {
                entity,
                component: "ColliderShape".into(),
            },
            &mut world,
            &registry,
        )
        .unwrap();
        assert!(world.get_component::<ColliderShape>(entity).is_none());
        assert!(world.get_component::<PhysicsMaterial>(entity).is_none());
        assert!(world.get_component::<CollisionFilter>(entity).is_none());
        assert!(world.get_component::<TriggerVolume>(entity).is_none());
        group.undo_all(&mut world).unwrap();
        assert!(
            matches!(world.get_component::<ColliderShape>(entity), Some(ColliderShape::Box(shape)) if shape.half_extents == [1., 2., 3.])
        );
        assert_eq!(
            world
                .get_component::<PhysicsMaterial>(entity)
                .unwrap()
                .friction,
            0.7
        );
        assert_eq!(
            world
                .get_component::<PhysicsMaterial>(entity)
                .unwrap()
                .density,
            2.
        );
        assert!(world.get_component::<TriggerVolume>(entity).is_some());
        group.redo_all(&mut world).unwrap();
        assert!(world.get_component::<ColliderShape>(entity).is_none());
        assert!(world.get_component::<PhysicsMaterial>(entity).is_none());
    }
}
