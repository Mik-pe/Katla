use katla_ecs::Component;

#[derive(Debug, Clone, Copy)]
pub struct FocusTarget {
    pub target: katla_math::Vec3,
    pub distance: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub start_target: katla_math::Vec3,
    pub start_distance: f32,
    pub start_yaw: f32,
    pub start_pitch: f32,
    pub target_yaw: f32,
    pub target_pitch: f32,
}

#[derive(Component, Debug, Clone)]
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
    pub focus: Option<FocusTarget>,
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
            focus: None,
        }
    }
}
