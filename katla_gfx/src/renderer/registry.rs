//! Asset registry for managing GPU resources without exposing Vulkan types.
//!
//! The registry stores meshes and materials internally and provides opaque handles
//! for referencing them. This keeps ash::vk types contained within katla_gfx.

use crate::handle::{MaterialHandle, MeshHandle, PipelineHandle};
use crate::vulkan::material::builder::Pipeline;
use crate::{IndexBuffer, VertexBinding, VertexBuffer};
use ash::vk;

/// Mesh representation containing Vulkan buffers.
pub struct MeshAsset {
    /// Vertex buffer with geometry data.
    pub vertex_buffer: Option<VertexBuffer>,
    /// Index buffer for indexed drawing.
    pub index_buffer: Option<IndexBuffer>,
}

/// Material representation using opaque handles.
pub struct MaterialAsset {
    /// Pipeline handle (references pipeline in registry).
    pub pipeline: PipelineHandle,
    /// Vertex binding description.
    pub vertex_binding: VertexBinding,
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
    /// Pipeline storage - actual Pipeline objects.
    pipelines: Vec<Option<Pipeline>>,
    /// Next mesh ID to allocate.
    next_mesh_id: usize,
    /// next material ID to allocate.
    next_material_id: usize,
    /// Next pipeline ID to allocate.
    next_pipeline_id: usize,
}

impl AssetRegistry {
    /// Create a new empty asset registry.
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            materials: Vec::new(),
            pipelines: Vec::new(),
            next_mesh_id: 0,
            next_material_id: 0,
            next_pipeline_id: 0,
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
        MeshHandle::new(id as u32)
    }

    /// Register a material and return a handle.
    ///
    /// Materials use bindless textures - texture indices should be set in MaterialAsset.
    pub(crate) fn register_material(&mut self, material: MaterialAsset) -> MaterialHandle {
        let id = self.next_material_id;
        self.next_material_id += 1;

        while self.materials.len() <= id {
            self.materials.push(None);
        }

        self.materials[id] = Some(material);
        MaterialHandle::new(id as u32)
    }

    /// Get a mesh by handle.
    pub fn get_mesh(&self, handle: MeshHandle) -> Option<&MeshAsset> {
        self.meshes.get(handle.index() as usize)?.as_ref()
    }

    /// Get a mutable mesh by handle (for dynamic updates).
    pub fn get_mesh_mut(&mut self, handle: MeshHandle) -> Option<&mut MeshAsset> {
        self.meshes.get_mut(handle.index() as usize)?.as_mut()
    }

    /// Get a material by handle (immutable).
    pub fn get_material(&self, handle: MaterialHandle) -> Option<&MaterialAsset> {
        self.materials.get(handle.index() as usize)?.as_ref()
    }

    /// Get a mutable material by handle (for rendering updates).
    pub fn get_material_mut(&mut self, handle: MaterialHandle) -> Option<&mut MaterialAsset> {
        self.materials.get_mut(handle.index() as usize)?.as_mut()
    }

    /// Update a material's pipeline handle (for hot reload).
    pub fn replace_material_pipeline(
        &mut self,
        handle: MaterialHandle,
        new_pipeline: PipelineHandle,
    ) {
        if let Some(Some(material)) = self.materials.get_mut(handle.index() as usize) {
            material.pipeline = new_pipeline;
        }
    }

    /// Register a pipeline and return a handle.
    pub(crate) fn register_pipeline(&mut self, pipeline: Pipeline) -> PipelineHandle {
        let id = self.next_pipeline_id;
        self.next_pipeline_id += 1;

        while self.pipelines.len() <= id {
            self.pipelines.push(None);
        }

        self.pipelines[id] = Some(pipeline);
        PipelineHandle::new(id as u32)
    }

    /// Get a pipeline by handle.
    pub fn get_pipeline(&self, handle: PipelineHandle) -> Option<&Pipeline> {
        self.pipelines.get(handle.index() as usize)?.as_ref()
    }

    /// Get the Vulkan pipeline and layout handles for rendering.
    pub(crate) fn get_pipeline_vk_handles(
        &self,
        handle: PipelineHandle,
    ) -> Option<(vk::Pipeline, vk::PipelineLayout)> {
        let pipeline = self.pipelines.get(handle.index() as usize)?.as_ref()?;
        Some((pipeline.vk_pipeline(), pipeline.vk_layout()))
    }

    /// Get the number of registered meshes.
    pub fn mesh_count(&self) -> usize {
        self.meshes.iter().filter(|m| m.is_some()).count()
    }

    /// Get the number of registered materials.
    pub fn material_count(&self) -> usize {
        self.materials.iter().filter(|m| m.is_some()).count()
    }

    /// Clear all assets from the registry.
    pub fn clear(&mut self) {
        self.meshes.clear();
        self.materials.clear();
        self.pipelines.clear();
        self.next_mesh_id = 0;
        self.next_material_id = 0;
        self.next_pipeline_id = 0;
    }

    /// Destroy all registered assets and free GPU resources.
    pub fn destroy(&mut self) {
        self.materials.clear();
        self.meshes.clear();
        self.pipelines.clear();
        self.next_mesh_id = 0;
        self.next_material_id = 0;
        self.next_pipeline_id = 0;
    }
}

