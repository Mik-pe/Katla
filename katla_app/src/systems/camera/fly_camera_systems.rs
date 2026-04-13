use katla_ecs::{ComponentAccess, System, World};
use katla_math::{Quat, Vec3};

use crate::components::{
    FlyCameraControllerComponent, FlyCameraLookComponent, ForceComponent, VelocityComponent,
};
use crate::input::{Action, InputState};

/// Compute the camera movement direction vector from input state.
///
/// Returns a normalized-ish direction vector where:
/// - X: positive = right, negative = left
/// - Y: positive = up, negative = down
/// - Z: negative = forward, positive = backward
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
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let Some(input) = world.get_resource::<InputState>() else {
            return;
        };
        let should_look = input.is_action_pressed(Action::LookEnable);
        let input_dir = compute_movement_direction(input);
        let speed = input.camera_speed * compute_speed_multiplier(input);

        let delta = input.mouse_delta;
        let has_movement_input = input_dir.length_squared() > 0.0;

        // Collect all updates first
        let transform_updates: Vec<(katla_ecs::EntityId, Quat, bool)> = world
            .query::<(
                &FlyCameraControllerComponent,
                &mut FlyCameraLookComponent,
                &crate::components::TransformComponent,
            )>()
            .map(|(entity, ctrl, look, transform)| {
                if should_look {
                    look.yaw -= ctrl.sensitivity * delta.0;
                    look.pitch -= ctrl.sensitivity * delta.1;

                    let limit = ctrl.pitch_limit.max(0.0);
                    look.pitch = look.pitch.clamp(-limit, limit);
                    (
                        entity,
                        Quat::new_from_yaw_pitch(look.yaw, look.pitch),
                        has_movement_input,
                    )
                } else {
                    (entity, transform.transform.rotation, has_movement_input)
                }
            })
            .collect();

        // Apply transform updates
        for (entity, rotation, _has_input) in &transform_updates {
            if let Some(transform) =
                world.get_component_mut::<crate::components::TransformComponent>(*entity)
            {
                transform.transform.rotation = *rotation;
            }
        }

        // Apply force/velocity updates
        for (entity, rotation, has_input) in transform_updates {
            if has_input {
                if let Some(force) = world.get_component_mut::<ForceComponent>(entity) {
                    let world_dir = rotation.rotate_vec3(input_dir);
                    force.force += world_dir.mul(speed);
                }
            } else {
                if let Some(velocity) = world.get_component_mut::<VelocityComponent>(entity) {
                    let vel_speed = velocity.velocity.length();
                    if vel_speed > 0.01 {
                        velocity.velocity *= 0.85;
                    } else {
                        velocity.velocity = Vec3::new(0.0, 0.0, 0.0);
                    }
                }
            }
        }
    }

    fn component_access() -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::read::<FlyCameraControllerComponent>(),
            ComponentAccess::write::<FlyCameraLookComponent>(),
            ComponentAccess::write::<crate::components::TransformComponent>(),
            ComponentAccess::write::<ForceComponent>(),
            ComponentAccess::write::<VelocityComponent>(),
        ]
    }
}
