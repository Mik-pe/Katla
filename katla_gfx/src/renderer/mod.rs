//! Vulkan renderer implementation modules.
//!
//! This module organizes VulkanRenderer methods into logical groups:
//!
//! - `frame` - Frame rendering and swapchain management
//! - `viewport` - Viewport system management
//! - `ui` - UI buffer and texture management

// Backend-agnostic modules (always available)
pub(crate) mod types;

pub mod any_renderer;
pub mod features;
pub mod frame_bindings;
pub mod gpu_renderer;
pub mod graphics_interface;
pub mod pipeline_descriptor;
pub mod pipeline_variant;
pub mod retirement;
pub mod texture_readback;
mod vulkan_core;

pub(crate) mod bindless_queries;
pub(crate) mod buffer_api;
#[cfg(test)]
mod capture_tests;
pub(crate) mod destroy_api;
pub(crate) mod frame_lifecycle;
pub mod frame_scope;
mod graph_readback;
pub(crate) mod material_api;
pub(crate) mod mesh_manager;
pub(crate) mod registry;
pub(crate) mod skeleton_api;
pub(crate) mod texture_api;
pub(crate) mod ui_renderer;

// Public re-exports (always available — backend-agnostic types)
pub use crate::handle::{
    Handle, MaterialHandle, MeshHandle, PipelineHandle, SkeletonHandle, TextureHandle,
};
pub use features::RendererFeature;
pub use frame_scope::{FrameAcquisition, FrameToken, PresentOutcome, SurfaceStatus};
pub use pipeline_descriptor::{
    BlendMode, DepthState, PipelineDescriptor, PipelineStages, SpecializationValue,
};
pub use types::{
    DrawCall, DrawList, GpuCapabilities, GpuTimestamp, GpuVendor, InstanceData, PointLightGPU,
    PreparedDrawCounts, PreparedDraws, UIDrawList, UiDrawCommand,
};

// Vulkan re-exports
pub use crate::error::ValidationMode;
pub use registry::AssetRegistry;

use crate::error::RendererError;
use crate::handle::{BufferMarker, ResourceStorage, SkeletonMarker};
use crate::renderer::retirement::RetirementSnapshot;
use crate::texture::{TextureDescriptor, TextureManager};
use crate::vulkan::IndexType;
use crate::vulkan::bindless_texture::{BindlessTextureManager, MAX_BINDLESS_TEXTURES};
use crate::vulkan::context::VulkanContext;
use crate::vulkan::context::VulkanFrameCtx;
use crate::vulkan::material::compiler::MaterialCompiler;
use crate::vulkan::material::storage_uniform::StorageUniformManager;
use crate::vulkan::retirement::{
    FrameRetirements, RetiredBuffer, RetiredResource, RetirementQueue,
};
use crate::vulkan::swapdata::SwapData;
use crate::vulkan::vertex_attribute::AttributeType;
use crate::vulkan::vertexbuffer::{IndexBuffer, VertexBuffer};
use ash::vk;
use log::{error, info};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{ffi::CString, rc::Rc};

/// Per-frame UI rendering resources.
pub(crate) struct UiFrameResources {
    /// Per-frame UI vertex buffers (complex geometry).
    pub vertex_buffers: Vec<VertexBuffer>,
    /// Per-frame UI index buffers (complex geometry).
    pub index_buffers: Vec<IndexBuffer>,
    /// Per-frame UI instance buffers (instanced quads).
    pub instance_buffers: Vec<VertexBuffer>,
    /// Per-frame unit quad vertex buffers.
    pub unit_quad_vertex_buffers: Vec<VertexBuffer>,
    /// Per-frame unit quad index buffers.
    pub unit_quad_index_buffers: Vec<IndexBuffer>,
    /// Per-frame UI descriptor sets (owns both set and pool, automatic cleanup).
    pub descriptor_sets: Vec<Option<crate::vulkan::descriptor_set::DescriptorSet>>,
    /// Screen-size uniform allocation for each frame slot.
    pub uniform_buffers: Vec<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,
}

impl UiFrameResources {
    /// Create new UI frame resources with pre-allocated buffers.
    fn new(context: &Rc<VulkanContext>) -> Self {
        let mut vertex_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut index_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut instance_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut unit_quad_vertex_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut unit_quad_index_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut descriptor_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);

        for _ in 0..FRAMES_IN_FLIGHT {
            vertex_buffers.push(VertexBuffer::new(context.clone(), 1024 * 1024, 65536));
            index_buffers.push(IndexBuffer::new(
                context.clone(),
                1024 * 1024,
                IndexType::Uint32,
                65536,
            ));
            instance_buffers.push(VertexBuffer::with_usage(
                context.clone(),
                1024 * 1024,
                65536,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            ));
            unit_quad_vertex_buffers.push(VertexBuffer::new(
                context.clone(),
                256, // 4 vertices × 8 bytes = 32 bytes, small
                4,
            ));
            unit_quad_index_buffers.push(IndexBuffer::new(
                context.clone(),
                256, // 6 indices × 4 bytes = 24 bytes, small
                IndexType::Uint32,
                6,
            ));
            descriptor_sets.push(None);
        }

        // UI uniform buffer (screen_size) - 16 bytes, CPU-visible
        let uniform_buffer_info = vk::BufferCreateInfo::default()
            .size(16) // 4 floats: screen_size[2] + padding[2]
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let uniform_buffers = (0..FRAMES_IN_FLIGHT)
            .map(|_| {
                context
                    .allocate_buffer(
                        &uniform_buffer_info,
                        gpu_allocator::MemoryLocation::CpuToGpu,
                    )
                    .expect("Failed to allocate UI uniform buffer")
            })
            .collect();

        Self {
            vertex_buffers,
            index_buffers,
            instance_buffers,
            unit_quad_vertex_buffers,
            unit_quad_index_buffers,
            descriptor_sets,
            uniform_buffers,
        }
    }
}

pub struct VulkanRenderer {
    pub(crate) context: Rc<VulkanContext>,
    pub(crate) frame_context: VulkanFrameCtx,
    pub(crate) swap_data: SwapData,
    /// Replaced native buffers waiting for their last in-flight submission
    /// to complete (dynamic mesh growth). Drained after each frame-slot
    /// fence wait; everything frees after the device idle wait in destroy().
    pub(crate) retirements: RetirementQueue,
    /// Mesh manager for mesh creation and storage.
    pub(crate) mesh_manager: mesh_manager::MeshManager,
    /// Asset registry for managing GPU resources (materials).
    /// This stores the actual pipelines, while the application
    /// only holds opaque handles (MaterialHandle).
    pub asset_registry: AssetRegistry,
    /// Bindless texture manager for efficient texture binding.
    /// All textures are stored in a single array accessed by index.
    /// Texture indices are passed via ObjectUniforms.texture_indices.
    pub(crate) bindless_manager: BindlessTextureManager,
    /// Centralized texture manager for handle-based texture creation.
    /// Provides a clean API for creating and looking up textures by handle.
    pub texture_manager: TextureManager,
    /// Storage uniform manager for storage buffer-based uniforms.
    /// Materials use storage buffers with instance indexing.
    pub(crate) storage_manager: StorageUniformManager,
    /// Per-slot ordinary buffers associated with a skeleton handle.
    pub(crate) skeleton_buffers: ResourceStorage<Vec<crate::BufferHandle>, SkeletonMarker>,
    /// Backend-neutral buffer resources addressable from render graphs.
    pub(crate) graph_buffers:
        ResourceStorage<crate::render_graph::transient_buffer::VulkanGraphBuffer, BufferMarker>,
    pub(crate) pending_texture_exports: Vec<graph_readback::VulkanTextureExport>,
    pub(crate) committed_texture_exports: std::collections::HashMap<
        crate::render_graph::ResourceId,
        graph_readback::VulkanTextureExport,
    >,
    pub(crate) texture_readbacks:
        std::collections::HashMap<u64, graph_readback::VulkanTextureReadback>,
    pub(crate) graphics_descriptor_sets: Vec<Vec<crate::vulkan::descriptor_set::DescriptorSet>>,
    pub(crate) graphics_constants:
        Vec<Vec<crate::render_graph::transient_buffer::VulkanGraphBuffer>>,
    pub(crate) graphics_image_views: Vec<Vec<vk::ImageView>>,
    pub(crate) graphics_samplers:
        std::collections::HashMap<super::renderer::frame_bindings::SamplingMode, vk::Sampler>,
    pub(crate) pending_graph_buffers: std::collections::HashSet<u64>,
    pub(crate) graph_buffer_consumers: std::collections::HashMap<u64, Option<vk::Fence>>,
    pub(crate) graph_compute_pipelines: std::collections::HashMap<
        crate::render_graph::ComputePipelineDesc,
        crate::render_graph::vulkan_compute::VulkanGraphComputePipeline,
    >,
    /// Last presented swapchain image index (for debugging readback).
    pub(super) last_presented_image_index: Option<u32>,
    /// Material compiler for compiling materials from shaders.
    pub(crate) material_compiler: MaterialCompiler,
    /// Optional scratch storage for native UI encoding.
    pub(crate) ui_renderer: ui_renderer::UIRenderer,
    /// The currently open frame-scoped token (see `renderer::frame_scope`).
    active_frame: Option<crate::renderer::frame_scope::FrameToken>,
    frame_rendered: bool,
    pub(crate) last_submission: Option<(usize, u64, Option<vk::Fence>)>,
    pub(crate) surface_recreation_required: bool,
    /// Monotonic counter handed to successive acquired frames.
    pub(crate) frame_generation: u64,
    /// Why the open frame is poisoned (a render failure); `present` refuses to submit.
    frame_poisoned: Option<String>,
    /// GPU hardware capabilities and limits.
    pub(crate) capabilities: types::GpuCapabilities,
    /// Whether destroy() has already been called.
    destroyed: bool,
}

/// Number of frames that can be processed concurrently.
/// This is an implementation detail for double-buffering.
pub(crate) const FRAMES_IN_FLIGHT: usize = 2;

/// Maximum number of objects that can be drawn per frame.
///
/// This is the limit of the storage buffer array that holds per-object data.
/// Each draw call uses one slot indexed by `instance_index`. If you exceed
/// this limit, `execute_draw_calls` will return a `RendererError::ObjectLimitExceeded`.
pub const MAX_OBJECTS_PER_FRAME: u32 = 256;

/// Private initialization helpers for VulkanRenderer.
impl VulkanRenderer {
    /// Wrap a fallible initialization step, converting errors to RendererError::InitializationFailed.
    fn init_step<T, E: std::fmt::Debug>(
        label: &str,
        result: Result<T, E>,
    ) -> Result<T, RendererError> {
        result.map_err(|e| {
            error!("Failed to create {}: {:?}", label, e);
            RendererError::InitializationFailed(format!("Failed to create {}: {:?}", label, e))
        })
    }

    pub fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        size: crate::Size2D,
        validation_mode: ValidationMode,
        app_name: CString,
        engine_name: CString,
    ) -> Result<Self, RendererError> {
        let context = Rc::new(VulkanContext::init(
            display,
            window,
            validation_mode,
            app_name,
            engine_name,
        )?);

        let frame_context = VulkanFrameCtx::init(
            &context,
            vk::Extent2D {
                width: size.width,
                height: size.height,
            },
        )?;
        Self::init_with_context(context, frame_context, validation_mode)
    }

    /// Create a Vulkan renderer without a window or presentation surface.
    pub fn init_headless(
        width: u32,
        height: u32,
        validation_mode: ValidationMode,
        app_name: CString,
        engine_name: CString,
    ) -> Result<Self, RendererError> {
        if width == 0 || height == 0 {
            return Err(RendererError::InitializationFailed(
                "Headless dimensions must be nonzero".into(),
            ));
        }
        let context = Rc::new(VulkanContext::init_headless(
            validation_mode,
            app_name,
            engine_name,
        )?);
        let frame_context =
            VulkanFrameCtx::init_headless(&context, vk::Extent2D { width, height })?;
        Self::init_with_context(context, frame_context, validation_mode)
    }

    fn init_with_context(
        context: Rc<VulkanContext>,
        frame_context: VulkanFrameCtx,
        validation_mode: ValidationMode,
    ) -> Result<Self, RendererError> {
        if validation_mode.is_enabled() {
            context.setup_validation_logging();
        }
        let gpu_capabilities = {
            use crate::renderer::types::{GpuCapabilities, GpuVendor};
            let props = unsafe {
                context
                    .instance
                    .get_physical_device_properties(context.physical_device)
            };
            let vendor = match props.vendor_id {
                0x10DE => GpuVendor::Nvidia,
                0x1002 => GpuVendor::Amd,
                0x8086 => GpuVendor::Intel,
                0x106B => GpuVendor::Apple,
                _ => GpuVendor::Unknown,
            };
            GpuCapabilities {
                max_texture_size: props.limits.max_image_dimension2_d,
                max_bindless_textures: MAX_BINDLESS_TEXTURES,
                supports_compute: true,
                max_frames_in_flight: FRAMES_IN_FLIGHT,
                vendor,
                clip_y_down: true,
            }
        };

        let swap_data = SwapData::new(
            &context.device,
            &frame_context
                .swapchain_images
                .iter()
                .map(|img| img.vk())
                .collect::<Vec<_>>(),
            FRAMES_IN_FLIGHT,
        )?;

        let mut texture_manager = TextureManager::new(context.clone())?;
        let fallback_handle = texture_manager.default_texture();
        let fallback = texture_manager
            .get_texture_rc(fallback_handle)
            .ok_or_else(|| {
                RendererError::InitializationFailed("Descriptor fallback texture is absent".into())
            })?;
        let bindless_manager = Self::init_step(
            "bindless texture manager",
            BindlessTextureManager::new(&context, fallback),
        )?;
        texture_manager.register_bindless_slot(fallback_handle, 0);
        info!(
            "Texture system initialized (max {} textures)",
            MAX_BINDLESS_TEXTURES
        );

        let storage_manager = StorageUniformManager::new(&context, FRAMES_IN_FLIGHT)?;

        let mesh_manager = mesh_manager::MeshManager::new(context.clone());

        let material_compiler = MaterialCompiler::new(context.clone(), &bindless_manager);
        info!("Material compiler initialized");

        Ok(Self {
            context,
            frame_context,
            swap_data,
            retirements: RetirementQueue::new(),
            mesh_manager,
            asset_registry: AssetRegistry::new(),
            bindless_manager,
            texture_manager,
            storage_manager,
            skeleton_buffers: ResourceStorage::new(),
            graph_buffers: ResourceStorage::new(),
            pending_texture_exports: Vec::new(),
            committed_texture_exports: Default::default(),
            texture_readbacks: Default::default(),
            graphics_descriptor_sets: (0..FRAMES_IN_FLIGHT).map(|_| Vec::new()).collect(),
            graphics_constants: (0..FRAMES_IN_FLIGHT).map(|_| Vec::new()).collect(),
            graphics_image_views: (0..FRAMES_IN_FLIGHT).map(|_| Vec::new()).collect(),
            graphics_samplers: Default::default(),
            pending_graph_buffers: Default::default(),
            graph_buffer_consumers: Default::default(),
            graph_compute_pipelines: std::collections::HashMap::new(),
            last_presented_image_index: None,
            material_compiler,
            ui_renderer: ui_renderer::UIRenderer::new(),
            active_frame: None,
            frame_rendered: false,
            last_submission: None,
            surface_recreation_required: false,
            frame_generation: 0,
            frame_poisoned: None,
            capabilities: gpu_capabilities,
            destroyed: false,
        })
    }

    /// Get the Vulkan context.
    ///
    /// This provides access to low-level Vulkan resources. Most operations should use
    /// higher-level VulkanRenderer methods instead.
    ///
    /// # Safety
    ///
    /// The caller must ensure proper synchronization when using the context.
    pub fn context(&self) -> &Rc<VulkanContext> {
        &self.context
    }

    /// Get the swapchain extent (primary window size).
    pub fn swapchain_extent(&self) -> crate::Size2D {
        let ext = self.frame_context.extent;
        crate::Size2D::new(ext.width, ext.height)
    }

    /// Register a texture image view with the bindless texture system.
    ///
    /// Returns the bindless slot index that can be used to sample this texture
    /// from shaders using the bindless texture array.
    ///
    /// # Arguments
    /// * `image_view` - Vulkan image view handle
    ///
    /// # Returns
    /// The bindless texture slot index (u32)
    ///
    /// # Example
    /// ```ignore
    /// let slot = renderer.register_bindless_texture(image_view)?;
    /// // Pass slot to shader via object_uniforms.texture_indices.x
    /// ```
    pub fn register_bindless_texture(
        &mut self,
        image_view: vk::ImageView,
    ) -> Result<u32, RendererError> {
        self.bindless_manager
            .register_texture(image_view)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to register bindless texture: {}",
                    e
                ))
            })
    }

    /// Update an existing bindless texture slot with a new image view.
    ///
    /// This is used when a texture is recreated (e.g., after window resize) and the
    /// bindless descriptor needs to be updated with the new image view.
    ///
    /// # Arguments
    /// * `slot` - The bindless slot to update
    /// * `image_view` - The new image view
    ///
    /// # Returns
    /// Ok(()) if successful, or an error if the slot is invalid.
    ///
    /// # Example
    /// ```ignore
    /// // After recreating a texture, update the bindless descriptor:
    /// renderer.update_bindless_texture(slot, new_image_view)?;
    /// ```
    pub fn update_bindless_texture(
        &mut self,
        slot: u32,
        image_view: vk::ImageView,
    ) -> Result<(), RendererError> {
        self.bindless_manager
            .update_texture(slot, image_view)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to update bindless texture slot {}: {}",
                    slot, e
                ))
            })
    }

    /// Retire submitted work and release native resource owners.
    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;
        self.wait_for_device();

        self.texture_readbacks.clear();
        self.pending_texture_exports.clear();
        self.committed_texture_exports.clear();
        // Every submission has completed: retired resources can free now and
        // staged uploads release their fences and staging allocations.
        self.drain_retirements_all();
        self.context.wait_and_drain_all_staged_uploads();

        for sets in &mut self.graphics_descriptor_sets {
            sets.clear();
        }
        for buffers in &mut self.graphics_constants {
            buffers.clear();
        }
        for views in &mut self.graphics_image_views {
            for view in views.drain(..) {
                unsafe {
                    self.context.device.destroy_image_view(view, None);
                }
            }
        }
        for (_, sampler) in self.graphics_samplers.drain() {
            unsafe {
                self.context.device.destroy_sampler(sampler, None);
            }
        }

        // Destroy all registered assets first (materials, meshes)
        self.asset_registry.destroy();

        // Destroy material compiler (cleans up descriptor layouts)
        self.material_compiler.destroy();

        self.ui_renderer.destroy(&self.context);

        self.context.pre_destroy();
        self.skeleton_buffers = ResourceStorage::new();
        self.graph_buffers = ResourceStorage::new();
        self.swap_data.destroy(&self.context.device);
        self.frame_context.destroy();
        self.context.destroy_surface();
        info!("Clean shutdown!");
    }

    pub fn wait_for_device(&self) {
        if let Err(e) = unsafe { self.context.device.device_wait_idle() } {
            error!("device_wait_idle failed: {e}");
        } else {
            self.context.graph_buffer_history.borrow_mut().clear();
        }
    }

    pub fn recreate_swapchain(
        &mut self,
        size: crate::Size2D,
    ) -> Result<(), crate::error::RendererError> {
        if size.width == 0 || size.height == 0 {
            return Err(RendererError::InvalidOperation(
                "Output dimensions must be nonzero".into(),
            ));
        }
        self.wait_for_device();
        self.frame_clear();
        for sets in &mut self.graphics_descriptor_sets {
            sets.clear();
        }
        for constants in &mut self.graphics_constants {
            constants.clear();
        }
        for views in &mut self.graphics_image_views {
            for view in views.drain(..) {
                unsafe {
                    self.context.device.destroy_image_view(view, None);
                }
            }
        }
        self.graph_buffer_consumers
            .values_mut()
            .for_each(|owner| *owner = None);
        self.last_submission = None;
        self.surface_recreation_required = false;
        self.last_presented_image_index = None;
        self.pending_texture_exports.clear();
        self.committed_texture_exports
            .retain(|_, export| export.owns_image());
        self.context.wait_and_drain_all_staged_uploads();

        let old_extent = self.frame_context.extent;
        info!("=== Recreating swapchain ===");
        info!("  Old extent: {}x{}", old_extent.width, old_extent.height);

        self.frame_context.recreate_swapchain(vk::Extent2D {
            width: size.width,
            height: size.height,
        })?;

        let swap_data = SwapData::new(
            &self.context.device,
            &self
                .frame_context
                .swapchain_images
                .iter()
                .map(|image| image.vk())
                .collect::<Vec<_>>(),
            FRAMES_IN_FLIGHT,
        )?;
        self.swap_data.destroy(&self.context.device);
        self.swap_data = swap_data;
        // The new SwapData restarts its frame counter; the device idle wait
        // above completed every old submission, so retirements can free now.
        self.drain_retirements_all();

        let new_extent = self.frame_context.extent;
        info!("  New extent: {}x{}", new_extent.width, new_extent.height);

        Ok(())
    }

    pub fn num_images(&self) -> usize {
        self.frame_context.swapchain_image_views.len()
    }

    /// Create a mesh from vertex and index data.
    ///
    /// Returns a handle that can be used in DrawCall objects.
    /// The actual GPU buffers are managed internally by the AssetRegistry.
    ///
    /// # Arguments
    /// * `vertices` - Slice of vertex data (must match the vertex binding of the material)
    /// * `indices` - Index data for indexed drawing
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_mesh<T, U>(
        &mut self,
        vertices: &[T],
        indices: &[U],
        topology: crate::renderer::registry::PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: crate::vertex::Vertex,
        U: crate::renderer::registry::MeshIndexElement,
    {
        self.mesh_manager
            .create_mesh(&mut self.asset_registry, vertices, indices, topology)
    }

    /// Report the index format recorded for a mesh, for diagnostics and tests.
    pub fn mesh_index_format(
        &self,
        mesh: MeshHandle,
    ) -> Option<crate::backend::command::IndexType> {
        self.asset_registry
            .get_mesh(mesh)
            .map(|asset| asset.index_format)
    }

    /// Create a mesh with separate per-attribute vertex buffers (SOA layout).
    ///
    /// Each attribute type (Position, Normal, Tangent, etc.) gets its own GPU buffer,
    /// enabling efficient depth-only and shadow passes that only need a subset of attributes.
    ///
    /// # Arguments
    /// * `attributes` - Map of attribute type to raw byte data
    /// * `vertex_count` - Total number of vertices
    /// * `indices` - Index data (u32)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_mesh_soa(
        &mut self,
        attributes: &std::collections::HashMap<AttributeType, Vec<u8>>,
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        self.mesh_manager.create_mesh_soa(
            &mut self.asset_registry,
            attributes,
            vertex_count,
            indices,
        )
    }

    /// Create a cube mesh with the given size.
    ///
    /// # Arguments
    /// * `size` - The size of the cube as [width, height, depth]
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_cube_mesh(&mut self, size: [f32; 3]) -> Result<MeshHandle, RendererError> {
        self.mesh_manager
            .create_cube(&mut self.asset_registry, size)
    }

    /// Create a UV sphere mesh.
    ///
    /// # Arguments
    /// * `radius` - The radius of the sphere
    /// * `segments` - Number of horizontal segments (longitude)
    /// * `rings` - Number of vertical rings (latitude)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_sphere_mesh(
        &mut self,
        radius: f32,
        segments: u32,
        rings: u32,
    ) -> Result<MeshHandle, RendererError> {
        self.mesh_manager
            .create_sphere(&mut self.asset_registry, radius, segments, rings)
    }

    /// Create a plane mesh on the XZ plane.
    ///
    /// # Arguments
    /// * `width` - The width of the plane (X axis)
    /// * `height` - The height of the plane (Z axis)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_plane_mesh(
        &mut self,
        width: f32,
        height: f32,
    ) -> Result<MeshHandle, RendererError> {
        self.mesh_manager
            .create_plane(&mut self.asset_registry, width, height)
    }

    /// Create a cone mesh with base at y=0 and apex at y=height.
    ///
    /// # Arguments
    /// * `height` - The height of the cone (Y axis)
    /// * `base_radius` - The radius of the base circle
    /// * `segments` - Number of segments around the circumference
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_cone_mesh(
        &mut self,
        height: f32,
        base_radius: f32,
        segments: u32,
    ) -> Result<MeshHandle, RendererError> {
        self.mesh_manager
            .create_cone(&mut self.asset_registry, height, base_radius, segments)
    }

    /// Create a cylinder mesh standing on Y axis.
    ///
    /// # Arguments
    /// * `height` - The height of the cylinder (Y axis)
    /// * `radius` - The radius of the cylinder
    /// * `segments` - Number of segments around the circumference
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_cylinder_mesh(
        &mut self,
        height: f32,
        radius: f32,
        segments: u32,
    ) -> Result<MeshHandle, RendererError> {
        self.mesh_manager
            .create_cylinder(&mut self.asset_registry, height, radius, segments)
    }

    /// Create a torus (donut) mesh on the XZ plane.
    ///
    /// # Arguments
    /// * `major_radius` - Distance from center of torus to center of tube
    /// * `minor_radius` - Radius of the tube
    /// * `segments` - Number of segments around the major circumference
    /// * `rings` - Number of segments around the minor circumference (tube)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_torus_mesh(
        &mut self,
        major_radius: f32,
        minor_radius: f32,
        segments: u32,
        rings: u32,
    ) -> Result<MeshHandle, RendererError> {
        self.mesh_manager.create_torus(
            &mut self.asset_registry,
            major_radius,
            minor_radius,
            segments,
            rings,
        )
    }

    /// Create a plane on the XY axis (vertical, facing +Z).
    ///
    /// # Arguments
    /// * `width` - The width of the plane (X axis)
    /// * `height` - The height of the plane (Y axis)
    /// * `segments` - Number of subdivisions in both directions
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_plane_xy_mesh(
        &mut self,
        width: f32,
        height: f32,
        segments: u32,
    ) -> Result<MeshHandle, RendererError> {
        self.mesh_manager
            .create_plane_xy(&mut self.asset_registry, width, height, segments)
    }

    /// Create a dynamic mesh from raw vertex and index data.
    ///
    /// This method creates a mesh that can be updated every frame.
    /// The buffers are created with CPU-accessible memory for fast updates.
    ///
    /// # Arguments
    /// * `vertex_data` - Raw vertex data in bytes
    /// * `vertex_count` - Number of vertices
    /// * `indices` - Index data (u32)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_mesh_dynamic(
        &mut self,
        descriptor: &crate::renderer::registry::MeshDescriptor,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        self.mesh_manager.create_mesh_dynamic(
            &mut self.asset_registry,
            descriptor,
            vertex_data,
            indices,
        )
    }

    /// Update a dynamic mesh with new vertex and index data.
    ///
    /// The mesh must have been created with `create_mesh_dynamic`; static
    /// meshes are immutable and updating them fails with a typed error.
    ///
    /// Contract (identical on every backend):
    /// - `vertex_data` is one interleaved blob describing exactly
    ///   `vertex_count` vertices of the mesh's recorded layout stride;
    ///   every index must reference a vertex in range. Violations fail with
    ///   typed errors before any GPU state changes.
    /// - Success publishes one internally consistent mesh: logical vertex
    ///   and index counts, buffer contents, and buffer capacities all
    ///   describe the new mesh. Shrinking updates only shrink the logical
    ///   counts; buffers keep their capacity for later growth.
    /// - Growth reallocates the buffers that no longer fit, retiring the old
    ///   native buffers until the submissions that can still read them have
    ///   completed. Allocation failure returns a typed error and leaves the
    ///   previous mesh state fully intact.
    /// - `vertex_count == 0` with empty `vertex_data` and empty `indices`
    ///   transitions the mesh to an empty state that draws nothing; a later
    ///   update can repopulate it.
    /// - The recorded index width (`u32`) never changes.
    ///
    /// # Arguments
    /// * `mesh` - Handle to the mesh to update
    /// * `vertex_data` - New interleaved vertex data in bytes
    /// * `vertex_count` - Number of vertices in `vertex_data`
    /// * `indices` - New index data (u32)
    pub fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        let mut retirements =
            FrameRetirements::new(&mut self.retirements, self.swap_data.frame_counter());
        self.mesh_manager.update_mesh_dynamic(
            &mut self.asset_registry,
            &mut retirements,
            mesh,
            vertex_data,
            vertex_count,
            indices,
        )
    }

    /// Queue a destroyed or replaced native resource for deferred free.
    ///
    /// The logical handle (mesh, material, texture, skeleton) is already
    /// invalidated by the time this runs; this only keeps the native object
    /// alive until the submissions that could still reference it have
    /// completed.
    pub(crate) fn retire(&mut self, resource: impl Into<RetiredResource>) {
        let mut retirements =
            FrameRetirements::new(&mut self.retirements, self.swap_data.frame_counter());
        retirements.retire(resource);
    }

    /// Free every pending retirement and release its bindless slots.
    ///
    /// Only valid after a device-wide idle wait (`destroy`, swapchain
    /// recreation): all submissions have completed, so nothing can still
    /// reference the retired resources.
    fn drain_retirements_all(&mut self) {
        for slot in self.retirements.drain_all() {
            self.bindless_manager.release_texture_slot(slot);
        }
    }

    /// Report the logical vertex count recorded for a mesh.
    ///
    /// For dynamic meshes this tracks the latest successful update and may
    /// be smaller than the underlying buffer capacity. Returns `None` when
    /// the handle does not reference a live mesh.
    pub fn mesh_vertex_count(&self, mesh: MeshHandle) -> Option<u32> {
        self.asset_registry.get_mesh(mesh).map(|m| m.vertex_count)
    }

    /// Report the logical index count recorded for a mesh.
    ///
    /// Draw encoding reads exactly this many indices. Returns `None` when
    /// the handle does not reference a live mesh.
    pub fn mesh_index_count(&self, mesh: MeshHandle) -> Option<u32> {
        self.asset_registry.get_mesh(mesh).map(|m| m.index_count)
    }

    /// Report pending deferred retirements per resource kind, plus the
    /// frame counter of the oldest entry (diagnostics and tests). Destroyed
    /// and replaced native resources stay pending until the submissions
    /// that can still reference them have completed.
    pub fn pending_retirements(&self) -> RetirementSnapshot {
        self.retirements.snapshot()
    }

    /// Report the number of staged mesh uploads submitted but not yet
    /// observed complete (diagnostics and tests). They release at frame
    /// boundaries.
    pub fn pending_staged_uploads(&self) -> usize {
        self.context.pending_staged_uploads()
    }

    /// Report the memory placement of a mesh's GPU buffers.
    ///
    /// Static meshes are staged into device-local memory where supported
    /// (with a host-visible fallback reported here); dynamic meshes stay
    /// host-visible. Returns `None` when the handle does not reference a
    /// live mesh.
    pub fn mesh_memory_report(
        &self,
        mesh: MeshHandle,
    ) -> Option<crate::renderer::registry::MeshMemoryReport> {
        use gpu_allocator::MemoryLocation;

        let class = |location: MemoryLocation| match location {
            MemoryLocation::GpuOnly => crate::renderer::registry::MeshMemoryClass::DeviceLocal,
            _ => crate::renderer::registry::MeshMemoryClass::HostVisible,
        };
        let mesh_asset = self.asset_registry.get_mesh(mesh)?;
        let mut report = crate::renderer::registry::MeshMemoryReport {
            device_local_buffers: 0,
            host_visible_buffers: 0,
            index_buffer: mesh_asset
                .index_buffer
                .as_ref()
                .map(|ib| class(ib.memory_location())),
        };
        for vb in mesh_asset.attribute_buffers.values() {
            match class(vb.memory_location()) {
                crate::renderer::registry::MeshMemoryClass::DeviceLocal => {
                    report.device_local_buffers += 1;
                }
                crate::renderer::registry::MeshMemoryClass::HostVisible => {
                    report.host_visible_buffers += 1;
                }
            }
        }
        Some(report)
    }

    // ========================================================================
    // Render Graph System
    // ========================================================================

    /// Return the current swapchain frame index.
    ///
    /// For mutable frame resources, use the acquired [`FrameToken`]'s slot.
    pub fn current_frame(&self) -> usize {
        self.swap_data.current_frame()
    }

    /// Create an empty graph builder.
    pub fn create_frame_graph(&self) -> crate::render_graph::FrameGraphBuilder {
        crate::render_graph::FrameGraphBuilder::new()
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        if !self.destroyed {
            log::warn!("VulkanRenderer dropped without explicit destroy() call");
            self.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_constructor_allocates_only_device_core_resources() {
        let mut renderer = VulkanRenderer::init_headless(
            32,
            32,
            ValidationMode::Enabled,
            c"constructor-proof".into(),
            c"Katla".into(),
        )
        .unwrap();
        assert_eq!(renderer.asset_registry.material_count(), 0);
        assert_eq!(renderer.asset_registry.material_variant_count(), 0);
        assert_eq!(renderer.asset_registry.mesh_count(), 0);
        assert_eq!(renderer.texture_manager.len(), 1);
        assert_eq!(
            renderer
                .texture_manager
                .get_bindless_slot(renderer.texture_manager.default_texture()),
            Some(0)
        );
        assert_eq!(renderer.graph_buffers.len(), 0);
        assert_eq!(renderer.skeleton_buffers.len(), 0);
        assert!(renderer.graph_compute_pipelines.is_empty());
        assert!(renderer.graphics_samplers.is_empty());
        assert!(renderer.graphics_constants.iter().all(Vec::is_empty));
        assert!(renderer.graphics_descriptor_sets.iter().all(Vec::is_empty));
        assert!(!renderer.ui_renderer.is_installed());
        renderer.destroy();
    }
}
