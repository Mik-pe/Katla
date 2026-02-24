use katla_ecs::Component;

#[derive(Component, Debug, Clone)]
pub struct PerspectiveComponent {
    pub fov: f32,
    pub near: f32,
    pub aspect_ratio: f32,
}

impl PerspectiveComponent {
    /// Creates a new PerspectiveComponent with the specified name.
    pub fn new(fov: f32, near: f32, aspect_ratio: f32) -> Self {
        Self {
            fov,
            near,
            aspect_ratio,
        }
    }
}

impl Default for PerspectiveComponent {
    fn default() -> Self {
        Self::new(60.0, 0.001, 16.0 / 9.0)
    }
}
