use katla_ecs::{ComponentStorageManager, System};

use crate::components::{TransformComponent, VelocityComponent};

pub struct VelocitySystem;

impl System for VelocitySystem {
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
        // Integrate position: p = p + v * dt
        for (_entity, transform, velocity) in
            storage.query::<(&mut TransformComponent, &VelocityComponent)>()
        {
            let displacement = velocity.velocity * delta_time;
            transform.transform.position += displacement;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{TransformComponent, VelocityComponent};
    use katla_ecs::{SystemExecutionOrder, World};
    use katla_math::{Transform, Vec3};

    #[test]
    fn test_velocity_system() {
        let mut world = World::new();
        world.register_system(Box::new(VelocitySystem), SystemExecutionOrder::NORMAL);

        let entity_id = world.create_entity();

        world.add_component(
            entity_id,
            VelocityComponent::new(Vec3::new(1.0, 2.0, 3.0), Vec3::new(-0.1, -0.2, -0.3)),
        );
        world.add_component(
            entity_id,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 0.0, 0.0))),
        );

        world.update(1.0);

        let transform = world
            .get_component::<TransformComponent>(entity_id)
            .unwrap();

        // Position should be updated by velocity
        assert_eq!(transform.transform.position, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_velocity_system_multiple_entities() {
        let mut world = World::new();
        world.register_system(Box::new(VelocitySystem), SystemExecutionOrder::NORMAL);

        let entity1 = world.create_entity();
        let entity2 = world.create_entity();

        world.add_component(
            entity1,
            VelocityComponent::new(Vec3::new(5.0, 0.0, 0.0), Vec3::default()),
        );
        world.add_component(
            entity1,
            TransformComponent::new(Transform::from_position(Vec3::new(0.0, 0.0, 0.0))),
        );

        world.add_component(
            entity2,
            VelocityComponent::new(Vec3::new(-2.0, 3.0, 0.0), Vec3::default()),
        );
        world.add_component(
            entity2,
            TransformComponent::new(Transform::from_position(Vec3::new(10.0, 5.0, 0.0))),
        );

        world.update(1.0);

        let transform1 = world.get_component::<TransformComponent>(entity1).unwrap();
        let transform2 = world.get_component::<TransformComponent>(entity2).unwrap();

        assert_eq!(transform1.transform.position, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(transform2.transform.position, Vec3::new(8.0, 8.0, 0.0));
    }

    #[test]
    fn test_velocity_system_no_transform() {
        let mut world = World::new();
        world.register_system(Box::new(VelocitySystem), SystemExecutionOrder::NORMAL);

        let entity = world.create_entity();
        world.add_component(
            entity,
            VelocityComponent::new(Vec3::new(1.0, 2.0, 3.0), Vec3::default()),
        );

        // Should not crash when entity has velocity but no transform
        world.update(1.0);
    }
}
