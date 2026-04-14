use katla_ecs::{ComponentAccess, System, World};
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

impl System for OrbitCameraSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        let Some(input) = world.get_resource::<InputState>() else {
            return;
        };
        let should_orbit = input.is_action_pressed(Action::LookEnable);
        let should_pan = is_mouse_button_pressed(input, MouseButton::Middle)
            || input.is_action_pressed(Action::PanEnable);
        let scroll = input.mouse_wheel_delta;
        let delta = input.mouse_delta;

        // First pass: compute new orbit state and position (query borrow ends after collect).
        let updates: Vec<(
            katla_ecs::EntityId,
            OrbitCameraControllerComponent,
            Vec3,
            Quat,
        )> = world
            .query::<(&OrbitCameraControllerComponent, &TransformComponent)>()
            .map(|(entity, orbit, _transform)| {
                let mut orbit = orbit.clone();

                // Update focus animation
                if let Some(focus) = &mut orbit.focus {
                    focus.elapsed += delta_time;
                    let t = (focus.elapsed / focus.duration).min(1.0);
                    let t = smoothstep(t);

                    orbit.target = focus.start_target + (focus.target - focus.start_target) * t;
                    orbit.distance =
                        focus.start_distance + (focus.distance - focus.start_distance) * t;
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

                (entity, orbit, position, rotation)
            })
            .collect();

        // Second pass: apply mutations
        for (entity, orbit, position, rotation) in updates {
            if let Some(orbit_comp) =
                world.get_component_mut::<OrbitCameraControllerComponent>(entity)
            {
                *orbit_comp = orbit;
            }
            if let Some(transform) = world.get_component_mut::<TransformComponent>(entity) {
                transform.transform.position = position;
                transform.transform.rotation = rotation;
            }
        }
    }

    fn component_access() -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::write::<OrbitCameraControllerComponent>(),
            ComponentAccess::write::<TransformComponent>(),
        ]
    }

    fn component_access_dyn(&self) -> Vec<ComponentAccess> {
        vec![
            ComponentAccess::write::<OrbitCameraControllerComponent>(),
            ComponentAccess::write::<TransformComponent>(),
        ]
    }
}
