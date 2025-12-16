use std::collections::HashMap;

use katla_ecs::{ComponentStorageManager, EntityId, System};
use katla_math::Vec3;

use crate::components::{DragComponent, TransformComponent, VelocityComponent};

pub struct VelocitySystem;

impl System for VelocitySystem {
    fn update(&mut self, storage: &mut ComponentStorageManager, delta_time: f32) {
        let drag_storage: HashMap<EntityId, DragComponent> = storage
            .get_storage::<DragComponent>()
            .unwrap()
            .iter()
            .map(|(id, component)| (id, component.clone()))
            .collect();
        let velocity_storage = storage.get_storage_mut::<VelocityComponent>().unwrap();
        let velocities: Vec<(EntityId, Vec3)> = velocity_storage
            .iter_mut()
            .map(|(id, component)| {
                if let Some(drag) = drag_storage.get(&id) {
                    component.acceleration =
                        component.acceleration - component.acceleration * drag.drag * delta_time;
                    component.velocity =
                        component.velocity - component.velocity * drag.drag * delta_time;
                }
                component.velocity += component.acceleration * delta_time;

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
