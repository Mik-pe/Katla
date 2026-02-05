use std::rc::Rc;

use katla_math::Mat4;
use katla_vulkan::{CommandBuffer, MaterialHandle, MeshHandle, RenderPass, VulkanContext};

use crate::{
    rendering::{Drawable, Material, Mesh},
    util::GLTFModel,
};

pub struct Model {
    pub meshes: Vec<Mesh>,
    pub material: Material,
    /// Handle after registration (None until registered)
    pub mesh_handle: Option<MeshHandle>,
    /// Handle after registration (None until registered)
    pub material_handle: Option<MaterialHandle>,
}

impl Model {
    pub fn new(meshes: Vec<Mesh>, material: Material) -> Self {
        Self {
            meshes,
            material,
            mesh_handle: None,
            material_handle: None,
        }
    }

    pub fn new_from_gltf(
        model: Rc<GLTFModel>,
        context: Rc<VulkanContext>,
        render_pass: &RenderPass,
    ) -> Self {
        let material = Material::new(model.clone(), context.clone(), render_pass);
        let mesh = Mesh::new_from_model(model, context.clone());
        Self {
            meshes: vec![mesh],
            material,
            mesh_handle: None,
            material_handle: None,
        }
    }
}

impl Drawable for Model {
    fn update(&mut self, view: &Mat4, proj: &Mat4, model_matrix: &Mat4) {
        self.material
            .upload_pipeline_data(view.clone(), proj.clone(), model_matrix.clone());
    }

    fn draw(&self, command_buffer: &CommandBuffer) {
        self.material.bind(command_buffer);

        for mesh in &self.meshes {
            mesh.draw(command_buffer);
        }
    }
}

impl Model {
    /// Get the mesh handle (returns None if not yet registered)
    pub fn mesh_handle(&self) -> Option<MeshHandle> {
        self.mesh_handle
    }

    /// Get the material handle (returns None if not yet registered)
    pub fn material_handle(&self) -> Option<MaterialHandle> {
        self.material_handle
    }
}
