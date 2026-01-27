use katla_ecs::Component;

#[derive(Component, Debug, Clone, Copy)]
pub struct FlyCameraController {
    pub speed: f32,
    pub sensitivity: f32,
    pub pitch_limit: f32,
}

impl Default for FlyCameraController {
    fn default() -> Self {
        Self {
            speed: 10000.0,
            sensitivity: 0.005,
            pitch_limit: std::f32::consts::FRAC_PI_2 - 0.01,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct FlyCameraLook {
    pub yaw: f32,
    pub pitch: f32,
}
