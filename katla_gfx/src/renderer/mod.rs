//! Vulkan renderer implementation modules.
//!
//! This module organizes VulkanRenderer methods into logical groups:
//!
//! - `frame` - Frame rendering and swapchain management
//! - `viewport` - Viewport system management (TODO: extract from lib.rs)
//! - `ui` - UI buffer and texture management (TODO: extract from lib.rs)

pub mod animation_init;
pub mod bindless_queries;
pub mod compositing;
pub mod depth_prepass;
pub mod destroy_api;
pub mod font_atlas;
pub mod frame_lifecycle;
pub mod fullscreen_shader;
pub mod light_culling;
pub mod material_api;
pub mod mesh_manager;
pub mod outline;
pub mod particle_init;
pub mod picking;
pub mod readback;
pub mod registry;
pub mod shadow;
pub mod skeleton_api;
pub mod texture_api;
pub mod types;
pub mod ui_renderer;
pub mod viewport_manager;

pub use crate::handle::{
    Handle, MaterialHandle, MeshHandle, PipelineHandle, SkeletonHandle, TextureHandle,
};
use crate::viewport::{Viewport, ViewportBuilder, ViewportHandle};
pub use crate::vulkan::context::ValidationMode;
pub use registry::AssetRegistry;
pub use types::{DrawCall, DrawList, FrameUniforms, InstanceData, UIDrawList, UiDrawCommand};

use crate::error::RendererError;
use crate::handle::ResourceStorage;
use crate::texture::{TextureDescriptor, TextureManager};
use crate::vulkan::bindless_texture::{BindlessTextureManager, MAX_BINDLESS_TEXTURES};
use crate::vulkan::context::VulkanContext;
use crate::vulkan::context::VulkanFrameCtx;
use crate::vulkan::material::SkeletonDescriptorSet;
use crate::vulkan::material::storage_uniform::{StorageDescriptorSet, StorageUniformManager};
use crate::vulkan::skeleton_buffer::SkeletonBuffer;
use crate::vulkan::swapdata::SwapData;
use crate::vulkan::vertexbuffer::{IndexBuffer, VertexBuffer};
use ash::vk;
use log::{error, info};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{ffi::CString, rc::Rc};

use crate::barrier::ImageBarrier;
use crate::sync::COLOR_SUBRESOURCE_RANGE;
use crate::vulkan::IndexType;
use crate::vulkan::material::compiler::MaterialCompiler;
use crate::vulkan::vertex_attribute::AttributeType;

/// Per-frame UI rendering resources.
pub(crate) struct UiFrameResources {
    /// Per-frame UI vertex buffers.
    pub vertex_buffers: Vec<VertexBuffer>,
    /// Per-frame UI index buffers.
    pub index_buffers: Vec<IndexBuffer>,
    /// Per-frame UI descriptor sets (owns both set and pool, automatic cleanup).
    pub descriptor_sets: Vec<Option<crate::vulkan::descriptor_set::DescriptorSet>>,
    /// UI uniform buffer (reused across frames).
    pub uniform_buffer: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,
}

impl UiFrameResources {
    /// Create new UI frame resources with pre-allocated buffers.
    fn new(context: &Rc<VulkanContext>) -> Self {
        let mut vertex_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut index_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut descriptor_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);

        for _ in 0..FRAMES_IN_FLIGHT {
            vertex_buffers.push(VertexBuffer::new(
                context.clone(),
                1024 * 1024, // 1MB initial size
                65536,       // Vertex count
            ));
            index_buffers.push(IndexBuffer::new(
                context.clone(),
                1024 * 1024,       // 1MB initial size
                IndexType::Uint32, // 32-bit indices
                65536,             // Index count
            ));
            descriptor_sets.push(None);
        }

        Self {
            vertex_buffers,
            index_buffers,
            descriptor_sets,
            uniform_buffer: None,
        }
    }
}

/// Transpose a 4x4 matrix from row-major to column-major format.
///
/// Pending readback operation for async frame checking
pub struct PendingReadback {
    frame: usize,
    fence: vk::Fence,
    command_buffer: crate::vulkan::commandbuffer::CommandBuffer,
    staging_buffer: vk::Buffer,
    staging_allocation: gpu_allocator::vulkan::Allocation,
    buffer_size: vk::DeviceSize,
}

pub struct VulkanRenderer {
    pub(crate) context: Rc<VulkanContext>,
    pub(crate) frame_context: VulkanFrameCtx,
    pub(crate) swap_data: SwapData,
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
    /// Per-frame storage descriptor sets for binding frame and object uniforms.
    /// Each set contains the storage buffer bound at two offsets (frame_data at 0, objects at 256).
    pub(crate) storage_descriptor_sets: Vec<StorageDescriptorSet>,
    /// Skeleton descriptor sets for GPU skeletal animation.
    /// Indexed by SkeletonHandle via ResourceStorage.
    pub(crate) skeleton_descriptors: ResourceStorage<SkeletonDescriptorSet>,
    /// Skeleton buffers for GPU skeletal animation.
    /// Indexed by SkeletonHandle via ResourceStorage.
    pub(crate) skeleton_buffers: ResourceStorage<SkeletonBuffer>,
    /// Compositing descriptor set layout for multi-viewport compositing.
    /// Created during initialization and used when compiling compositing materials.
    pub(crate) compositing_descriptor_set_layout: vk::DescriptorSetLayout,
    /// Frame-level uniforms set once per frame via set_frame_uniforms().
    pub(crate) frame_uniforms: FrameUniforms,
    /// Last presented swapchain image index (for debugging readback).
    pub(super) last_presented_image_index: Option<u32>,
    /// Cached default white PBR material handle.
    pub(super) default_material_handle: Option<MaterialHandle>,
    /// Pending async readback operation
    pub(super) pending_readback: Option<PendingReadback>,
    /// Output render target for final composition (UI renders here, then present_pass copies to swapchain).
    output_target: Option<OutputRenderTarget>,
    /// Viewport manager for viewport and render target management.
    pub(crate) viewport_manager: viewport_manager::ViewportManager,
    /// Material compiler for compiling materials from shaders.
    pub(crate) material_compiler: MaterialCompiler,
    /// UI rendering subsystem - owns UI resources and font atlas.
    pub ui_renderer: ui_renderer::UIRenderer,
    /// Global particle system for GPU-driven particle effects.
    pub particle_system: Option<crate::particles::GlobalParticleSystem>,
    /// GPU animation pose evaluation pipeline.
    pub animation_pipeline: Option<crate::animation::PoseComputePipeline>,
    /// GPU animation buffers for pose evaluation.
    pub animation_buffers: Option<crate::animation::PoseComputeBuffers>,
    /// Light culling state (Forward+ dynamic lighting).
    light_culling: light_culling::LightCullingState,
    /// Shadow system state (CSM cascaded shadow maps).
    pub(crate) shadow: shadow::ShadowState,
    /// Shared empty descriptor set layout (no bindings).
    /// Used as a placeholder for Set 1 in skinned pipelines (outline, depth prepass, shadow).
    pub(crate) shared_empty_descriptor_layout: vk::DescriptorSetLayout,
    /// Depth prepass state (depth-only pre-pass).
    depth_prepass: depth_prepass::DepthPrepassState,
    /// Outline highlight state (stencil-based selection highlight).
    pub(crate) outline: outline::OutlineState,
    /// Pending picking readback operation.
    pending_picking_readback: Option<picking::PickingReadback>,
    /// Base bindless index for per-frame depth textures.
    /// Actual index for frame N is `depth_texture_base_index + N`.
    depth_texture_base_index: Option<u32>,
    /// Tracks whether the first frame has been rendered.
    /// Used to skip the inter-frame semaphore wait on the very first frame.
    first_frame_rendered: bool,
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

impl VulkanRenderer {
    pub fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
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
        ));

        // Set up validation logging at appropriate log levels
        if validation_mode.is_enabled() {
            context.setup_validation_logging();
        }

        let frame_context = VulkanFrameCtx::init(&context);

        let swapchain_images_raw: Vec<vk::Image> = frame_context
            .swapchain_images
            .iter()
            .map(|img| img.vk())
            .collect();
        let swap_data = SwapData::new(&context.device, &swapchain_images_raw, FRAMES_IN_FLIGHT);

        // Initialize bindless texture manager
        let bindless_manager = BindlessTextureManager::new(context.clone()).map_err(|e| {
            error!("Failed to create bindless texture manager: {:?}", e);
            RendererError::InitializationFailed(
                "Failed to create bindless texture manager".to_string(),
            )
        })?;
        info!(
            "Bindless texture system initialized (max {} textures)",
            MAX_BINDLESS_TEXTURES
        );

        // Initialize texture manager
        let texture_manager = TextureManager::new(context.clone()).map_err(|e| {
            error!("Failed to create texture manager: {:?}", e);
            RendererError::InitializationFailed("Failed to create texture manager".to_string())
        })?;
        info!("Texture manager initialized");

        // Initialize storage uniform system with standard layout
        let storage_manager = StorageUniformManager::new(context.clone(), FRAMES_IN_FLIGHT)?;

        // Create per-frame storage descriptor sets for binding frame and object uniforms
        let mut storage_descriptor_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame_idx in 0..FRAMES_IN_FLIGHT {
            let descriptor_set = StorageDescriptorSet::new(
                &context,
                storage_manager.buffer(frame_idx),
                storage_manager.buffer_size(),
            )
            .map_err(|e| {
                error!("Failed to create storage descriptor set: {:?}", e);
                RendererError::InitializationFailed(format!(
                    "Failed to create storage descriptor set for frame {}: {:?}",
                    frame_idx, e
                ))
            })?;
            storage_descriptor_sets.push(descriptor_set);
        }

        // Initialize mesh manager
        let mesh_manager = mesh_manager::MeshManager::new(context.clone());

        // Initialize viewport manager
        let viewport_manager = viewport_manager::ViewportManager::new();

        // Initialize material compiler (use first descriptor set for compilation)
        let material_compiler = MaterialCompiler::new(
            context.clone(),
            &bindless_manager,
            &storage_descriptor_sets[0],
        )
        .map_err(|e| {
            error!("Failed to create material compiler: {:?}", e);
            RendererError::InitializationFailed("Failed to create material compiler".to_string())
        })?;
        info!("Material compiler initialized");

        // Create compositing descriptor set layout for multi-viewport rendering
        let compositing_descriptor_set_layout = {
            use crate::render_graph::descriptor_sets::CompositingDescriptorSet;
            CompositingDescriptorSet::create_layout(&context.device).map_err(|e| {
                error!(
                    "Failed to create compositing descriptor set layout: {:?}",
                    e
                );
                RendererError::InitializationFailed(
                    "Failed to create compositing descriptor set layout".to_string(),
                )
            })?
        };
        info!("Compositing descriptor set layout created");

        // Create shared empty descriptor set layout (no bindings).
        // Used as a placeholder for Set 1 in skinned pipelines (outline, depth prepass, shadow).
        let shared_empty_descriptor_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default(), None)
                .map_err(|_| vk::Result::ERROR_INITIALIZATION_FAILED)?
        };

        Ok(Self {
            context: context.clone(),
            frame_context,
            swap_data,
            mesh_manager,
            asset_registry: AssetRegistry::new(),
            bindless_manager,
            texture_manager,
            storage_manager,
            storage_descriptor_sets,
            skeleton_descriptors: ResourceStorage::new(),
            skeleton_buffers: ResourceStorage::new(),
            compositing_descriptor_set_layout,
            frame_uniforms: FrameUniforms::default(),
            last_presented_image_index: None,
            default_material_handle: None,
            pending_readback: None,
            output_target: None,
            viewport_manager,
            material_compiler,
            ui_renderer: ui_renderer::UIRenderer::new(&context),
            particle_system: None,
            animation_pipeline: None,
            animation_buffers: None,
            light_culling: light_culling::LightCullingState::default(),
            shared_empty_descriptor_layout,
            shadow: shadow::ShadowState::default(),
            depth_prepass: depth_prepass::DepthPrepassState::default(),
            outline: outline::OutlineState::default(),
            pending_picking_readback: None,
            depth_texture_base_index: None,
            first_frame_rendered: false,
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

    /// Register per-frame depth textures with the bindless system.
    ///
    /// Must be called after frame context is created. Returns the base bindless slot;
    /// frame N's depth texture is at `base + N`.
    pub fn register_depth_textures_bindless(&mut self) -> Result<u32, RendererError> {
        let mut base_slot: Option<u32> = None;
        for (frame_idx, depth_texture) in
            self.frame_context.depth_render_textures.iter().enumerate()
        {
            let slot = self
                .bindless_manager
                .register_texture(depth_texture.image_view.vk())
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to register depth texture frame {}: {}",
                        frame_idx, e
                    ))
                })?;
            if frame_idx == 0 {
                base_slot = Some(slot);
            }
            log::debug!(
                "Registered depth texture frame {} at bindless slot {}",
                frame_idx,
                slot
            );
        }
        let base = base_slot.ok_or_else(|| {
            RendererError::InitializationFailed("No depth textures to register".to_string())
        })?;
        self.depth_texture_base_index = Some(base);
        Ok(base)
    }

    /// Get the base bindless index for per-frame depth textures.
    /// Actual index for frame N is `base + N`.
    pub fn depth_texture_base_index(&self) -> Option<u32> {
        self.depth_texture_base_index
    }

    /// Initialize or resize the output render target.
    ///
    /// This creates a texture that the UI renders to, which is then
    /// copied to the swapchain by the present pass.
    pub fn init_output_target(&mut self, width: u32, height: u32) -> Result<(), vk::Result> {
        let needs_resize = self
            .output_target
            .as_ref()
            .map(|t| t.extent.width != width || t.extent.height != height)
            .unwrap_or(true);

        if needs_resize {
            // Old target is dropped automatically with Drop
            self.output_target = None;
            let target = OutputRenderTarget::new(self.context.clone(), width, height)?;
            self.output_target = Some(target);
            info!(
                "Output render target created/resized to {}x{}",
                width, height
            );
        }
        Ok(())
    }

    /// Get the swapchain extent (primary window size).
    pub fn swapchain_extent(&self) -> crate::Size2D {
        let ext = self.frame_context.swapchain.get_extent();
        crate::Size2D::new(ext.width, ext.height)
    }

    /// Get output dimensions.
    pub fn output_extent(&self) -> Option<crate::Size2D> {
        self.output_target
            .as_ref()
            .map(|t| crate::Size2D::from(t.extent))
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

    // ========================================================================
    // Viewport System
    // ========================================================================

    /// Create a viewport builder for configuring a new viewport.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let viewport = renderer.create_viewport()
    ///     .size(512, 512)
    ///     .with_depth(DepthFormat::D32SfloatS8Uint)
    ///     .output_mode(OutputMode::Offscreen)
    ///     .label("preview")
    ///     .build(&mut renderer)?;
    /// ```
    pub fn create_viewport(&mut self) -> ViewportBuilder {
        self.viewport_manager.create()
    }

    /// Get the number of viewports.
    pub fn viewport_count(&self) -> usize {
        self.viewport_manager.count()
    }

    /// Get viewport by handle.
    pub fn get_viewport(&self, handle: ViewportHandle) -> Option<&Viewport> {
        self.viewport_manager.get(handle)
    }

    /// Get mutable viewport by handle.
    pub fn get_viewport_mut(&mut self, handle: ViewportHandle) -> Option<&mut Viewport> {
        self.viewport_manager.get_mut(handle)
    }

    /// Get the texture ID for a viewport (for UI sampling).
    ///
    /// Note: This is a legacy method for UI compatibility. The actual texture
    /// is managed by the frame graph system, not the viewport itself.
    /// Returns a u64 that can be used with katla_ui::TextureId::custom(id).
    pub fn viewport_texture_id(&self, handle: ViewportHandle) -> Option<u64> {
        self.viewport_manager.texture_id(handle)
    }

    /// Get the viewport extent by handle.
    pub fn viewport_extent(&self, handle: ViewportHandle) -> Option<crate::Size2D> {
        self.viewport_manager.extent(handle)
    }

    /// Destroy a viewport by handle.
    pub fn destroy_viewport(&mut self, handle: ViewportHandle) {
        if self.viewport_manager.destroy(handle) {
            info!("Viewport {} destroyed", handle.0);
        }
    }

    pub fn destroy(&mut self) {
        // Note: Pending readback should have been cleaned up by wait_for_pending_readback()
        // in cleanup_on_exit(). This is a safety check in case destroy() is called directly.
        if let Some(readback) = self.pending_readback.take() {
            log::warn!(
                "Pending readback found during destroy() - cleanup should have happened earlier"
            );
            unsafe {
                let _ = self
                    .context
                    .device
                    .wait_for_fences(&[readback.fence], true, u64::MAX);
                readback.command_buffer.return_to_pool();
                self.context.device.destroy_fence(readback.fence, None);
                self.context
                    .free_buffer(readback.staging_buffer, readback.staging_allocation);
            }
        }

        // Wait for device idle to ensure all GPU operations have completed
        self.wait_for_device();

        // Destroy output render target (Drop handles cleanup)
        self.output_target = None;

        // Destroy all viewports
        self.viewport_manager.clear();

        // Destroy particle system FIRST (before destroying other resources)
        // This ensures proper cleanup order and avoids heap corruption
        if let Some(mut particle_system) = self.particle_system.take() {
            info!("Destroying particle system");
            particle_system.destroy();
        }

        // Destroy animation pipeline and buffers
        if let Some(mut pipeline) = self.animation_pipeline.take() {
            info!("Destroying animation pose compute pipeline");
            pipeline.destroy();
        }
        self.animation_buffers = None; // Drop handles cleanup via Drop impl

        // Destroy all registered assets first (materials, meshes)
        self.asset_registry.destroy();

        // Destroy material compiler (cleans up descriptor layouts)
        self.material_compiler.destroy();

        // Destroy compositing descriptor set layout
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(self.compositing_descriptor_set_layout, None);
        }

        // Clean up UI resources
        {
            let ui_resources = self.ui_renderer.ui_resources_mut();
            // Vertex and index buffers have Drop impls that clean up themselves
            ui_resources.vertex_buffers.clear();
            ui_resources.index_buffers.clear();

            // Descriptor sets own their pools and clean up automatically via Drop
            ui_resources.descriptor_sets.clear();

            // Destroy uniform buffer
            if let Some((buffer, allocation)) = ui_resources.uniform_buffer.take() {
                self.context.free_buffer(buffer, allocation);
            }
        }

        // Drop light culling pipeline and buffers.
        // Drop order: pipeline first (doesn't own descriptor layouts),
        // then buffers (owns descriptor layouts and GPU buffers).
        self.light_culling.pipeline = None;
        self.light_culling.buffers = None;

        // Destroy shadow system resources (buffers, samplers, pools)
        self.shadow.csm = None;
        self.shadow.buffers = None;
        if let Some(sampler) = self.shadow.sampler.take() {
            unsafe {
                self.context.device.destroy_sampler(sampler, None);
            }
        }
        // Cascade descriptor resources (Set 2 for shadow depth shader)
        if let Some(pool) = self.shadow.cascade_descriptor_pool.take() {
            unsafe {
                self.context.device.destroy_descriptor_pool(pool, None);
            }
        }
        self.shadow.cascade_descriptor_sets.clear();
        for (buffer, allocation) in self
            .shadow
            .cascade_buffers
            .drain(..)
            .zip(self.shadow.cascade_allocations.drain(..))
        {
            unsafe {
                self.context.device.destroy_buffer(buffer, None);
                let _ = self.context.allocator.borrow_mut().free(allocation);
            }
        }
        self.shadow.cascade_mapped_ptrs.clear();
        // Original shadow descriptor resources (pool only, layout destroyed after pre_destroy)
        if let Some(pool) = self.shadow.descriptor_pool.take() {
            unsafe {
                self.context.device.destroy_descriptor_pool(pool, None);
            }
        }
        self.shadow.descriptor_sets.clear();

        // Wait for GPU to finish all in-flight work before destroying resources
        // that pipelines still reference (descriptor set layouts, etc.)
        self.context.pre_destroy();

        // Destroy descriptor set layouts AFTER device_wait_idle, since pipelines
        // in asset_registry still reference them until they are dropped.
        if let Some(layout) = self.shadow.cascade_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.shadow.descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if self.shared_empty_descriptor_layout != vk::DescriptorSetLayout::null() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(self.shared_empty_descriptor_layout, None);
            }
        }

        self.swap_data.destroy(&self.context.device);
        self.frame_context.destroy();
        info!("Clean shutdown!");
    }

    pub fn wait_for_device(&self) {
        unsafe {
            self.context.device.device_wait_idle().unwrap();
        }
    }

    pub fn recreate_swapchain(
        &mut self,
        frame_graph: &mut crate::render_graph::FrameGraph,
    ) -> Vec<(String, u32)> {
        self.wait_for_device();
        self.first_frame_rendered = false;

        let old_extent = self.frame_context.swapchain.get_extent();
        info!("=== Recreating swapchain ===");
        info!("  Old extent: {}x{}", old_extent.width, old_extent.height);

        self.frame_context.recreate_swapchain();

        let new_extent = self.frame_context.swapchain.get_extent();
        info!("  New extent: {}x{}", new_extent.width, new_extent.height);

        // Re-register depth textures with bindless (depth images were recreated)
        if self.depth_texture_base_index.is_some() {
            for (frame_idx, depth_texture) in
                self.frame_context.depth_render_textures.iter().enumerate()
            {
                if let Some(base) = self.depth_texture_base_index {
                    let slot = base + frame_idx as u32;
                    if let Err(e) = self
                        .bindless_manager
                        .update_texture(slot, depth_texture.image_view.vk())
                    {
                        log::error!(
                            "Failed to update depth texture bindless slot {}: {}",
                            slot,
                            e
                        );
                    }
                }
            }
        }

        // Recreate light culling buffers for new dimensions
        self.resize_light_culling(new_extent.width, new_extent.height);

        // Recreate transient textures with new dimensions
        match frame_graph.recreate_transient_textures(self, new_extent.width, new_extent.height) {
            Ok(mut recreated_textures) => {
                // Update internal references (tonemap HDR texture)
                recreated_textures.retain(|(name, slot)| {
                    if name == "hdr_color" {
                        // Update tonemap pass with new HDR texture slot
                        if let Err(e) = frame_graph.set_tonemap_texture_index("tonemap", *slot) {
                            log::error!("Failed to update tonemap texture index: {}", e);
                        }
                    }
                    true // Keep all entries for app layer
                });
                info!(
                    "Recreated {} transient textures for resize",
                    recreated_textures.len()
                );
                recreated_textures
            }
            Err(e) => {
                log::error!("Failed to recreate transient textures: {}", e);
                Vec::new()
            }
        }
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
    pub fn create_mesh<T, U>(&mut self, vertices: &[T], indices: &[U]) -> MeshHandle
    where
        T: bytemuck::Pod,
        U: bytemuck::Pod,
    {
        self.mesh_manager
            .create_mesh(&mut self.asset_registry, vertices, indices)
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
    ) -> MeshHandle {
        self.mesh_manager.create_mesh_soa(
            &mut self.asset_registry,
            attributes,
            vertex_count,
            indices,
        )
    }

    /// Register a mesh with pre-existing buffers.
    ///
    /// This is useful when you've already created buffers and want to register them
    /// with the renderer for use in the draw list system.
    ///
    /// # Arguments
    /// * `vertex_buffer` - The vertex buffer (or None if no vertices)
    /// * `index_buffer` - The index buffer (or None if no indices)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn register_mesh(
        &mut self,
        vertex_buffer: Option<VertexBuffer>,
        index_buffer: Option<IndexBuffer>,
    ) -> MeshHandle {
        self.mesh_manager
            .register_mesh(&mut self.asset_registry, vertex_buffer, index_buffer)
    }

    /// Create a cube mesh with the given size.
    ///
    /// # Arguments
    /// * `size` - The size of the cube as [width, height, depth]
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_cube_mesh(&mut self, size: [f32; 3]) -> MeshHandle {
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
    pub fn create_sphere_mesh(&mut self, radius: f32, segments: u32, rings: u32) -> MeshHandle {
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
    pub fn create_plane_mesh(&mut self, width: f32, height: f32) -> MeshHandle {
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
    pub fn create_cone_mesh(&mut self, height: f32, base_radius: f32, segments: u32) -> MeshHandle {
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
    pub fn create_cylinder_mesh(&mut self, height: f32, radius: f32, segments: u32) -> MeshHandle {
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
    ) -> MeshHandle {
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
    pub fn create_plane_xy_mesh(&mut self, width: f32, height: f32, segments: u32) -> MeshHandle {
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
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> MeshHandle {
        self.mesh_manager.create_mesh_dynamic(
            &mut self.asset_registry,
            vertex_data,
            vertex_count,
            indices,
        )
    }

    /// Update a dynamic mesh with new vertex and index data.
    ///
    /// The mesh must have been created with `create_mesh_dynamic`.
    /// This method updates the existing buffers with new data.
    ///
    /// # Arguments
    /// * `mesh` - Handle to the mesh to update
    /// * `vertex_data` - New vertex data in bytes
    /// * `vertex_count` - Number of vertices
    /// * `indices` - New index data (u32)
    ///
    /// # Returns
    /// `Ok(())` on success, `Err` if mesh not found or buffers too small.
    pub fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        self.mesh_manager.update_mesh_dynamic(
            &mut self.asset_registry,
            mesh,
            vertex_data,
            vertex_count,
            indices,
        )
    }

    // ========================================================================
    // Render Graph System
    // ========================================================================

    /// Create a frame graph builder for configuring a render pipeline.
    ///
    /// Frame graphs are built once at startup and executed every frame.
    /// They define the structure of your rendering pipeline (passes,
    /// resources, dependencies) and handle automatic barrier generation.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let frame_graph = renderer.create_frame_graph()
    ///     .add_pass(GeometryPass::new("geometry")
    ///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
    ///         .write_depth("depth", ImageFormat::D32Sfloat))
    ///     .add_pass(FullscreenPass::new("tonemap")
    ///         .read("color")
    ///         .write_backbuffer())
    ///     .build()?;
    /// ```
    ///
    /// Returns the frame index from swap_data, which is the authoritative source.
    /// This ensures consistency across all frame-indexed resource access.
    pub fn current_frame(&self) -> usize {
        self.swap_data.current_frame()
    }

    /// Initialize the particle system.
    ///
    /// This must be called after renderer initialization but before frame graph creation.
    /// Sets up the global particle buffer and prepares for particle rendering.
    pub fn init_particle_system(&mut self) -> Result<(), RendererError> {
        use crate::particles::GlobalParticleSystem;

        info!("Initializing particle system...");

        let particle_system = GlobalParticleSystem::new(&self.context, 1_048_576).map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to create particle system: {}", e))
        })?;

        self.particle_system = Some(particle_system);

        info!("Particle system initialized successfully");
        Ok(())
    }

    pub fn create_frame_graph(&self) -> crate::render_graph::FrameGraphBuilder {
        crate::render_graph::FrameGraphBuilder::new()
    }

    /// Execute a frame graph with the given submission callback.
    ///
    /// This is the main rendering entry point when using frame graphs.
    /// The callback receives a [`Frame`] for submitting draw lists to passes.
    ///
    /// # Arguments
    /// * `frame_graph` - The compiled frame graph to execute
    /// * `f` - Callback for submitting work to passes
    ///
    /// # Example
    ///
    /// ```ignore
    /// renderer.render(&frame_graph, |frame| {
    ///     frame.submit("geometry", &opaque_draw_list);
    ///     frame.submit("geometry", &transparent_draw_list);
    ///     // Passes without draw lists (like tonemap) run automatically
    /// });
    /// ```
    pub fn render<F>(&mut self, frame_graph: &mut crate::render_graph::FrameGraph, f: F)
    where
        F: FnOnce(&mut crate::render_graph::Frame),
    {
        // NOTE: wait_for_fence() is NOT called here — it must be called before
        // set_frame_uniforms() and execute_draw_calls() to prevent CPU-GPU data races
        // on per-frame storage buffers. Call wait_for_frame() at the start of each frame.

        // 1. Get frame index (start_frame() was already called in set_frame_uniforms())
        let frame_idx = self.current_frame();

        // 2. Acquire next swapchain image
        let (image_index, _is_suboptimal) = unsafe {
            self.frame_context
                .swapchain
                .swapchain_loader
                .acquire_next_image(
                    self.frame_context.swapchain.swapchain,
                    u64::MAX,
                    self.swap_data.image_available_semaphore(),
                    vk::Fence::null(),
                )
                .expect("Failed to acquire swapchain image")
        };

        // Store image index for readback debugging
        self.last_presented_image_index = Some(image_index);

        // 3. Get command buffer for this frame
        let cmd = self.frame_context.command_buffers[frame_idx].vk_command_buffer();

        // 4. Begin command buffer
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.context
                .device
                .begin_command_buffer(cmd, &begin_info)
                .expect("Failed to begin command buffer");
        }

        // 5. Transition swapchain image to COLOR_ATTACHMENT_OPTIMAL for rendering
        // Use transition_from_undefined for swapchain images because:
        // - After acquire_next_image, the actual layout is platform-specific
        // - We use load_op=CLEAR so we don't care about preserving contents
        // - Works correctly after swapchain recreation (images start as UNDEFINED)
        let swapchain_image = self.frame_context.swapchain_images[image_index as usize].vk();
        ImageBarrier::transition_from_undefined(
            &cmd,
            &self.context.device,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );

        // 6. Execute frame graph (records commands into the command buffer)
        frame_graph
            .execute(self, image_index, f)
            .expect("Frame graph execution failed");

        // 7. Transition swapchain image from COLOR_ATTACHMENT_OPTIMAL to PRESENT_SRC_KHR
        let swapchain_image = self.frame_context.swapchain_images[image_index as usize].vk();
        ImageBarrier::transition(
            &cmd,
            &self.context.device,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );

        // 8. End command buffer
        unsafe {
            self.context
                .device
                .end_command_buffer(cmd)
                .expect("Failed to end command buffer");
        }

        // 8. Submit command buffer with synchronization
        let render_finished_semaphore = self.swap_data.render_finished_semaphore(image_index);
        let frame_complete_semaphore = self.swap_data.frame_complete_semaphore();
        let signal_semaphores = [render_finished_semaphore, frame_complete_semaphore];
        let swapchains = [self.frame_context.swapchain.swapchain];
        let image_indices = [image_index];

        // On the first frame there's no previous frame to wait on.
        // After that, wait on the previous frame's completion semaphore at ALL_COMMANDS
        // to cover TRANSFER/CLEAR from vkCmdUpdateBuffer and TRANSFER_READ from vkCmdCopyBuffer.
        if self.first_frame_rendered {
            let wait_semaphores = [
                self.swap_data.image_available_semaphore(),
                self.swap_data.previous_frame_complete_semaphore(),
            ];
            let wait_stage_masks = [
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::ALL_COMMANDS,
            ];
            self.context.gfx_queue.submit_with_stages(
                &[&self.frame_context.command_buffers[frame_idx]],
                &wait_semaphores,
                &signal_semaphores,
                self.swap_data.in_flight_fence(),
                &wait_stage_masks,
            );
        } else {
            self.first_frame_rendered = true;
            let wait_semaphores = [self.swap_data.image_available_semaphore()];
            self.context.gfx_queue.submit(
                &[&self.frame_context.command_buffers[frame_idx]],
                &wait_semaphores,
                &signal_semaphores,
                self.swap_data.in_flight_fence(),
            );
        }

        // 10. Present to swapchain
        let present_wait_semaphores = [render_finished_semaphore];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&present_wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        unsafe {
            self.frame_context
                .swapchain
                .swapchain_loader
                .queue_present(self.context.gfx_queue.vk_queue(), &present_info)
                .expect("Failed to present");
        }

        // 10. Advance to next frame
        self.swap_data.step_frame();

        // 11. Advance to next frame
    }
}

/// Output render target for final UI composition.
/// The UI renders to this texture, then present_pass blits it to the swapchain.
/// This decouples rendering from presentation for a cleaner architecture.
pub struct OutputRenderTarget {
    /// Color attachment image.
    pub(crate) color_image: vk::Image,
    color_memory: Option<gpu_allocator::vulkan::Allocation>,
    pub(crate) color_image_view: vk::ImageView,
    /// Render extent (matches swapchain size).
    pub extent: vk::Extent2D,
    /// Context for cleanup.
    context: Rc<VulkanContext>,
}

impl OutputRenderTarget {
    /// Create a new output render target with the given dimensions.
    pub fn new(context: Rc<VulkanContext>, width: u32, height: u32) -> Result<Self, vk::Result> {
        unsafe {
            let extent = vk::Extent2D { width, height };
            let extent3d = vk::Extent3D {
                width,
                height,
                depth: 1,
            };

            // Create color image (RGBA8, can be used as color attachment and transfer source/dest)
            let color_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::B8G8R8A8_SRGB) // Match swapchain format
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .samples(vk::SampleCountFlags::TYPE_1);

            let (color_image, color_memory) =
                context.create_image(color_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create color image view
            let color_view_create_info = vk::ImageViewCreateInfo::default()
                .image(color_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::B8G8R8A8_SRGB)
                .components(vk::ComponentMapping::default())
                .subresource_range(COLOR_SUBRESOURCE_RANGE);

            let color_image_view = context
                .device
                .create_image_view(&color_view_create_info, None)?;

            // Transition image to COLOR_ATTACHMENT_OPTIMAL (ready for UI rendering)
            let cmd_buffer = context.begin_single_time_commands();
            let cmd = cmd_buffer.vk_command_buffer();

            ImageBarrier::transition_from_undefined(
                &cmd,
                &context.device,
                color_image,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            );

            context.end_single_time_commands(cmd_buffer);

            Ok(Self {
                color_image,
                color_memory: Some(color_memory),
                color_image_view,
                extent,
                context,
            })
        }
    }
}

impl Drop for OutputRenderTarget {
    fn drop(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.color_image_view, None);
            self.context.device.destroy_image(self.color_image, None);
            if let Some(memory) = self.color_memory.take()
                && let Ok(mut allocator) = self.context.allocator.try_borrow_mut()
            {
                allocator.free(memory).ok();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::texture::{ImageFormat, TextureDescriptor};

    #[test]
    fn test_ui_font_atlas_format() {
        // Test that the font atlas texture descriptor uses SRGB format
        // Font atlas should use SRGB format for proper color reproduction
        let desc = TextureDescriptor::rgba8_srgb(512, 512);

        // Font atlas should use SRGB format
        assert_eq!(
            desc.format,
            ImageFormat::R8G8B8A8Srgb,
            "Font atlas must use SRGB format for correct color rendering"
        );
    }

    #[test]
    fn test_texture_descriptor_format_difference() {
        // Demonstrate the difference between SRGB and UNORM formats
        let srgb_desc = TextureDescriptor::rgba8_srgb(256, 256);
        let unorm_desc = TextureDescriptor::rgba8_unorm(256, 256);

        assert_eq!(srgb_desc.format, ImageFormat::R8G8B8A8Srgb);
        assert_eq!(unorm_desc.format, ImageFormat::R8G8B8A8Unorm);

        // Both have same dimensions
        assert_eq!(srgb_desc.width, unorm_desc.width);
        assert_eq!(srgb_desc.height, unorm_desc.height);
    }
}
