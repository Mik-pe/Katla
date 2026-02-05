use std::collections::HashMap;
use std::rc::Rc;

use katla_vulkan::{MaterialHandle, MeshHandle, VulkanContext, VulkanRenderer};

use crate::rendering::Material;

pub struct MaterialId(pub usize);

pub struct MaterialManager {
    materials: Vec<Material>,
    by_name: HashMap<String, MaterialId>,
}

impl MaterialManager {
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    /// Register a material with a name, returning its ID.
    pub fn create_material(
        &mut self,
        name: impl Into<String>,
        material: Material,
        renderer: &mut VulkanRenderer,
    ) -> MaterialId {
        let name = name.into();

        // Register the material with the renderer to get handles
        let mesh_h = if let Some(first_mesh) = material.meshes.first() {
            let vertex_buffer = first_mesh.vertex_buffer.as_ref().map(|vb| vb.clone());
            let index_buffer = first_mesh.index_buffer.as_ref().map(|ib| ib.clone());
            renderer.register_mesh(vertex_buffer, index_buffer)
        } else {
            MeshHandle(0)
        };

        let mat_h = renderer.create_material(
            material.material_pipeline.clone(),
            material.texture.clone(),
            material.vertex_binding.clone(),
        );

        // Store the handles in the material
        let mut material = material;
        material.mesh_handle = Some(mesh_h);
        material.material_handle = Some(mat_h);

        let id = MaterialId(self.materials.len());
        self.materials.push(material);
        self.by_name.insert(name, id);
        id
    }

    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.0)
    }

    pub fn get_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.get_mut(id.0)
    }

    pub fn get_by_name(&self, name: &str) -> Option<MaterialId> {
        self.by_name.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }
}

impl Default for MaterialManager {
    fn default() -> Self {
        Self::new()
    }
}
