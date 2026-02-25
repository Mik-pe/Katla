//! Asset registry for managing GPU resources without exposing Vulkan types.
//!
//! The registry stores meshes and materials internally and provides opaque handles
//! for referencing them. This keeps ash::vk types contained within katla_vulkan.

use super::types::*;
use crate::vulkan::*;
use std::{cell::RefCell, rc::Rc};

/// Mesh representation containing Vulkan buffers.
pub struct MeshAsset {
    /// Vertex buffer with geometry data.
    pub vertex_buffer: Option<VertexBuffer>,
    /// Index buffer for indexed drawing.
    pub index_buffer: Option<IndexBuffer>,
}

/// Material representation.
pub struct MaterialAsset {
    /// Graphics pipeline and descriptor sets (shared ownership with interior mutability).
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
    #[allow(dead_code)] // Needed for resource cleanup
    /// Optional texture bound to this material.
    pub texture: Option<Rc<Texture>>,
    #[allow(dead_code)] // Needed for resource cleanup
    /// Vertex binding description.
    pub vertex_binding: VertexBinding,
    /// PBR textures kept alive for the lifetime of the material.
    #[allow(dead_code)]
    pub pbr_textures: Option<Vec<Rc<Texture>>>,
    /// Bindless texture indices: [albedo, normal, metallic_roughness, ao]
    pub texture_indices: [u32; 4],
    /// Emission texture index for bindless.
    pub emission_index: u32,
    /// Whether this material uses bindless textures (has 2 descriptor sets).
    pub uses_bindless: bool,
}

/// Registry for GPU assets.
///
/// Stores meshes and materials internally, providing opaque handles for reference.
/// This prevents ash::vk types from leaking to the application layer.
pub struct AssetRegistry {
    /// Mesh storage - slots can be None to support sparse allocation.
    meshes: Vec<Option<MeshAsset>>,
    /// Material storage.
    materials: Vec<Option<MaterialAsset>>,
    /// Next mesh ID to allocate.
    next_mesh_id: usize,
    /// Next material ID to allocate.
    next_material_id: usize,
}

impl AssetRegistry {
    /// Create a new empty asset registry.
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            materials: Vec::new(),
            next_mesh_id: 0,
            next_material_id: 0,
        }
    }

    /// Register a mesh and return a handle.
    pub(crate) fn register_mesh(&mut self, mesh: MeshAsset) -> MeshHandle {
        let id = self.next_mesh_id;
        self.next_mesh_id += 1;

        // Push None slots until we reach the required index
        while self.meshes.len() <= id {
            self.meshes.push(None);
        }

        self.meshes[id] = Some(mesh);
        MeshHandle(id)
    }

    /// Register a material and return a handle.
    ///
    /// Materials use bindless textures - texture indices should be set in MaterialAsset.
    pub(crate) fn register_material(&mut self, material: MaterialAsset) -> MaterialHandle {
        let id = self.next_material_id;
        self.next_material_id += 1;

        // Push None slots until we reach the required index
        while self.materials.len() <= id {
            self.materials.push(None);
        }

        self.materials[id] = Some(material);
        MaterialHandle(id)
    }

    /// Register a material with PBR textures and return a handle.
    ///
    /// The textures are stored to keep them alive for the lifetime of the material.
    /// Materials use bindless textures - texture indices should be set in MaterialAsset.
    pub(crate) fn register_material_pbr(
        &mut self,
        mut material: MaterialAsset,
        _pbr_textures: crate::vulkan::material::PbrTextureSet,
        textures: Vec<Rc<Texture>>,
    ) -> MaterialHandle {
        // Store textures to keep them alive
        material.pbr_textures = Some(textures);

        let id = self.next_material_id;
        self.next_material_id += 1;

        // Push None slots until we reach the required index
        while self.materials.len() <= id {
            self.materials.push(None);
        }

        self.materials[id] = Some(material);
        MaterialHandle(id)
    }

    /// Get a mesh by handle.
    pub fn get_mesh(&self, handle: MeshHandle) -> Option<&MeshAsset> {
        self.meshes.get(handle.0)?.as_ref()
    }

    /// Get a material by handle (immutable).
    pub fn get_material(&self, handle: MaterialHandle) -> Option<&MaterialAsset> {
        self.materials.get(handle.0)?.as_ref()
    }

    /// Get a mutable material by handle (for rendering updates).
    pub fn get_material_mut(&mut self, handle: MaterialHandle) -> Option<&mut MaterialAsset> {
        self.materials.get_mut(handle.0)?.as_mut()
    }

    /// Update a material's pipeline without destroying the old one (for hot reload).
    ///
    /// This replaces the pipeline Rc pointer, allowing the old pipeline to be
    /// dropped naturally when no longer in use. This is safe for hot reload
    /// because the GPU will finish using the old pipeline before it's actually
    /// destroyed via the Drop trait.
    pub fn replace_material_pipeline(
        &mut self,
        handle: MaterialHandle,
        new_pipeline: std::rc::Rc<std::cell::RefCell<crate::vulkan::material::MaterialPipeline>>,
    ) -> bool {
        if let Some(material) = self.get_material_mut(handle) {
            material.pipeline = new_pipeline;
            true
        } else {
            false
        }
    }

    /// Get the number of registered meshes.
    pub fn mesh_count(&self) -> usize {
        self.meshes.iter().filter(|m| m.is_some()).count()
    }

    /// Get the number of registered materials.
    pub fn material_count(&self) -> usize {
        self.materials.iter().filter(|m| m.is_some()).count()
    }

    /// Get the skeleton descriptor set layout for a material.
    ///
    /// Returns `None` if the material doesn't support skeletal animation.
    pub fn get_skeleton_set_layout(
        &self,
        handle: MaterialHandle,
    ) -> Option<crate::sync::VkDescriptorSetLayout> {
        self.materials.get(handle.0)?.as_ref()?;
        let material = self.materials.get(handle.0)?.as_ref()?;
        material
            .pipeline
            .borrow()
            .skeleton_set_layout
            .map(crate::sync::VkDescriptorSetLayout::new)
    }

    /// Clear all assets from the registry.
    pub fn clear(&mut self) {
        self.meshes.clear();
        self.materials.clear();
        self.next_mesh_id = 0;
        self.next_material_id = 0;
    }
}

impl Default for AssetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetRegistry {
    /// Destroy all registered assets and free GPU resources.
    pub fn destroy(&mut self) {
        self.materials.clear();
        self.meshes.clear();
        self.next_mesh_id = 0;
        self.next_material_id = 0;
    }
}

impl Drop for AssetRegistry {
    fn drop(&mut self) {
        // Clean up any remaining assets as a safety net
        self.destroy();
    }
}
