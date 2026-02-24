use katla_ecs::Component;

/// Character controller settings
#[derive(Component, Debug, Clone, Copy)]
pub struct ThirdPersonControllerComponent {
    /// Walking speed (units/second)
    pub walk_speed: f32,
    /// Sprinting speed multiplier
    pub sprint_multiplier: f32,
    /// Jump impulse velocity
    pub jump_velocity: f32,
    /// Gravity acceleration (units/second^2)
    pub gravity: f32,
    /// Ground height threshold (y <= grounded_threshold is grounded)
    pub grounded_threshold: f32,
}

impl Default for ThirdPersonControllerComponent {
    fn default() -> Self {
        Self {
            walk_speed: 5.0,
            sprint_multiplier: 1.8,
            jump_velocity: 6.0,
            gravity: 20.0,
            grounded_threshold: 0.01,
        }
    }
}

/// Camera orbital settings
#[derive(Component, Debug, Clone, Copy)]
pub struct ThirdPersonCameraComponent {
    /// Distance from player to camera
    pub distance: f32,
    /// Height offset above player
    pub height: f32,
    /// Minimum pitch angle (radians)
    pub min_pitch: f32,
    /// Maximum pitch angle (radians)
    pub max_pitch: f32,
    /// Mouse rotation sensitivity
    pub sensitivity: f32,
    /// Zoom speed (scroll wheel)
    pub zoom_speed: f32,
    /// Minimum zoom distance
    pub min_distance: f32,
    /// Maximum zoom distance
    pub max_distance: f32,
}

impl Default for ThirdPersonCameraComponent {
    fn default() -> Self {
        Self {
            distance: 5.0,
            height: 2.0,
            min_pitch: -0.3, // ~-17 degrees
            max_pitch: 1.0,  // ~57 degrees
            sensitivity: 0.003,
            zoom_speed: 1.0,
            min_distance: 2.0,
            max_distance: 15.0,
        }
    }
}

/// Current camera state (yaw, pitch, distance)
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CameraStateComponent {
    pub yaw: f32,
    pub pitch: f32,
    pub current_distance: f32,
}

/// Character state (grounded, jumping, etc.)
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CharacterStateComponent {
    pub is_grounded: bool,
}
