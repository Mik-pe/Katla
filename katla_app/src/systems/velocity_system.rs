use std::collections::HashMap;

use katla_ecs::{ComponentStorageManager, EntityId, System};
use katla_math::Vec3;

use crate::components::{DragComponent, TransformComponent, VelocityComponent};

pub struct VelocitySystem;

impl System for VelocitySystem {
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
        let drag_storage: HashMap<EntityId, DragComponent> =
            match storage.get_storage::<DragComponent>() {
                Some(drag_storage) => drag_storage
                    .iter()
                    .map(|(id, component)| (id, component.clone()))
                    .collect(),
                None => HashMap::new(),
            };

        if let Some(velocity_storage) = storage.get_storage_mut::<VelocityComponent>() {
            let velocities: Vec<(EntityId, Vec3)> = velocity_storage
                .iter_mut()
                .map(|(id, component)| {
                    if let Some(drag) = drag_storage.get(&id) {
                        let friction_dir = -component.velocity.clone().normalize();
                        let friction_magnitude = component.velocity.distance_squared();
                        let friction_vector = friction_dir * friction_magnitude;
                        println!("Acceleration was: {:?}", component.acceleration);
                        println!("Friction was: {:?}", friction_vector);
                        println!("Velocity was: {:?}", component.velocity);

                        component.velocity += drag.drag * friction_vector * delta_time;
                    }

                    component.velocity += component.acceleration * delta_time;
                    component.acceleration = Vec3::default();

                    (id, component.velocity)
                })
                .collect();

            let transform_storage = storage.get_storage_mut::<TransformComponent>().unwrap();
            for (id, velocity) in velocities {
                if let Some(transform) = transform_storage.get_mut(id) {
                    transform.transform.position += velocity * delta_time;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_ecs::{SystemExecutionOrder, World};
    use katla_math::Vec3;

    #[test]
    fn test_velocity_system() {
        let mut world = World::new();
        let system = VelocitySystem;
        world.register_system(Box::new(system), SystemExecutionOrder::NORMAL);

        let entity_id = world.create_entity();

        world.add_component(
            entity_id,
            VelocityComponent::new(Vec3::new(1.0, 2.0, 3.0), Vec3::new(-0.1, -0.2, -0.3)),
        );
        world.add_component(entity_id, TransformComponent::default());
        world.update(1.0);
        let velocity = world.get_component::<VelocityComponent>(entity_id).unwrap();
        let transform = world
            .get_component::<TransformComponent>(entity_id)
            .unwrap();

        assert_eq!(velocity.velocity, Vec3::new(0.9, 1.8, 2.7));
        assert_eq!(transform.transform.position, Vec3::new(0.9, 1.8, 2.7));
    }
}
