use katla_ecs::{ComponentAccess, System, World};
use katla_math::{Quat, Vec3};

use crate::components::{FlyCameraControllerComponent, FlyCameraLookComponent};
use crate::input::{Action, InputState};

fn compute_movement_direction(input: &InputState) -> Vec3 {
    let fwd = input.is_action_pressed(Action::MoveForward) as i32 as f32;
    let back = input.is_action_pressed(Action::MoveBackward) as i32 as f32;
    let left = input.is_action_pressed(Action::MoveLeft) as i32 as f32;
    let right = input.is_action_pressed(Action::MoveRight) as i32 as f32;
    let up = input.is_action_pressed(Action::MoveUp) as i32 as f32;
    let down = input.is_action_pressed(Action::MoveDown) as i32 as f32;

    let x = right - left;
    let y = up - down;
    let z = -(fwd - back);

    Vec3::new(x, y, z)
}

fn compute_speed_multiplier(input: &InputState) -> f32 {
    if input.is_action_pressed(Action::Sprint) {
        3.0
    } else if input.is_action_pressed(Action::Slow) {
        0.3
    } else {
        1.0
    }
}

pub struct FlyCameraLookSystem;

impl System for FlyCameraLookSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        let Some(input) = world.get_resource::<InputState>() else {
            return;
        };
        let should_look = input.is_action_pressed(Action::LookEnable);
        let input_dir = compute_movement_direction(input);
        let speed = input.camera_speed * compute_speed_multiplier(input);

        let delta = input.mouse_delta;
        let has_movement_input = input_dir.length_squared() > 0.0;

        let updates: Vec<(katla_ecs::EntityId, Quat, Vec3, Vec3)> = world
            .query::<(
                &FlyCameraControllerComponent,
                &mut FlyCameraLookComponent,
                &crate::components::TransformComponent,
            )>()
            .map(|(entity, ctrl, look, transform)| {
                let rotation = if should_look {
                    look.yaw -= ctrl.sensitivity * delta.0;
                    look.pitch -= ctrl.sensitivity * delta.1;

                    let limit = ctrl.pitch_limit.max(0.0);
                    look.pitch = look.pitch.clamp(-limit, limit);
                    Quat::new_from_yaw_pitch(look.yaw, look.pitch)
                } else {
                    transform.transform.rotation
                };

                let velocity = if has_movement_input {
                    let world_dir = rotation.rotate_vec3(input_dir);
                    look.velocity + world_dir.mul(speed * delta_time)
                } else {
                    let vel_speed = look.velocity.length();
                    if vel_speed > 0.01 {
                        let damping = 0.85_f32.powf(delta_time * 60.0);
                        look.velocity * damping
                    } else {
                        Vec3::new(0.0, 0.0, 0.0)
                    }
                };
                look.velocity = velocity;

                let new_position = transform.transform.position + velocity * delta_time;

                (entity, rotation, velocity, new_position)
            })
            .collect();

        for (entity, rotation, _velocity, new_position) in updates {
            if let Some(transform) =
                world.get_component_mut::<crate::components::TransformComponent>(entity)
            {
                transform.transform.rotation = rotation;
                transform.transform.position = new_position;
            }
        }
    }

    fn component_access() -> Vec<ComponentAccess>
    where
        Self: Sized,
    {
        vec![
            ComponentAccess::read::<FlyCameraControllerComponent>(),
            ComponentAccess::write::<FlyCameraLookComponent>(),
            ComponentAccess::write::<crate::components::TransformComponent>(),
        ]
    }

    fn component_access_dyn(&self) -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::read::<FlyCameraControllerComponent>(),
            ComponentAccess::write::<FlyCameraLookComponent>(),
            ComponentAccess::write::<crate::components::TransformComponent>(),
        ]
    }
}
