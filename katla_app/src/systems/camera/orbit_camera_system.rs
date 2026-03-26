use katla_ecs::{input::MouseButton, InputState, System, World};
use katla_math::{Quat, Vec3};

use crate::components::{OrbitCameraControllerComponent, TransformComponent};

fn is_mouse_button_pressed(input: &InputState, button: MouseButton) -> bool {
    use katla_ecs::input::ButtonState;
    input.mouse_buttons[button as usize] == ButtonState::Pressed
}

pub struct OrbitCameraSystem;

impl System for OrbitCameraSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let input = world.get_input();
        let should_orbit = input.is_action_pressed(katla_ecs::input::Action::LookEnable);
        let should_pan = is_mouse_button_pressed(input, MouseButton::Middle)
            || input.is_action_pressed(katla_ecs::input::Action::PanEnable);
        let scroll = input.mouse_wheel_delta;
        let delta = input.mouse_delta;

        let storage = world.storage_mut();

        // First pass: compute new orbit state and position (query borrow ends after collect).
        let updates: Vec<(katla_ecs::EntityId, OrbitCameraControllerComponent, Vec3, Quat)> = storage
            .query::<(&OrbitCameraControllerComponent, &TransformComponent)>()
            .map(|(entity, orbit, _transform)| {
                let mut orbit = *orbit;

                if should_orbit {
                    orbit.yaw -= orbit.sensitivity * delta.0;
                    orbit.pitch -= orbit.sensitivity * delta.1;
                    let limit = orbit.pitch_limit.max(0.0);
                    orbit.pitch = orbit.pitch.clamp(-limit, limit);
                }

                if should_pan {
                    let rotation = Quat::new_from_yaw_pitch(orbit.yaw, orbit.pitch);
                    let right = rotation.rotate_vec3(Vec3::new(1.0, 0.0, 0.0));
                    let up = Vec3::new(0.0, 1.0, 0.0);
                    orbit.target -= right * delta.0 * orbit.pan_speed * orbit.distance;
                    orbit.target += up * delta.1 * orbit.pan_speed * orbit.distance;
                }

                if scroll.abs() > 0.0 {
                    orbit.distance -= scroll * orbit.zoom_speed * 0.1 * orbit.distance * 0.1;
                    orbit.distance = orbit.distance.clamp(orbit.min_distance, orbit.max_distance);
                }

                let rotation = Quat::new_from_yaw_pitch(orbit.yaw, orbit.pitch);
                let offset = rotation.rotate_vec3(Vec3::new(0.0, 0.0, orbit.distance));
                let position = orbit.target + offset;

                (entity, orbit, position, rotation)
            })
            .collect();

        // Second pass: apply mutations (same storage, borrow from query has ended).
        for (entity, orbit, position, rotation) in updates {
            if let Some(orbit_comp) =
                storage.get_component_mut::<OrbitCameraControllerComponent>(entity)
            {
                *orbit_comp = orbit;
            }
            if let Some(transform) = storage.get_component_mut::<TransformComponent>(entity) {
                transform.transform.position = position;
                transform.transform.rotation = rotation;
            }
        }
    }
}
