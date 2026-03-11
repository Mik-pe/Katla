//! Asset registry for managing GPU resources without exposing Vulkan types.
//!
//! The registry stores meshes and materials internally and provides opaque handles
//! for referencing them. This keeps ash::vk types contained within katla_gfx.

use crate::handle::{MaterialHandle, MeshHandle, PipelineHandle, ResourceStorage};
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

/// Material data for per-material descriptor set (Set 1).
#[derive(Clone, Copy, Debug)]
pub struct MaterialData {
    /// Base color (RGBA)
    pub color: [f32; 4],
    /// Metallic factor (0.0 = dielectric, 1.0 = metal)
    pub metallic: f32,
    /// Roughness factor (0.0 = smooth, 1.0 = rough)
    pub roughness: f32,
    /// Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    pub ao: f32,
    /// Texture indices for bindless: [albedo, normal, metallic_roughness, ao]
    pub texture_indices: [u32; 4],
    /// Emission texture index (0 = no emission)
    pub emission_index: u32,
}

impl Default for MaterialData {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
            texture_indices: [0, 0, 0, 0],
            emission_index: 0,
        }
    }
}

/// Material representation using opaque handles.
pub struct MaterialAsset {
    /// Pipeline handle (references pipeline in registry).
    /// - Some(PipelineHandle) when fully_compiled = true
    /// - None when fully_compiled = false (deferred compilation)
    pub pipeline: Option<PipelineHandle>,
    /// Whether this material has been fully compiled.
    /// When false, the pipeline will be compiled on-demand when first used.
    pub fully_compiled: bool,
    /// Shader path for deferred compilation (set when fully_compiled = false).
    pub shader_path: Option<std::path::PathBuf>,
    /// Vertex binding description.
    pub vertex_binding: VertexBinding,
    /// Per-material data (color, metallic, roughness, etc.)
    pub material_data: MaterialData,
    /// Descriptor set containing material uniforms (Set 1).
    pub material_descriptor_set: Option<vk::DescriptorSet>,
    /// Descriptor set layout for material uniforms (Set 1).
    pub material_descriptor_layout: Option<vk::DescriptorSetLayout>,
}

/// Registry for GPU assets.
///
/// Stores meshes and materials internally, providing opaque handles for reference.
/// This prevents ash::vk types from leaking to the application layer.
pub struct AssetRegistry {
    /// Mesh storage with slot recycling.
    meshes: ResourceStorage<MeshAsset>,
    /// Material storage with slot recycling.
    materials: ResourceStorage<MaterialAsset>,
    /// Pipeline storage with slot recycling.
    pipelines: ResourceStorage<Pipeline>,
}

impl AssetRegistry {
    /// Create a new empty asset registry.
    pub fn new() -> Self {
        Self {
            meshes: ResourceStorage::new(),
            materials: ResourceStorage::new(),
            pipelines: ResourceStorage::new(),
        }
    }

    /// Register a mesh and return a handle.
    pub(crate) fn register_mesh(&mut self, mesh: MeshAsset) -> MeshHandle {
        let id = self.meshes.insert(mesh);
        MeshHandle::new(id)
    }

    /// Register a material and return a handle.
    ///
    /// Materials use bindless textures - texture indices should be set in MaterialAsset.
    pub(crate) fn register_material(&mut self, material: MaterialAsset) -> MaterialHandle {
        let id = self.materials.insert(material);
        MaterialHandle::new(id)
    }

    /// Get a mesh by handle.
    pub fn get_mesh(&self, handle: MeshHandle) -> Option<&MeshAsset> {
        self.meshes.get(handle.index())
    }

    /// Get a mutable mesh by handle (for dynamic updates).
    pub fn get_mesh_mut(&mut self, handle: MeshHandle) -> Option<&mut MeshAsset> {
        self.meshes.get_mut(handle.index())
    }

    /// Get a material by handle (immutable).
    pub fn get_material(&self, handle: MaterialHandle) -> Option<&MaterialAsset> {
        self.materials.get(handle.index())
    }

    /// Get a mutable material by handle (for rendering updates).
    pub fn get_material_mut(&mut self, handle: MaterialHandle) -> Option<&mut MaterialAsset> {
        self.materials.get_mut(handle.index())
    }

    /// Update a material's pipeline handle (for hot reload).
    pub fn replace_material_pipeline(
        &mut self,
        handle: MaterialHandle,
        new_pipeline: PipelineHandle,
    ) {
        if let Some(material) = self.materials.get_mut(handle.index()) {
            material.pipeline = Some(new_pipeline);
        }
    }

    /// Register a pipeline and return a handle.
    pub(crate) fn register_pipeline(&mut self, pipeline: Pipeline) -> PipelineHandle {
        let id = self.pipelines.insert(pipeline);
        PipelineHandle::new(id)
    }

    /// Get a pipeline by handle.
    pub fn get_pipeline(&self, handle: PipelineHandle) -> Option<&Pipeline> {
        self.pipelines.get(handle.index())
    }

    /// Get the Vulkan pipeline and layout handles for rendering.
    pub(crate) fn get_pipeline_vk_handles(
        &self,
        handle: PipelineHandle,
    ) -> Option<(vk::Pipeline, vk::PipelineLayout)> {
        let pipeline = self.pipelines.get(handle.index())?;
        Some((pipeline.vk_pipeline(), pipeline.vk_layout()))
    }

    /// Get the number of registered meshes.
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Get the number of registered materials.
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    /// Clear all assets from the registry.
    pub fn clear(&mut self) {
        self.meshes = ResourceStorage::new();
        self.materials = ResourceStorage::new();
        self.pipelines = ResourceStorage::new();
    }

    /// Destroy all registered assets and free GPU resources.
    pub fn destroy(&mut self) {
        self.clear();
    }
}
