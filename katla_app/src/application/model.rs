use std::rc::Rc;

use katla_vulkan::{MaterialHandle, MeshHandle, RenderPass, VulkanContext};

use crate::{
    rendering::{Material, Mesh},
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
