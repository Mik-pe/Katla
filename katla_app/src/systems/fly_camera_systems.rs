use katla_ecs::{InputState, System, World};
use katla_math::{Quat, Vec3};

use crate::components::{FlyCameraController, FlyCameraLook, ForceComponent};

pub struct FlyCameraLookSystem;

impl FlyCameraLookSystem {
    fn get_input_dir(&mut self, input: &InputState) -> Vec3 {
        let fwd = input.is_action_pressed(katla_ecs::input::Action::MoveForward) as i32 as f32;
        let back = input.is_action_pressed(katla_ecs::input::Action::MoveBackward) as i32 as f32;
        let left = input.is_action_pressed(katla_ecs::input::Action::MoveLeft) as i32 as f32;
        let right = input.is_action_pressed(katla_ecs::input::Action::MoveRight) as i32 as f32;
        let up = input.is_action_pressed(katla_ecs::input::Action::MoveUp) as i32 as f32;
        let down = input.is_action_pressed(katla_ecs::input::Action::MoveDown) as i32 as f32;

        let x = right - left;
        let y = up - down;
        let z = -(fwd - back);

        Vec3::new(x, y, z)
    }
}

impl System for FlyCameraLookSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let input = world.get_input();
        let should_look = input.is_action_pressed(katla_ecs::input::Action::LookEnable);
        let input_dir = self.get_input_dir(input);

        if input_dir.length_squared() == 0.0 && !should_look {
            return;
        }

        let delta = input.mouse_delta;

        let storage = world.storage_mut();

        let updates: Vec<(katla_ecs::EntityId, Quat, f32)> = storage
            .query::<(
                &FlyCameraController,
                &mut FlyCameraLook,
                &crate::components::TransformComponent,
            )>()
            .map(|(entity, ctrl, look, transform)| {
                if should_look {
                    look.yaw -= ctrl.sensitivity * delta.x;
                    look.pitch -= ctrl.sensitivity * delta.y;

                    let limit = ctrl.pitch_limit.max(0.0);
                    look.pitch = look.pitch.clamp(-limit, limit);
                    (
                        entity,
                        Quat::new_from_yaw_pitch(look.yaw, look.pitch),
                        ctrl.speed,
                    )
                } else {
                    (entity, transform.transform.rotation, ctrl.speed)
                }
            })
            .collect();

        for (entity, rotation, speed) in updates {
            if let Some(transform) =
                storage.get_component_mut::<crate::components::TransformComponent>(entity)
            {
                transform.transform.rotation = rotation;
            }
            if let Some(force) = storage.get_component_mut::<ForceComponent>(entity) {
                let world_dir = rotation.rotate_vec3(input_dir);
                force.force += world_dir.mul(speed);
            }
        }
    }
}
