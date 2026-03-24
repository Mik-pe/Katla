use katla_ecs::{EntityId, System, World};
use katla_math::{Transform, Vec3};

use crate::components::{
    CameraStateComponent, CharacterStateComponent, Children, ForceComponent, Parent,
    ThirdPersonCameraComponent, ThirdPersonControllerComponent, TransformComponent,
    VelocityComponent,
};

/// Handles character movement for third-person controller.
///
/// Reads input actions (MoveForward, MoveBackward, MoveLeft, MoveRight, Jump, Sprint)
/// and calculates movement direction relative to camera yaw. Applies forces for movement
/// and jumping, and applies gravity when not grounded.
///
/// **Execution Order**: EARLY (before physics)
pub struct ThirdPersonControlSystem;

impl ThirdPersonControlSystem {
    fn get_camera_yaw(world: &World, player_entity: EntityId) -> f32 {
        // Find the camera entity (child of player)
        if let Some(children) = world.get_component::<Children>(player_entity) {
            for &child_entity in &children.children {
                if let Some(_camera) =
                    world.get_component::<ThirdPersonCameraComponent>(child_entity)
                    && let Some(camera_state) =
                        world.get_component::<CameraStateComponent>(child_entity)
                {
                    return camera_state.yaw;
                }
            }
        }
        0.0
    }
}

impl System for ThirdPersonControlSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        // Collect input state first to avoid borrow issues
        let input = world.get_input();
        let forward = input.is_action_pressed(katla_ecs::input::Action::MoveForward) as i32 as f32;
        let backward =
            input.is_action_pressed(katla_ecs::input::Action::MoveBackward) as i32 as f32;
        let left = input.is_action_pressed(katla_ecs::input::Action::MoveLeft) as i32 as f32;
        let right = input.is_action_pressed(katla_ecs::input::Action::MoveRight) as i32 as f32;
        let jump = input.is_action_pressed(katla_ecs::input::Action::Jump);
        let sprint = input.is_action_pressed(katla_ecs::input::Action::Sprint);

        // Collect player entities with their components
        let players: Vec<_> = world
            .query::<(&ThirdPersonControllerComponent, &TransformComponent)>()
            .map(|(entity, controller, transform)| {
                (entity, *controller, transform.transform.position.y())
            })
            .collect();

        for (player_entity, controller, player_y) in players {
            // Check if grounded (y position near zero)
            let is_grounded = player_y <= controller.grounded_threshold;

            // Update character state
            if let Some(char_state) =
                world.get_component_mut::<CharacterStateComponent>(player_entity)
            {
                char_state.is_grounded = is_grounded;
            }

            // Calculate movement direction relative to camera
            let camera_yaw = Self::get_camera_yaw(world, player_entity);

            // Calculate movement direction in camera space
            let z = -(forward - backward); // Negative because forward is -Z
            let x = right - left;

            // Rotate movement direction by camera yaw (only around Y axis)
            let cos_yaw = camera_yaw.cos();
            let sin_yaw = camera_yaw.sin();
            let world_x = x * cos_yaw - z * sin_yaw;
            let world_z = x * sin_yaw + z * cos_yaw;

            let movement_dir = Vec3::new(world_x, 0.0, world_z);
            let movement_dir = movement_dir.normalize();

            // Calculate speed (with sprint multiplier)
            let speed = if sprint {
                controller.walk_speed * controller.sprint_multiplier
            } else {
                controller.walk_speed
            };

            // Apply movement force if there's input
            if movement_dir.length_squared() > 0.01
                && let Some(force) = world.get_component_mut::<ForceComponent>(player_entity)
            {
                force.force += movement_dir * speed;
            }

            // Handle jump
            if jump
                && is_grounded
                && let Some(velocity) = world.get_component_mut::<VelocityComponent>(player_entity)
            {
                velocity.velocity[1] = controller.jump_velocity;
            }

            // Apply gravity if not grounded
            if !is_grounded
                && let Some(force) = world.get_component_mut::<ForceComponent>(player_entity)
            {
                force.force[1] -= controller.gravity;
            }
        }
    }
}

/// Handles camera orbital rotation and zoom for third-person view.
///
/// Reads mouse delta when LookEnable is held (right mouse button) and updates
/// yaw/pitch. Reads mouse wheel for zoom. Updates camera transform relative to player.
///
/// **Execution Order**: LATE (after transform hierarchy, before rendering)
pub struct ThirdPersonCameraSystem;

impl System for ThirdPersonCameraSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let input = world.get_input();
        let should_look = input.is_action_pressed(katla_ecs::input::Action::LookEnable);
        let mouse_delta = input.mouse_delta;
        let mouse_wheel = input.mouse_wheel_delta;

        // Collect camera entities with their parent transforms
        let camera_data: Vec<_> = world
            .query::<(&ThirdPersonCameraComponent, &CameraStateComponent, &Parent)>()
            .map(|(entity, camera, camera_state, parent)| {
                (entity, *camera, *camera_state, parent.parent)
            })
            .collect();

        for (camera_entity, camera, camera_state, player_entity) in camera_data {
            // Update yaw/pitch from mouse input
            let mut updated_state = camera_state;
            if should_look {
                updated_state.yaw -= camera.sensitivity * mouse_delta.0;
                updated_state.pitch -= camera.sensitivity * mouse_delta.1;

                // Clamp pitch
                updated_state.pitch = updated_state
                    .pitch
                    .clamp(camera.min_pitch, camera.max_pitch);
            }

            // Update distance from mouse wheel
            updated_state.current_distance -= camera.zoom_speed * mouse_wheel;
            updated_state.current_distance = updated_state
                .current_distance
                .clamp(camera.min_distance, camera.max_distance);

            // Get player position
            let player_pos = if let Some(player_transform) =
                world.get_component::<TransformComponent>(player_entity)
            {
                player_transform.transform.position
            } else {
                Vec3::new(0.0, 0.0, 0.0)
            };

            // Calculate camera position using spherical coordinates
            let distance = updated_state.current_distance;
            let pitch = updated_state.pitch;
            let yaw = updated_state.yaw;

            // Convert spherical to cartesian
            let cos_pitch = pitch.cos();
            let sin_pitch = pitch.sin();
            let cos_yaw = yaw.cos();
            let sin_yaw = yaw.sin();

            let offset_x = distance * cos_pitch * sin_yaw;
            let offset_y = distance * sin_pitch + camera.height;
            let offset_z = distance * cos_pitch * cos_yaw;

            let camera_pos = player_pos + Vec3::new(offset_x, offset_y, offset_z);

            // Calculate camera rotation to look at player
            let forward = (player_pos - camera_pos).normalize();
            let camera_rotation = Transform::look_direction(forward, Vec3::y_axis()).rotation;

            // Update camera transform and state
            if let Some(transform) = world.get_component_mut::<TransformComponent>(camera_entity) {
                transform.transform = Transform::from_position_rotation_scale(
                    camera_pos,
                    camera_rotation,
                    Vec3::new(1.0, 1.0, 1.0),
                );
            }

            // Update the camera state
            if let Some(state) = world.get_component_mut::<CameraStateComponent>(camera_entity) {
                *state = updated_state;
            }
        }
    }
}
