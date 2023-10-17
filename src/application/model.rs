use std::{rc::Rc, sync::Arc};

use katla_math::{Mat4, Quat, Sphere, Transform, Vec3};
use katla_vulkan::{CommandBuffer, RenderPass, VulkanContext};

use crate::{
    rendering::{Drawable, Material, Mesh},
    util::GLTFModel,
};

pub struct Model {
    pub meshes: Vec<Mesh>,
    pub material: Material,
    pub transform: Transform,
    pub bounds: Sphere,
}

impl Model {
    pub fn new_from_gltf(
        model: Rc<GLTFModel>,
        context: Arc<VulkanContext>,
        render_pass: &RenderPass,
        num_images: usize,
        position: Vec3,
    ) -> Self {
        let material = Material::new(model.clone(), context.clone(), render_pass, num_images);
        let mut bounds = model.bounds.clone();
        bounds.center = position;
        let transform = Transform::new_from_position(position);

        let mesh = Mesh::new_from_model(model, context.clone());
        Self {
            meshes: vec![mesh],
            material,
            transform,
            bounds,
        }
    }
}

impl Drawable for Model {
    fn update(&mut self, view: &Mat4, proj: &Mat4) {
        let quat = Quat::new_from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.001);
        self.transform.rotation = self.transform.rotation * quat;
        let model = self.transform.make_mat4();
        self.material
            .upload_pipeline_data(view.clone(), proj.clone(), model);
    }

    fn draw(&self, command_buffer: &CommandBuffer) {
        self.material.bind(command_buffer);

        for mesh in &self.meshes {
            mesh.draw(command_buffer);
        }
    }
}
