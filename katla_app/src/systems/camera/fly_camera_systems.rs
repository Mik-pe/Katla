use katla_ecs::{Query, Read, Res, SystemParam, TypedSystem, Write};
use katla_math::{Quat, Vec3};

use crate::components::{FlyCameraControllerComponent, FlyCameraLookComponent, TransformComponent};
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

impl TypedSystem for FlyCameraLookSystem {
    type Params = (
        Query<(
            Read<FlyCameraControllerComponent>,
            Write<FlyCameraLookComponent>,
            Write<TransformComponent>,
        )>,
        Option<Res<InputState>>,
    );

    fn run(
        &mut self,
        (mut cameras, input): <Self::Params as SystemParam>::Item<'_>,
        delta_time: f32,
    ) {
        let Some(input) = input else { return };
        let should_look = input.is_action_pressed(Action::LookEnable);
        let input_dir = compute_movement_direction(&input);
        let speed = input.camera_speed * compute_speed_multiplier(&input);
        let delta = input.mouse_delta;
        let has_movement_input = input_dir.length_squared() > 0.0;

        for (_entity, (ctrl, look, transform)) in cameras.iter_mut() {
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
            } else if look.velocity.length() > 0.01 {
                look.velocity * 0.85_f32.powf(delta_time * 60.0)
            } else {
                Vec3::new(0.0, 0.0, 0.0)
            };
            look.velocity = velocity;
            transform.transform.rotation = rotation;
            transform.transform.position += velocity * delta_time;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_ecs::{SystemExecutionOrder, World};

    #[test]
    fn test_typed_fly_camera_optional_input_and_motion() {
        let mut world = World::new();
        let entity = world.spawn((
            FlyCameraControllerComponent::default(),
            FlyCameraLookComponent::default(),
            TransformComponent::default(),
        ));
        world.register_typed_system(FlyCameraLookSystem, SystemExecutionOrder::NORMAL);
        world.update_parallel(0.1);
        assert_eq!(
            world
                .get_component::<TransformComponent>(entity)
                .unwrap()
                .transform
                .position,
            Vec3::new(0.0, 0.0, 0.0)
        );
        let mut input = InputState::default();
        input.set_action_state(Action::MoveForward, true);
        input.set_action_state(Action::LookEnable, true);
        input.mouse_delta = (20.0, 10.0);
        world.insert_resource(input);
        world.update_parallel(0.1);
        let look = world
            .get_component::<FlyCameraLookComponent>(entity)
            .unwrap();
        assert!((look.yaw + 0.1).abs() < 1e-6);
        assert!((look.pitch + 0.05).abs() < 1e-6);
        let expected = Quat::new_from_yaw_pitch(look.yaw, look.pitch)
            .rotate_vec3(Vec3::new(0.0, 0.0, -1.0))
            * 0.5;
        let actual = world
            .get_component::<TransformComponent>(entity)
            .unwrap()
            .transform
            .position;
        assert!((actual - expected).length() < 1e-6);
    }
}
