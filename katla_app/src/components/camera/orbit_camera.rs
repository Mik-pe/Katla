use katla_ecs::Component;

#[derive(Component, Debug, Clone, Copy)]
pub struct OrbitCameraControllerComponent {
    pub target: katla_math::Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
    pub sensitivity: f32,
    pub zoom_speed: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub pitch_limit: f32,
}

impl Default for OrbitCameraControllerComponent {
    fn default() -> Self {
        Self {
            target: katla_math::Vec3::new(0.0, 0.5, -3.0),
            distance: 12.0,
            yaw: -0.5,
            pitch: -0.45,
            fov: 60.0,
            sensitivity: 0.005,
            zoom_speed: 1.0,
            min_distance: 0.5,
            max_distance: 100.0,
            pitch_limit: std::f32::consts::FRAC_PI_2 - 0.01,
        }
    }
}
