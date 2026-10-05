use katla_ecs::{EntityId, World};
use katla_math::{Mat4, Quat, Transform, Vec3};

use crate::components::{
    EditorHidden, OrbitCameraControllerComponent, PerspectiveComponent, TransformComponent,
};

pub(crate) struct Camera {
    pub entity: EntityId,
}

impl Camera {
    pub fn new(world: &mut World) -> Self {
        let orbit = OrbitCameraControllerComponent::default();
        let position = Self::compute_position(&orbit);

        let transform = Transform {
            position,
            rotation: Quat::new_from_yaw_pitch(orbit.yaw, orbit.pitch),
            scale: Vec3::new(1.0, 1.0, 1.0),
        };

        let entity = world.spawn((
            TransformComponent::new(transform),
            PerspectiveComponent::default(),
            orbit,
            EditorHidden,
        ));

        Self { entity }
    }

    fn compute_position(orbit: &OrbitCameraControllerComponent) -> Vec3 {
        let rotation = Quat::new_from_yaw_pitch(orbit.yaw, orbit.pitch);
        let offset = rotation.rotate_vec3(Vec3::new(0.0, 0.0, orbit.distance));
        orbit.target + offset
    }

    pub fn aspect_ratio_changed(&mut self, world: &mut World, aspect_ratio: f32) {
        if let Some(projection) = world.get_component_mut::<PerspectiveComponent>(self.entity) {
            projection.aspect_ratio = aspect_ratio;
        }
    }

    pub fn get_view_rotation(&self, world: &World) -> Mat4 {
        if let Some(transform) = world.get_component::<TransformComponent>(self.entity) {
            transform.transform.rotation.make_mat4()
        } else {
            Mat4::identity()
        }
    }

    pub fn get_proj_mat(&self, world: &World) -> Mat4 {
        if let Some(projection) = world.get_component::<PerspectiveComponent>(self.entity) {
            Mat4::create_proj_reverse_z(projection.fov, projection.aspect_ratio, projection.near)
        } else {
            Mat4::identity()
        }
    }

    /// Returns the world-to-camera view matrix (inverse of look-at).
    /// Used by GPU shaders as `frame_data.view`.
    pub fn get_view_mat(&self, world: &World) -> Mat4 {
        self.get_lookat_mat(world)
            .inverse()
            .unwrap_or_else(Mat4::identity)
    }

    /// Returns the camera-to-world look-at matrix.
    /// Used for frustum culling via `Frustum::from_proj_and_lookat`.
    pub fn get_lookat_mat(&self, world: &World) -> Mat4 {
        let fwd = Vec3::new(0.0, 0.0, -1.0);
        let to = self.get_view_rotation(world) * fwd;
        if let Some(transform) = world.get_component::<TransformComponent>(self.entity) {
            let pos = transform.transform.position;
            Mat4::create_lookat(pos, pos + to, Vec3::new(0.0, 1.0, 0.0))
        } else {
            Mat4::identity()
        }
    }
}
