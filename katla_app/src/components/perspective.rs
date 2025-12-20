use katla_ecs::Component;
use katla_math::Mat4;

#[derive(Component, Debug, Clone)]
pub struct PerspectiveComponent {
    pub fov: f32,
    pub near_plane: f32,
    pub far_plane: f32,
    pub matrix: Mat4,
}

impl PerspectiveComponent {
    /// Creates a new PerspectiveComponent with the specified name.
    pub fn new(fov: f32, near_plane: f32, far_plane: f32) -> Self {
        let matrix = Mat4::create_proj(fov, 1.0, near_plane, far_plane);

        Self {
            fov,
            near_plane,
            far_plane,
            matrix,
        }
    }
}

impl Default for PerspectiveComponent {
    fn default() -> Self {
        Self::new(60.0, 0.001, 10000.0)
    }
}
