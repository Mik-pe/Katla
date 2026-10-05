use katla_ecs::Component;
use katla_math::Vec3;

#[derive(Component, Debug, Clone, Copy)]
pub struct FlyCameraControllerComponent {
    pub speed: f32,
    pub sensitivity: f32,
    pub pitch_limit: f32,
}

impl Default for FlyCameraControllerComponent {
    fn default() -> Self {
        Self {
            speed: 100.0,
            sensitivity: 0.005,
            pitch_limit: std::f32::consts::FRAC_PI_2 - 0.01,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct FlyCameraLookComponent {
    pub yaw: f32,
    pub pitch: f32,
    pub velocity: Vec3,
}
