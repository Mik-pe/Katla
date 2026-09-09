//! Asset registry for managing GPU resources without exposing Vulkan types.
//!
//! The registry stores meshes and materials internally and provides opaque handles
//! for referencing them. This keeps ash::vk types contained within katla_gfx.

use crate::handle::{MaterialHandle, MeshHandle, PipelineHandle, ResourceStorage};
use crate::render_graph::RenderGraphError;
use crate::vulkan::material::builder::Pipeline;
use crate::vulkan::material::compute_pipeline::ComputePipeline;
use crate::vulkan::vertex_attribute::AttributeType;
use crate::vulkan::vertexbinding::VertexBinding;
use crate::vulkan::vertexbuffer::{IndexBuffer, VertexBuffer};
use ash::vk;
use std::collections::HashMap;

/// Wrapper for graphics and compute pipelines.
///
/// Stores either a graphics pipeline (for rendering) or a compute pipeline (for GPGPU).
/// Both types have compatible vk::Pipeline and vk::PipelineLayout handles.
pub enum AnyPipeline {
    /// Graphics pipeline for rendering geometry.
    Graphics(Pipeline),
    /// Compute pipeline for GPGPU operations (particle simulation, etc.).
    Compute(ComputePipeline),
}

impl AnyPipeline {
    /// Get the Vulkan pipeline handle.
    pub fn vk_pipeline(&self) -> vk::Pipeline {
        match self {
            AnyPipeline::Graphics(p) => p.vk_pipeline(),
            AnyPipeline::Compute(p) => p.pipeline().vk(),
        }
    }

    /// Get the Vulkan pipeline layout handle.
    pub fn vk_layout(&self) -> vk::PipelineLayout {
        match self {
            AnyPipeline::Graphics(p) => p.vk_layout(),
            AnyPipeline::Compute(p) => p.pipeline_layout().vk(),
        }
    }

    /// Get the descriptor set layouts for this pipeline.
    pub(crate) fn descriptor_set_layouts(&self) -> Vec<vk::DescriptorSetLayout> {
        match self {
            AnyPipeline::Graphics(p) => p.descriptor_set_layouts().to_vec(),
            AnyPipeline::Compute(p) => p.descriptor_set_layouts().to_vec(),
        }
    }
}

/// Mesh representation containing Vulkan buffers.
pub struct MeshAsset {
    /// Per-attribute vertex buffers (SOA layout).
    pub attribute_buffers: HashMap<AttributeType, VertexBuffer>,
    /// Index buffer for indexed drawing.
    pub index_buffer: Option<IndexBuffer>,
    /// Backend-neutral format of `index_buffer`'s indices. Draw paths must bind
    /// this exact format; it is recorded at mesh creation and never re-guessed.
    pub index_format: crate::backend::command::IndexType,
    /// Number of vertices in this mesh.
    pub vertex_count: u32,
    /// Vertex layout the mesh was created with. Backends translate this same
    /// neutral descriptor into native vertex state; pipeline compatibility
    /// (issue #100) keys off it.
    pub layout: crate::vertex::VertexLayout,
    /// Primitive topology. Only `TriangleList` is encodable today; anything
    /// else fails creation with `UnsupportedFeature` instead of rendering as
    /// triangles by accident.
    pub topology: PrimitiveTopology,
    /// Upload policy this mesh was created under. Static meshes are immutable
    /// after creation (issue #96 stages them into GPU-optimal memory);
    /// dynamic meshes are CPU-updatable.
    pub usage: MeshUsage,
}

/// Backend-neutral primitive topology.
///
/// Mirrors the topologies the pipelines can encode. Vulkan and Metal
/// translate the same value; unsupported values fail mesh creation instead
/// of silently rendering as triangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PrimitiveTopology {
    /// Triangle list (the only topology encodable today).
    #[default]
    TriangleList,
    /// Triangle strip.
    TriangleStrip,
    /// Triangle fan.
    TriangleFan,
    /// Points.
    Points,
    /// Line list.
    Lines,
    /// Line strip.
    LineStrip,
}

/// Buffer residency policy for mesh data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MeshUsage {
    /// Immutable after creation. May live in GPU-optimal memory behind a
    /// staging upload (issue #96).
    #[default]
    Static,
    /// CPU-updatable through `update_mesh_dynamic`.
    Dynamic,
}

/// Explicit backend-neutral mesh creation descriptor.
///
/// Replaces `Pod`-shape guessing: the vertex layout comes from a trusted
/// [`crate::vertex::Vertex`] implementation (or an explicit layout plus
/// attribute semantics), the index width from [`MeshIndexElement`], and
/// topology/usage are stated instead of implied. Creation validates the
/// whole descriptor before touching the GPU.
#[derive(Debug, Clone)]
pub struct MeshDescriptor {
    /// Vertex layout: attribute formats in order.
    pub layout: crate::vertex::VertexLayout,
    /// Semantic attribute per layout entry, in the same order.
    pub attributes: Vec<AttributeType>,
    /// Primitive topology.
    pub topology: PrimitiveTopology,
    /// Upload policy.
    pub usage: MeshUsage,
    /// Vertex count.
    pub vertex_count: u32,
    /// Index count (0 for non-indexed meshes).
    pub index_count: u32,
    /// Index element width for indexed meshes.
    pub index_format: crate::backend::command::IndexType,
}

impl MeshDescriptor {
    /// Build a descriptor from trusted type implementations.
    pub fn from_types<V, I>(
        topology: PrimitiveTopology,
        usage: MeshUsage,
        vertex_count: u32,
        index_count: u32,
    ) -> Self
    where
        V: crate::vertex::Vertex,
        I: MeshIndexElement,
    {
        Self {
            layout: V::layout(),
            attributes: V::attribute_kinds(),
            topology,
            usage,
            vertex_count,
            index_count,
            index_format: I::INDEX_FORMAT,
        }
    }

    /// Describe and validate a typed mesh upload without touching the GPU.
    ///
    /// Shared by every backend: topology support, descriptor consistency,
    /// struct stride, and per-index range are all checked before any buffer
    /// is allocated. Returns the validated descriptor for upload.
    pub fn describe_typed_upload<V, I>(
        topology: PrimitiveTopology,
        usage: MeshUsage,
        vertices: &[V],
        indices: &[I],
    ) -> Result<Self, crate::error::RendererError>
    where
        V: crate::vertex::Vertex,
        I: MeshIndexElement,
    {
        use crate::error::RendererError;
        if topology != PrimitiveTopology::TriangleList {
            return Err(RendererError::UnsupportedFeature(format!(
                "mesh topology {topology:?} is not encodable; only TriangleList is supported"
            )));
        }
        let descriptor =
            Self::from_types::<V, I>(topology, usage, vertices.len() as u32, indices.len() as u32);
        descriptor.validate(std::mem::size_of::<V>())?;
        for (position, index) in indices.iter().enumerate() {
            let value = index.to_u32();
            if value >= descriptor.vertex_count {
                return Err(RendererError::InvalidDescriptor {
                    resource: "mesh".to_string(),
                    reason: format!(
                        "index {position} references vertex {value} of {}",
                        descriptor.vertex_count
                    ),
                });
            }
        }
        Ok(descriptor)
    }

    /// Validate internal consistency without touching the GPU.
    ///
    /// Checks attribute/format agreement and the vertex struct stride
    /// against the layout stride. Index ranges are validated against the
    /// vertex count at upload time, where the index data is available.
    pub fn validate(&self, vertex_stride: usize) -> Result<(), crate::error::RendererError> {
        use crate::error::RendererError;
        if self.vertex_count == 0 {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: "empty vertex list".to_string(),
            });
        }
        if self.attributes.len() != self.layout.len() {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: format!(
                    "attribute/layout mismatch: {} semantics for {} formats",
                    self.attributes.len(),
                    self.layout.len()
                ),
            });
        }
        if vertex_stride != self.layout.stride() {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: format!(
                    "vertex stride {vertex_stride} disagrees with layout stride {}",
                    self.layout.stride()
                ),
            });
        }
        if !self.attributes.contains(&AttributeType::Position) {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: "mesh has no Position attribute; draw paths require one".to_string(),
            });
        }
        Ok(())
    }

    /// Validate split (Structure-of-Arrays) attribute data without GPUs.
    ///
    /// Each present buffer must hold exactly `vertex_count` elements of its
    /// layout format, and a Position attribute is required. Empty buffers
    /// are filtered by the caller before building the descriptor.
    pub fn validate_split(
        &self,
        attributes: &std::collections::HashMap<AttributeType, Vec<u8>>,
    ) -> Result<(), crate::error::RendererError> {
        use crate::error::RendererError;
        if self.vertex_count == 0 {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: "empty vertex list".to_string(),
            });
        }
        if !self.attributes.contains(&AttributeType::Position) {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: "mesh has no Position attribute; draw paths require one".to_string(),
            });
        }
        for (kind, format) in self.attributes.iter().zip(self.layout.formats().iter()) {
            let expected = self.vertex_count as usize * format.size_bytes();
            let actual = attributes.get(kind).map(Vec::len).unwrap_or(0);
            if actual != expected {
                return Err(RendererError::InvalidDescriptor {
                    resource: "mesh".to_string(),
                    reason: format!(
                        "attribute {kind:?}: expected {expected} bytes for {} vertices, got {actual}",
                        self.vertex_count
                    ),
                });
            }
        }
        Ok(())
    }
}

/// An index element type whose width is preserved through upload, storage, and
/// draw encoding on every backend.
///
/// Mesh creation accepts only `u16` and `u32` indices; other element types are
/// rejected here at the type level instead of being guessed from byte layout.
pub trait MeshIndexElement: Copy + 'static {
    /// The index format this element type maps to.
    const INDEX_FORMAT: crate::backend::command::IndexType;

    /// Widen one index to `u32` (used by backends that store a single width).
    fn to_u32(self) -> u32;
}

impl MeshIndexElement for u16 {
    const INDEX_FORMAT: crate::backend::command::IndexType =
        crate::backend::command::IndexType::Uint16;

    fn to_u32(self) -> u32 {
        self as u32
    }
}

impl MeshIndexElement for u32 {
    const INDEX_FORMAT: crate::backend::command::IndexType =
        crate::backend::command::IndexType::Uint32;

    fn to_u32(self) -> u32 {
        self
    }
}

impl MeshAsset {
    #[inline]
    pub fn get_attribute_buffer(&self, attr_type: AttributeType) -> Option<&VertexBuffer> {
        self.attribute_buffers.get(&attr_type)
    }

    #[inline]
    pub fn has_attribute(&self, attr_type: AttributeType) -> bool {
        self.attribute_buffers.contains_key(&attr_type)
    }
}

/// Bindless texture indices for a material.
///
/// Stores the indices into the bindless texture array for each PBR texture slot.
/// Layout: [albedo, normal, metallic_roughness, ao].
#[derive(Clone, Copy, Debug)]
pub struct MaterialTextures {
    /// Texture indices for bindless: [albedo, normal, metallic_roughness, ao]
    pub texture_indices: [u32; 4],
}

impl Default for MaterialTextures {
    fn default() -> Self {
        Self {
            texture_indices: [0, 1, 2, 3], // albedo, normal, metallic_roughness, ao
        }
    }
}

/// Material representation using opaque handles.
pub struct MaterialAsset {
    /// Pipeline handle (references pipeline in registry).
    /// - Some(PipelineHandle) when fully_compiled = true
    /// - None when fully_compiled = false (deferred compilation)
    pub pipeline: Option<PipelineHandle>,
    /// Instanced pipeline handle for UI materials.
    /// Compiled with `vs_instanced`/`fs_instanced` entry points and UnitQuadVertex format.
    /// Only set for VertexType::Ui materials.
    pub instanced_pipeline: Option<PipelineHandle>,
    /// Whether this material has been fully compiled.
    /// When false, the pipeline will be compiled on-demand when first used.
    pub fully_compiled: bool,
    /// Shader path for deferred/recompilation.
    pub shader_path: Option<std::path::PathBuf>,
    /// Vertex type used when compiling (Pbr, Ui, Skinned, etc.).
    /// Needed for correct recompilation when descriptor layouts change.
    pub vertex_type: crate::vulkan::material::compiler::VertexType,
    /// Whether this material uses compositing (requires set 2 descriptor set layout).
    pub is_compositing: bool,
    /// Whether alpha blending is enabled for this material.
    /// Must be preserved during recompilation.
    pub alpha_blended: bool,
    /// Whether double-sided rendering is enabled.
    pub double_sided: bool,
    /// Whether wireframe rendering is enabled.
    pub wireframe: bool,
    /// Whether depth testing is enabled for this material.
    pub depth_test: bool,
    /// Vertex binding description.
    pub vertex_binding: VertexBinding,
    /// Bindless texture indices for this material.
    pub textures: MaterialTextures,
    /// Descriptor set containing material uniforms (Set 1).
    pub material_descriptor_set: Option<vk::DescriptorSet>,
    /// Descriptor set layout for material uniforms (Set 1).
    pub material_descriptor_layout: Option<vk::DescriptorSetLayout>,
    /// Color attachment format this material was compiled for.
    /// Used for recompilation when descriptor layouts change (e.g., resize).
    pub color_format: crate::texture::ImageFormat,
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
    /// Pipeline storage with slot recycling (graphics and compute).
    pipelines: ResourceStorage<AnyPipeline>,
}

impl Default for AssetRegistry {
    fn default() -> Self {
        Self::new()
    }
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

    /// Register a graphics pipeline and return a handle.
    pub(crate) fn register_pipeline(&mut self, pipeline: Pipeline) -> PipelineHandle {
        let id = self.pipelines.insert(AnyPipeline::Graphics(pipeline));
        PipelineHandle::new(id)
    }

    /// Register a compute pipeline and return a handle.
    pub fn register_compute_pipeline(&mut self, pipeline: ComputePipeline) -> PipelineHandle {
        let id = self.pipelines.insert(AnyPipeline::Compute(pipeline));
        PipelineHandle::new(id)
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

    /// Get the Vulkan pipeline and layout handles for rendering.
    pub(crate) fn get_pipeline_vk_handles(
        &self,
        handle: PipelineHandle,
    ) -> Option<(vk::Pipeline, vk::PipelineLayout)> {
        let pipeline = self.pipelines.get(handle.index())?;
        Some((pipeline.vk_pipeline(), pipeline.vk_layout()))
    }

    /// Get the Vulkan pipeline and layout handles, returning an error if the handle is invalid.
    pub(crate) fn get_pipeline_handles(
        &self,
        handle: PipelineHandle,
    ) -> Result<(vk::Pipeline, vk::PipelineLayout), RenderGraphError> {
        self.get_pipeline_vk_handles(handle)
            .ok_or(RenderGraphError::InvalidPipelineHandle(handle))
    }

    /// Get a pipeline by handle.
    pub fn get_pipeline(&self, handle: PipelineHandle) -> Option<&AnyPipeline> {
        self.pipelines.get(handle.index())
    }

    /// Get the number of registered meshes.
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Get the number of registered materials.
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    /// Find all materials whose shader path matches the given file name.
    ///
    /// Used for shader hot reload to identify which materials need recompilation
    /// when a shader file changes on disk.
    pub fn materials_for_shader(
        &self,
        shader_path: &std::path::Path,
    ) -> Vec<(MaterialHandle, std::path::PathBuf)> {
        let file_name = shader_path.file_name();
        self.materials
            .iter_enumerated()
            .filter_map(|(idx, mat)| {
                let sp = mat.shader_path.as_ref()?;
                if sp.file_name() == file_name {
                    Some((MaterialHandle::new(idx), sp.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Remove a mesh by handle, returning the removed asset for GPU cleanup.
    ///
    /// Returns `None` if the handle is invalid or already removed.
    pub fn remove_mesh(&mut self, handle: MeshHandle) -> Option<MeshAsset> {
        self.meshes.remove(handle.index())
    }

    /// Remove a material by handle, returning the removed asset for GPU cleanup.
    ///
    /// Returns `None` if the handle is invalid or already removed.
    pub fn remove_material(&mut self, handle: MaterialHandle) -> Option<MaterialAsset> {
        self.materials.remove(handle.index())
    }

    /// Clear all assets from the registry.
    pub fn clear(&mut self) {
        self.meshes = ResourceStorage::new();
        self.materials = ResourceStorage::new();
        self.pipelines = ResourceStorage::new();
    }

    /// Remove a pipeline by handle.
    pub(crate) fn remove_pipeline(&mut self, handle: PipelineHandle) -> Option<AnyPipeline> {
        self.pipelines.remove(handle.index())
    }

    /// Invalidate all compiled materials and destroy their pipelines.
    ///
    /// Called after descriptor layout changes (e.g., light culling resize)
    /// to ensure pipelines reference valid descriptor set layouts.
    /// Deferred materials are marked for recompilation on next use.
    pub fn invalidate_compiled_materials(&mut self) {
        // Mark all compiled materials for recompilation and collect their pipeline handles
        let mut pipelines_to_destroy = Vec::new();
        for material in self.materials.iter_mut() {
            if material.fully_compiled && material.shader_path.is_some() {
                material.fully_compiled = false;
                if let Some(pipeline_handle) = material.pipeline.take() {
                    pipelines_to_destroy.push(pipeline_handle);
                }
                if let Some(pipeline_handle) = material.instanced_pipeline.take() {
                    pipelines_to_destroy.push(pipeline_handle);
                }
            }
        }
        // Only destroy the specific material pipelines, not all pipelines
        for handle in pipelines_to_destroy {
            self.pipelines.remove(handle.index());
        }
    }

    /// Destroy all registered assets and free GPU resources.
    pub fn destroy(&mut self) {
        self.clear();
    }
}
