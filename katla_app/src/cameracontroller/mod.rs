use katla_ecs::{EntityId, World};
use katla_math::{Mat4, Transform, Vec3};

use crate::components::{
    DragComponent, ForceComponent, PerspectiveComponent, TransformComponent, VelocityComponent,
};

pub mod fpscontrol;

pub trait CameraController {
    fn tick_camera(&mut self, camera: &Camera, world: &mut World, dt: f32);
}

pub struct Camera {
    pub entity: EntityId,
}

impl Camera {
    pub fn new(world: &mut World) -> Self {
        let transform = Transform::new_from_position(Vec3::new(0.0, 50.0, 450.0));
        let id = world.create_entity();
        let transform_component = TransformComponent::new(transform);
        world.add_component(id, transform_component);
        world.add_component(id, VelocityComponent::default());
        world.add_component(id, ForceComponent::default());
        world.add_component(id, DragComponent::new(0.25));
        world.add_component(id, PerspectiveComponent::default());

        Self { entity: id }
    }

    pub fn aspect_ratio_changed(&mut self, world: &mut World, aspect_ratio: f32) {
        if let Some(projection) = world.get_component_mut::<PerspectiveComponent>(self.entity) {
            projection.matrix = Mat4::create_proj(
                projection.fov,
                aspect_ratio,
                projection.near_plane,
                projection.far_plane,
            );
        }
    }

    // Note to self:
    // This is valid since we are doing some assumptions w.r.t. rotation of the surfacetransformation
    pub fn get_view_rotation(&self, world: &World) -> Mat4 {
        if let Some(transform) = world.get_component::<TransformComponent>(self.entity) {
            transform.transform.rotation.make_mat4()
        } else {
            Mat4::identity()
        }
    }

    pub fn get_proj_mat(&self, world: &World) -> Mat4 {
        if let Some(projection) = world.get_component::<PerspectiveComponent>(self.entity) {
            projection.matrix.clone()
        } else {
            Mat4::identity()
        }
    }

    pub fn get_view_mat(&self, world: &World) -> Mat4 {
        let fwd = Vec3::new(0.0, 0.0, -1.0);
        let to = katla_math::mat4_mul_vec3(&self.get_view_rotation(world), &fwd);
        if let Some(transform) = world.get_component::<TransformComponent>(self.entity) {
            let pos = transform.transform.position;
            Mat4::create_lookat(pos, pos + to, Vec3::new(0.0, 1.0, 0.0))
        } else {
            Mat4::identity()
        }
    }
}
