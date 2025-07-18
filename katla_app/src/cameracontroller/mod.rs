use katla_math::{Mat4, Quat, Vec3};

pub mod fpscontrol;
pub mod orbitcontrols;

pub trait CameraController {
    fn tick_camera(&mut self, camera: &mut Camera, dt: f32);
}

pub struct PerspectiveProjection {
    fov: f32,
    near_plane: f32,
    far_plane: f32,
    matrix: Mat4,
}

impl Default for PerspectiveProjection {
    fn default() -> Self {
        let fov = 60.0;
        let near_plane = 0.001;
        let far_plane = 10000.0;
        let matrix = Mat4::create_proj(fov, 1.0, near_plane, far_plane);

        Self {
            fov,
            near_plane,
            far_plane,
            matrix,
        }
    }
}

impl PerspectiveProjection {
    pub fn recreate_matrix(&mut self, aspect_ratio: f32) {
        self.matrix = Mat4::create_proj(self.fov, aspect_ratio, self.near_plane, self.far_plane);
    }
}

pub struct Camera {
    pos: Vec3,
    projection: PerspectiveProjection,
    quat: Quat,
}

impl Camera {
    pub fn new() -> Self {
        

        Self {
            pos: Vec3::new(0.0, 50.0, -450.0),
            projection: PerspectiveProjection::default(),
            quat: Quat::new(),
        }
    }

    pub fn aspect_ratio_changed(&mut self, aspect_ratio: f32) {
        self.projection.recreate_matrix(aspect_ratio);
    }

    // pub fn look_at_sphere(&mut self, sphere: &Sphere) {
    //     self.pos = sphere.center - Vec3::new(0.0, 0.0, sphere.radius * 2.0);
    //     self.yaw = 0.0;
    //     self.pitch = 0.0;
    // }

    // Note to self:
    // This is valid since we are doing some assumptions w.r.t. rotation of the surfacetransformation
    pub fn get_view_rotation(&self) -> Mat4 {
        self.quat.make_mat4()
    }

    // pub fn get_cam_pos(&self) -> Vec3 {
    //     self.pos.clone()
    // }
    pub fn get_proj_mat(&self) -> &Mat4 {
        &self.projection.matrix
    }

    pub fn get_view_mat(&self) -> Mat4 {
        let fwd = Vec3::new(0.0, 0.0, 1.0);
        let to = katla_math::mat4_mul_vec3(&self.get_view_rotation(), &fwd);

        Mat4::create_lookat(
            self.pos,
            self.pos + to,
            Vec3::new(0.0, -1.0, 0.0),
        )
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}
