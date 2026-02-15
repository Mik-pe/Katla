use katla_ecs::{EntityId, World};
use katla_math::{Mat4, Transform, Vec3};

use crate::components::{
    DragComponent, FlyCameraControllerComponent, FlyCameraLookComponent, ForceComponent,
    PerspectiveComponent, TransformComponent, VelocityComponent,
};

pub struct Camera {
    pub entity: EntityId,
}

impl Camera {
    pub fn new(world: &mut World) -> Self {
        let transform = Transform::new_from_position(Vec3::new(0.0, 5.0, 5.0));
        let entity = world.spawn((
            TransformComponent::new(transform),
            VelocityComponent::default(),
            ForceComponent::default(),
            DragComponent::new(0.25),
            PerspectiveComponent::default(),
            FlyCameraControllerComponent::default(),
            FlyCameraLookComponent::default(),
        ));

        Self { entity }
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
            Mat4::create_proj(
                projection.fov,
                projection.aspect_ratio,
                projection.near,
                projection.far,
            )
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
