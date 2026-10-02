use katla_ecs::{Query, Res, SystemParam, TypedSystem, Write};
use katla_math::{Quat, Vec3};

use crate::components::{OrbitCameraControllerComponent, TransformComponent};
use crate::input::{Action, ButtonState, InputState, MouseButton};

fn is_mouse_button_pressed(input: &InputState, button: MouseButton) -> bool {
    input.mouse_buttons[button as usize] == ButtonState::Pressed
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub struct OrbitCameraSystem;

impl TypedSystem for OrbitCameraSystem {
    type Params = (
        Query<(
            Write<OrbitCameraControllerComponent>,
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
        let should_orbit = input.is_action_pressed(Action::LookEnable);
        let should_pan = is_mouse_button_pressed(&input, MouseButton::Middle)
            || input.is_action_pressed(Action::PanEnable);
        let scroll = input.mouse_wheel_delta;
        let delta = input.mouse_delta;

        for (_entity, (orbit, transform)) in cameras.iter_mut() {
            // Update focus animation
            if let Some(focus) = &mut orbit.focus {
                focus.elapsed += delta_time;
                let t = (focus.elapsed / focus.duration).min(1.0);
                let t = smoothstep(t);

                orbit.target = focus.start_target + (focus.target - focus.start_target) * t;
                orbit.distance = focus.start_distance + (focus.distance - focus.start_distance) * t;
                orbit.yaw = focus.start_yaw + (focus.target_yaw - focus.start_yaw) * t;
                orbit.pitch = focus.start_pitch + (focus.target_pitch - focus.start_pitch) * t;

                if t >= 1.0 {
                    orbit.focus = None;
                }
            }

            // Skip manual controls during focus animation
            let animating = orbit.focus.is_some();

            if !animating && should_orbit {
                orbit.yaw -= orbit.sensitivity * delta.0;
                orbit.pitch -= orbit.sensitivity * delta.1;
                let limit = orbit.pitch_limit.max(0.0);
                orbit.pitch = orbit.pitch.clamp(-limit, limit);
            }

            if !animating && should_pan {
                let fov_rad = orbit.fov.to_radians();
                let visible_height = 2.0 * orbit.distance * fov_rad.tan();
                let units_per_pixel = visible_height / 1000.0;

                let rotation = Quat::new_from_yaw_pitch(orbit.yaw, orbit.pitch);
                let right = rotation.rotate_vec3(Vec3::new(1.0, 0.0, 0.0));
                let up = Vec3::new(0.0, 1.0, 0.0);
                orbit.target -= right * delta.0 * units_per_pixel;
                orbit.target += up * delta.1 * units_per_pixel;
            }

            if !animating && scroll.abs() > 0.0 {
                orbit.distance *= 1.0 - scroll * orbit.zoom_speed * 0.1;
                orbit.distance = orbit.distance.clamp(orbit.min_distance, orbit.max_distance);
            }

            let rotation = Quat::new_from_yaw_pitch(orbit.yaw, orbit.pitch);
            let offset = rotation.rotate_vec3(Vec3::new(0.0, 0.0, orbit.distance));
            let position = orbit.target + offset;

            transform.transform.position = position;
            transform.transform.rotation = rotation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::FocusTarget;
    use katla_ecs::{SystemExecutionOrder, World};

    #[test]
    fn test_typed_orbit_focus_mutates_controller_and_transform_together() {
        let mut world = World::new();
        let orbit = OrbitCameraControllerComponent {
            target: Vec3::new(0.0, 0.0, 0.0),
            distance: 10.0,
            yaw: 0.0,
            pitch: 0.0,
            focus: Some(FocusTarget {
                target: Vec3::new(2.0, 0.0, 0.0),
                distance: 4.0,
                duration: 1.0,
                elapsed: 0.0,
                start_target: Vec3::new(0.0, 0.0, 0.0),
                start_distance: 10.0,
                start_yaw: 0.0,
                start_pitch: 0.0,
                target_yaw: 0.0,
                target_pitch: 0.0,
            }),
            ..Default::default()
        };
        let entity = world.spawn((orbit, TransformComponent::default()));
        let mut input = InputState::default();
        input.set_action_state(Action::LookEnable, true);
        input.mouse_delta = (100.0, 50.0);
        world.insert_resource(input);
        world.register_typed_system(OrbitCameraSystem, SystemExecutionOrder::NORMAL);
        world.update_parallel(0.5);
        let orbit = world
            .get_component::<OrbitCameraControllerComponent>(entity)
            .unwrap();
        assert_eq!(orbit.target, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(orbit.distance, 7.0);
        assert_eq!(orbit.yaw, 0.0);
        assert_eq!(orbit.pitch, 0.0);
        assert!(orbit.focus.is_some());
        assert_eq!(
            world
                .get_component::<TransformComponent>(entity)
                .unwrap()
                .transform
                .position,
            Vec3::new(1.0, 0.0, 7.0)
        );
        world
            .get_resource_mut::<InputState>()
            .unwrap()
            .set_action_state(Action::LookEnable, false);
        world.update_parallel(0.5);
        let orbit = world
            .get_component::<OrbitCameraControllerComponent>(entity)
            .unwrap();
        assert!(orbit.focus.is_none());
        assert_eq!(orbit.target, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(
            world
                .get_component::<TransformComponent>(entity)
                .unwrap()
                .transform
                .position,
            Vec3::new(2.0, 0.0, 4.0)
        );
    }
}
