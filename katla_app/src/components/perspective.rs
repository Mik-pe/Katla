use katla_ecs::Component;

#[derive(Component, Debug, Clone)]
pub struct Perspective {
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub aspect_ratio: f32,
}

impl Perspective {
    /// Creates a new PerspectiveComponent with the specified name.
    pub fn new(fov: f32, near: f32, far: f32, aspect_ratio: f32) -> Self {
        Self {
            fov,
            near,
            far,
            aspect_ratio,
        }
    }
}

impl Default for Perspective {
    fn default() -> Self {
        Self::new(60.0, 0.001, 10000.0, 16.0 / 9.0)
    }
}
