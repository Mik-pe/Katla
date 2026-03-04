//! Vulkan renderer implementation modules.
//!
//! This module organizes VulkanRenderer methods into logical groups:
//!
//! - `frame` - Frame rendering and swapchain management
//! - `viewport` - Viewport system management (TODO: extract from lib.rs)
//! - `ui` - UI buffer and texture management (TODO: extract from lib.rs)

pub mod registry;
pub mod types;

pub use crate::handle::{Handle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use crate::viewport::{ViewportBuilder, ViewportHandle};
pub use registry::AssetRegistry;
pub use types::{
    DrawCall, DrawList, FrameUniforms, InstanceData, ParticleDispatch, ParticleRender,
    UiDrawCommand, UiDrawList,
};

use crate::material::Material;
use crate::vulkan::context::VulkanContext;
use crate::{
    BindlessTextureManager, IndexBuffer, MAX_BINDLESS_TEXTURES, RendererError,
    SkeletonDescriptorSet, StorageUniformManager, SwapData, TextureManager, VertexBuffer,
    VulkanFrameCtx, viewport::Viewport,
};
use ash::vk;
use log::{error, info};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{cell::RefCell, ffi::CString, rc::Rc};

use crate::sync::{COLOR_SUBRESOURCE_RANGE, DEPTH_SUBRESOURCE_RANGE};

pub struct VulkanRenderer {
    pub(crate) context: Rc<VulkanContext>,
    pub(crate) frame_context: VulkanFrameCtx,
    pub(crate) swap_data: SwapData,
    /// Asset registry for managing GPU resources (meshes, materials).
    /// This stores the actual Vulkan buffers and pipelines, while the application
    /// only holds opaque handles (MeshHandle, MaterialHandle).
    pub asset_registry: AssetRegistry,
    /// Bindless texture manager for efficient texture binding.
    /// All textures are stored in a single array accessed by index.
    /// Texture indices are passed via ObjectUniforms.texture_indices.
    pub(crate) bindless_manager: BindlessTextureManager,
    /// Centralized texture manager for handle-based texture creation.
    /// Provides a clean API for creating and looking up textures by handle.
    pub(crate) texture_manager: TextureManager,
    /// Storage uniform manager for storage buffer-based uniforms.
    /// Materials use storage buffers with instance indexing.
    pub(crate) storage_manager: StorageUniformManager,
    /// Draw list cell for geometry pass (shared with render graph).
    pub(crate) draw_list_cell: Rc<RefCell<Option<DrawList>>>,
    /// Skeleton descriptor sets for GPU skeletal animation.
    /// Indexed by SkeletonHandle.
    pub(crate) skeleton_descriptors: Vec<Option<SkeletonDescriptorSet>>,
    /// Frame-level uniforms set once per frame via set_frame_uniforms().
    pub(crate) frame_uniforms: Option<FrameUniforms>,
    /// Cached default white PBR material handle.
    default_material_handle: Option<MaterialHandle>,
    /// Offscreen render targets as (texture_id, target) pairs.
    /// Simple Vec since we only have a few targets (viewport + preview).
    /// - TextureId 2 = viewport
    /// - TextureId 101 = preview
    render_targets: Vec<(u64, ViewportRenderTarget)>,
    /// Output render target for final composition (UI renders here, then present_pass copies to swapchain).
    output_target: Option<OutputRenderTarget>,
    /// Viewport system (new unified API).
    /// Application layer manages which handle is "main" vs "preview".
    viewports: Vec<Viewport>,
    /// Cached UI render state (lazy initialized).
    ui_state: Option<UiRenderState>,
    /// Pending UI data for next frame (set by render_ui, consumed by render_frame).
    pending_ui: Option<UiFrameData>,
}

/// Cached UI mesh and material handles.
struct UiRenderState {
    mesh: MeshHandle,
    material: MaterialHandle,
}

/// Pending UI frame data passed from render_ui() to render_frame().
struct UiFrameData {
    vertex_bytes: Vec<u8>,
    vertex_count: u32,
    indices: Vec<u32>,
    commands: Vec<UiDrawCommand>,
    screen_size: [f32; 2],
}

/// Number of frames that can be processed concurrently.
/// This is an implementation detail for double-buffering.
pub(crate) const FRAMES_IN_FLIGHT: usize = 2;

/// Default bindless texture slot indices.
///
/// These slots are reserved for default textures in the bindless texture array.
/// Use these when registering fallback textures or when a material lacks a texture.
#[derive(Debug, Clone, Copy)]
pub struct BindlessDefaults {
    /// Default albedo/diffuse texture slot (white texture).
    pub albedo: u32,
    /// Default normal map slot (flat normal pointing +Z).
    pub normal: u32,
    /// Default metallic/roughness slot (non-metal, medium roughness).
    pub metallic_roughness: u32,
    /// Default ambient occlusion slot (white = no occlusion).
    pub occlusion: u32,
    /// Default emission slot (black = no emission).
    pub emission: u32,
}

impl VulkanRenderer {
    pub fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Result<Self, RendererError> {
        let context = Rc::new(VulkanContext::init(
            display,
            window,
            with_validation_layers,
            app_name,
            engine_name,
        ));

        // Set up validation logging at appropriate log levels
        if with_validation_layers {
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
        let storage_manager = StorageUniformManager::new(context.clone())?;

        Ok(Self {
            context: context.clone(),
            frame_context,
            swap_data,
            asset_registry: AssetRegistry::new(),
            bindless_manager,
            texture_manager,
            storage_manager,
            draw_list_cell: Rc::new(RefCell::new(None)),
            skeleton_descriptors: Vec::new(),
            frame_uniforms: None,
            default_material_handle: None,
            render_targets: Vec::new(),
            output_target: None,
            viewports: Vec::new(),
            ui_state: None,
            pending_ui: None,
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

    /// Render a single frame to the swapchain.
    ///
    /// This method handles the complete frame rendering pipeline:
    /// 1. Wait for previous frame to complete
    /// 2. Acquire next swapchain image
    /// 3. Record command buffer with render passes
    /// 4. Submit command buffer to GPU
    /// 5. Present to swapchain
    pub fn render_frame(&mut self) -> Result<(), RendererError> {
        let frame_index = self.swap_data.current_frame();
        let extent = self.frame_context.swapchain.get_extent();
        let swapchain = self.frame_context.swapchain.swapchain;

        // Wait for previous frame to complete
        self.swap_data.wait_for_fence(&self.context.device);

        // Acquire next swapchain image
        let swapchain_loader = self.context.swapchain_loader.as_ref().ok_or_else(|| {
            RendererError::SwapchainError("Swapchain loader not initialized".into())
        })?;

        let (image_index, _suboptimal) = unsafe {
            swapchain_loader
                .acquire_next_image(
                    swapchain,
                    u64::MAX,
                    self.swap_data.image_available_semaphore(),
                    vk::Fence::null(),
                )
                .map_err(|e| {
                    RendererError::SwapchainError(format!(
                        "Failed to acquire swapchain image: {:?}",
                        e
                    ))
                })?
        };

        // Begin command buffer recording
        let command_buffer = &self.frame_context.command_buffers[frame_index];

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.context
                .device
                .begin_command_buffer(command_buffer.vk_command_buffer(), &begin_info)
                .map_err(|e| {
                    RendererError::VulkanError(format!("Failed to begin command buffer: {:?}", e))
                })?;
        }

        // Transition swapchain image from undefined to color attachment optimal
        let swapchain_images = self.frame_context.swapchain_images();
        let swapchain_image: vk::Image = swapchain_images[image_index as usize].into();

        let color_barrier = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

        unsafe {
            self.context.device.cmd_pipeline_barrier(
                command_buffer.vk_command_buffer(),
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[color_barrier],
            )
        }

        // Begin rendering with dynamic rendering (Vulkan 1.3)
        let swapchain_image_views = self.frame_context.swapchain_image_views();
        let swapchain_image_view: vk::ImageView =
            swapchain_image_views[image_index as usize].into();

        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(swapchain_image_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.05, 0.1, 1.0, 1.0],
                },
            });

        let render_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .layer_count(1)
            .color_attachments(std::slice::from_ref(&color_attachment));

        unsafe {
            self.context
                .device
                .cmd_begin_rendering(command_buffer.vk_command_buffer(), &render_info);
        }

        // End rendering
        unsafe {
            self.context
                .device
                .cmd_end_rendering(command_buffer.vk_command_buffer())
        }

        // Transition swapchain image to present layout
        let present_barrier = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::empty());

        unsafe {
            self.context.device.cmd_pipeline_barrier(
                command_buffer.vk_command_buffer(),
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[present_barrier],
            )
        }

        // End command buffer
        unsafe {
            self.context
                .device
                .end_command_buffer(command_buffer.vk_command_buffer())
                .map_err(|e| {
                    RendererError::VulkanError(format!("Failed to end command buffer: {:?}", e))
                })?;
        }

        // Submit command buffer to GPU
        self.submit_command_buffer(frame_index, image_index)?;

        // Present to swapchain
        self.present_swapchain(image_index)?;

        // Step to next frame
        self.swap_data.step_frame();

        Ok(())
    }

    /// Submit command buffer to GPU queue.
    fn submit_command_buffer(
        &mut self,
        frame_index: usize,
        image_index: u32,
    ) -> Result<(), RendererError> {
        let command_buffer = &self.frame_context.command_buffers[frame_index];

        let wait_semaphore = self.swap_data.image_available_semaphore();
        let signal_semaphore = self.swap_data.render_finished_semaphore(image_index);
        let in_flight_fence = self.swap_data.in_flight_fence();

        // Reset fence
        unsafe {
            self.context
                .device
                .reset_fences(std::slice::from_ref(&in_flight_fence))
                .map_err(|e| {
                    RendererError::VulkanError(format!("Failed to reset fence: {:?}", e))
                })?;
        }

        // Submit command buffer
        self.context.gfx_queue.submit(
            &[command_buffer],
            &[wait_semaphore],
            &[signal_semaphore],
            in_flight_fence,
        );

        Ok(())
    }

    /// Present the swapchain image to screen.
    fn present_swapchain(&self, image_index: u32) -> Result<(), RendererError> {
        let signal_semaphore = self.swap_data.render_finished_semaphore(image_index);

        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(std::slice::from_ref(&signal_semaphore))
            .swapchains(std::slice::from_ref(
                &self.frame_context.swapchain.swapchain,
            ))
            .image_indices(std::slice::from_ref(&image_index));

        let swapchain_loader = self.context.swapchain_loader.as_ref().ok_or_else(|| {
            RendererError::SwapchainError("Swapchain loader not initialized".into())
        })?;

        unsafe {
            swapchain_loader
                .queue_present(self.context.gfx_queue.vk_queue(), &present_info)
                .map_err(|e| {
                    RendererError::SwapchainError(format!("Failed to present swapchain: {:?}", e))
                })?;
        }

        Ok(())
    }

    /// Get the bindless texture manager.
    pub fn bindless_manager(&self) -> &BindlessTextureManager {
        &self.bindless_manager
    }

    /// Get default bindless texture slot indices.
    ///
    /// These slots are reserved for default textures in the bindless texture array.
    /// Use these values when a material lacks a specific texture type.
    pub fn bindless_defaults(&self) -> BindlessDefaults {
        BindlessDefaults {
            albedo: crate::vulkan::bindless_texture::DEFAULT_ALBEDO_SLOT,
            normal: crate::vulkan::bindless_texture::DEFAULT_NORMAL_SLOT,
            metallic_roughness: crate::vulkan::bindless_texture::DEFAULT_MR_SLOT,
            occlusion: crate::vulkan::bindless_texture::DEFAULT_AO_SLOT,
            emission: crate::vulkan::bindless_texture::DEFAULT_EMISSION_SLOT,
        }
    }

    /// Get the bindless texture manager mutably.
    pub fn bindless_manager_mut(&mut self) -> &mut BindlessTextureManager {
        &mut self.bindless_manager
    }

    /// Get the texture manager.
    pub fn texture_manager(&self) -> &TextureManager {
        &self.texture_manager
    }

    /// Get the texture manager mutably.
    pub fn texture_manager_mut(&mut self) -> &mut TextureManager {
        &mut self.texture_manager
    }

    /// Set frame-level uniforms for the current frame.
    ///
    /// This should be called once per frame before `render_frame()`.
    /// The uniforms are used by all draw calls in the frame.
    ///
    /// # Arguments
    /// * `uniforms` - Frame uniforms containing view/proj matrices, camera position, and lighting
    pub fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        self.frame_uniforms = Some(uniforms);
    }

    /// Update frame uniforms in storage buffer.
    ///
    /// Should be called once per frame before rendering.
    ///
    /// # Arguments
    /// * `view` - View matrix (world-to-camera) - column-major [f32; 16]
    /// * `proj` - Projection matrix (camera-to-clip) - column-major [f32; 16]
    pub fn update_storage_frame(&mut self, view: &[f32; 16], proj: &[f32; 16]) {
        self.storage_manager.update_frame(view, proj);
    }

    /// Update object uniforms in storage buffer.
    ///
    /// # Arguments
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world) - column-major [f32; 16]
    /// * `color` - Color tint (RGBA)
    pub fn update_storage_object(&mut self, index: usize, model: &[f32; 16], color: &[f32; 4]) {
        self.storage_manager.update_object(index, model, color);
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

    /// Get output dimensions.
    pub fn output_extent(&self) -> Option<crate::Size2D> {
        self.output_target
            .as_ref()
            .map(|t| crate::Size2D::from(t.extent))
    }

    // ========================================================================
    // Render Target Management (Unified)
    // ========================================================================

    /// Texture IDs for built-in render targets.
    pub const VIEWPORT_TEXTURE_ID: u64 = 2;
    pub const PREVIEW_TEXTURE_ID: u64 = 101;

    /// Create or resize a render target for the given texture ID.
    ///
    /// # Arguments
    /// * `texture_id` - Unique ID for this render target (e.g., 2 for viewport, 101 for preview)
    /// * `width` - Width in pixels
    /// * `height` - Height in pixels
    /// * `count` - Number of targets to create (1 for single-buffered, FRAMES_IN_FLIGHT for double-buffered)
    pub fn init_render_target(
        &mut self,
        texture_id: u64,
        width: u32,
        height: u32,
        _count: usize, // Ignored - we only need one target per texture_id
    ) -> Result<(), vk::Result> {
        // Check if we need to resize
        let needs_resize = self
            .render_targets
            .iter()
            .find(|(id, _)| *id == texture_id)
            .map(|(_, t)| t.extent.width != width || t.extent.height != height)
            .unwrap_or(true);

        if needs_resize {
            // Remove old target if exists
            self.render_targets.retain(|(id, _)| *id != texture_id);

            // Create new target
            let target = ViewportRenderTarget::new(self.context.clone(), width, height)?;
            self.render_targets.push((texture_id, target));

            info!(
                "Render target {} created/resized to {}x{}",
                texture_id, width, height
            );
        }
        Ok(())
    }

    /// Get a render target by texture ID.
    pub fn get_render_target(&self, texture_id: u64) -> Option<&ViewportRenderTarget> {
        self.render_targets
            .iter()
            .find(|(id, _)| *id == texture_id)
            .map(|(_, t)| t)
    }

    /// Get the first render target for a texture ID (alias for get_render_target).
    pub fn get_render_target_first(&self, texture_id: u64) -> Option<&ViewportRenderTarget> {
        self.get_render_target(texture_id)
    }

    /// Check if a render target exists for the given texture ID.
    pub fn has_render_target(&self, texture_id: u64) -> bool {
        self.render_targets.iter().any(|(id, _)| *id == texture_id)
    }

    /// Remove a render target.
    pub fn remove_render_target(&mut self, texture_id: u64) {
        self.render_targets.retain(|(id, _)| *id != texture_id);
    }

    // ========================================================================
    // Viewport Render Target (Convenience Methods)
    // ========================================================================

    /// Initialize viewport render target (double-buffered for frames in flight).
    pub fn init_viewport_target(&mut self, width: u32, height: u32) -> Result<(), vk::Result> {
        self.init_render_target(Self::VIEWPORT_TEXTURE_ID, width, height, 1)
    }

    /// Get viewport dimensions.
    pub fn viewport_extent(&self) -> Option<crate::Size2D> {
        self.get_render_target_first(Self::VIEWPORT_TEXTURE_ID)
            .map(|t| crate::Size2D::from(t.extent))
    }

    // ========================================================================
    // Viewport System (New Unified API)
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
    pub fn create_viewport(&self) -> ViewportBuilder {
        ViewportBuilder::new()
    }

    /// Get the number of viewports.
    pub fn viewport_count(&self) -> usize {
        self.viewports.len()
    }

    /// Get viewport by handle.
    pub fn get_viewport(&self, handle: ViewportHandle) -> Option<&Viewport> {
        self.viewports.get(handle.0)
    }

    /// Get mutable viewport by handle.
    pub fn get_viewport_mut(&mut self, handle: ViewportHandle) -> Option<&mut Viewport> {
        self.viewports.get_mut(handle.0)
    }

    /// Get the texture ID for a viewport (for UI sampling).
    /// Returns a u64 that can be used with katla_ui::TextureId::custom(id).
    pub fn viewport_texture_id(&self, handle: ViewportHandle) -> Option<u64> {
        self.viewports.get(handle.0).map(|_| {
            // Generate a unique texture ID based on viewport index
            // Using range 200+ to avoid conflicts with existing texture IDs
            200 + handle.0 as u64
        })
    }

    /// Get the viewport extent (by handle).
    pub fn get_viewport_extent(&self, handle: ViewportHandle) -> Option<crate::Size2D> {
        self.viewports.get(handle.0).map(|v| v.get_extent())
    }

    /// Set frame uniforms for a viewport.
    pub fn set_viewport_uniforms(&mut self, handle: ViewportHandle, uniforms: FrameUniforms) {
        if let Some(viewport) = self.viewports.get_mut(handle.0) {
            viewport.set_frame_uniforms(uniforms);
        }
    }

    /// Set the draw list for a viewport.
    pub fn set_viewport_draw_list(&self, handle: ViewportHandle, draw_list: DrawList) {
        if let Some(viewport) = self.viewports.get(handle.0) {
            viewport.set_draw_list(draw_list);
        }
    }

    /// Clear the draw list for a viewport.
    pub fn clear_viewport_draw_list(&self, handle: ViewportHandle) {
        if let Some(viewport) = self.viewports.get(handle.0) {
            viewport.clear_draw_list();
        }
    }

    /// Destroy a viewport by handle.
    pub fn destroy_viewport(&mut self, handle: ViewportHandle) {
        if handle.0 < self.viewports.len() {
            // Remove the viewport (Drop handles cleanup)
            self.viewports.remove(handle.0);
            info!("Viewport {} destroyed", handle.0);
        }
    }

    /// Check if a viewport is ready for rendering.
    pub fn is_viewport_ready(&self, handle: ViewportHandle) -> bool {
        self.viewports
            .get(handle.0)
            .is_some_and(|v| v.storage_manager.is_some() && v.storage_descriptor.is_some())
    }

    /// Update viewport camera and lighting.
    ///
    /// Call this each frame before rendering to update the viewport's view/projection
    /// matrices and lighting parameters.
    pub fn update_viewport_camera(
        &mut self,
        handle: ViewportHandle,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
        inv_view_proj: &[f32; 16],
        camera_position: &[f32; 4],
        light_direction: &[f32; 4],
        light_color: &[f32; 4],
        light_intensity: f32,
    ) {
        if let Some(viewport) = self.viewports.get_mut(handle.0)
            && let Some(ref mut manager) = viewport.storage_manager
        {
            manager.update_frame_with_lighting(
                view_matrix,
                proj_matrix,
                inv_view_proj,
                camera_position,
                light_direction,
                light_color,
                light_intensity,
            );
        }
    }

    pub fn destroy(&mut self) {
        // Destroy output render target (Drop handles cleanup)
        self.output_target = None;

        // Destroy all render targets (Drop handles cleanup)
        self.render_targets.clear();

        // Destroy all viewports (Drop handles cleanup for ViewportRenderTarget)
        self.viewports.clear();

        // Destroy all registered assets first (materials, meshes)
        self.asset_registry.destroy();

        // Storage uniform resources will be dropped automatically
        self.context.pre_destroy();
        self.swap_data.destroy(&self.context.device);
        self.frame_context.destroy();
        info!("Clean shutdown!");
    }

    pub fn wait_for_device(&self) {
        unsafe {
            self.context.device.device_wait_idle().unwrap();
        }
    }

    pub fn recreate_swapchain(&mut self) {
        self.wait_for_device();

        let old_extent = self.frame_context.swapchain.get_extent();
        info!("=== Recreating swapchain ===");
        info!("  Old extent: {}x{}", old_extent.width, old_extent.height);

        self.frame_context.recreate_swapchain();

        let new_extent = self.frame_context.swapchain.get_extent();
        info!("  New extent: {}x{}", new_extent.width, new_extent.height);
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
        use crate::renderer::registry::MeshAsset;
        use crate::vulkan::*;

        // Convert vertices to bytes
        let vertex_bytes = unsafe {
            std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                std::mem::size_of_val(vertices),
            )
        };

        // Convert indices to bytes
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };

        // Determine index type
        let index_type = match std::mem::size_of::<U>() {
            1 => IndexType::Uint8,
            2 => IndexType::Uint16,
            4 => IndexType::Uint32,
            _ => IndexType::None,
        };

        // Determine index count
        let index_count = match index_type {
            IndexType::Uint8 => index_bytes.len() as u32,
            IndexType::Uint16 => (index_bytes.len() as u32) / 2,
            IndexType::Uint32 => (index_bytes.len() as u32) / 4,
            IndexType::None => 0_u32,
        };

        // Create vertex buffer and upload data
        let vertex_buffer = if !vertex_bytes.is_empty() {
            let mut vb = VertexBuffer::new(
                self.context.clone(),
                vertex_bytes.len() as u64,
                vertices.len() as u32,
            );
            vb.upload_data(vertex_bytes);
            Some(vb)
        } else {
            None
        };

        // Create index buffer and upload data
        let index_buffer = if !index_bytes.is_empty() {
            let mut ib = IndexBuffer::new(
                self.context.clone(),
                index_bytes.len() as u64,
                index_type,
                index_count,
            );
            ib.upload_data(index_bytes);
            Some(ib)
        } else {
            None
        };

        let mesh_asset = MeshAsset {
            vertex_buffer,
            index_buffer,
        };

        self.asset_registry.register_mesh(mesh_asset)
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
        use crate::renderer::registry::MeshAsset;

        let mesh_asset = MeshAsset {
            vertex_buffer,
            index_buffer,
        };

        self.asset_registry.register_mesh(mesh_asset)
    }

    /// Create a cube mesh with the given size.
    ///
    /// # Arguments
    /// * `size` - The size of the cube as [width, height, depth]
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub fn create_cube_mesh(&mut self, size: [f32; 3]) -> MeshHandle {
        let (vertices, indices) = crate::primitives::generate_cube(size);
        self.create_mesh(&vertices, &indices)
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
        let (vertices, indices) = crate::primitives::generate_sphere(radius, segments, rings);
        self.create_mesh(&vertices, &indices)
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
        let (vertices, indices) = crate::primitives::generate_plane(width, height);
        self.create_mesh(&vertices, &indices)
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
        let (vertices, indices) = crate::primitives::generate_cylinder(height, radius, segments);
        self.create_mesh(&vertices, &indices)
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
        let (vertices, indices) =
            crate::primitives::generate_torus(major_radius, minor_radius, segments, rings);
        self.create_mesh(&vertices, &indices)
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
        let (vertices, indices) = crate::primitives::generate_plane_xy(width, height, segments);
        self.create_mesh(&vertices, &indices)
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
        use crate::renderer::registry::MeshAsset;
        use crate::vulkan::IndexType;

        // Create vertex buffer
        let vertex_buffer = if !vertex_data.is_empty() {
            let mut vb =
                VertexBuffer::new(self.context.clone(), vertex_data.len() as u64, vertex_count);
            vb.upload_data(vertex_data);
            Some(vb)
        } else {
            None
        };

        // Create index buffer (always u32 for UI)
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };

        let index_buffer = if !indices.is_empty() {
            let mut ib = IndexBuffer::new(
                self.context.clone(),
                index_bytes.len() as u64,
                IndexType::Uint32,
                indices.len() as u32,
            );
            ib.upload_data(index_bytes);
            Some(ib)
        } else {
            None
        };

        let mesh_asset = MeshAsset {
            vertex_buffer,
            index_buffer,
        };

        self.asset_registry.register_mesh(mesh_asset)
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
        _vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        let mesh_asset = self
            .asset_registry
            .get_mesh_mut(mesh)
            .ok_or_else(|| RendererError::NotFound("Mesh handle not found".to_string()))?;

        // Update vertex buffer
        if let Some(ref mut vb) = mesh_asset.vertex_buffer {
            vb.upload_data(vertex_data);
        }

        // Update index buffer
        if let Some(ref mut ib) = mesh_asset.index_buffer {
            let index_bytes = unsafe {
                std::slice::from_raw_parts(
                    indices.as_ptr() as *const u8,
                    std::mem::size_of_val(indices),
                )
            };
            ib.upload_data(index_bytes);
        }

        Ok(())
    }

    /// Register a Material with the renderer.
    ///
    /// This method registers a material instance for use in rendering.
    /// Materials can be created from templates or directly from pipelines.
    ///
    /// # Arguments
    /// * `material` - The material to register
    ///
    /// # Returns
    /// A `MaterialHandle` that references the registered material.
    ///
    /// # Example
    /// ```ignore
    /// let material = Material::new(template_handle)
    ///     .with_texture(0, albedo_texture);
    ///
    /// let handle = renderer.register_material(&material);
    /// ```
    pub fn register_material(&mut self, material: &Material) -> MaterialHandle {
        use crate::renderer::registry::MaterialAsset;

        let pipeline = material.pipeline();
        let vertex_binding = material
            .vertex_binding()
            .expect("Material must have vertex binding")
            .clone();
        let is_bindless = material.is_bindless();

        // Extract texture indices from material (convert handles to u32 indices)
        let texture_handles = material.texture_slots();
        let texture_indices = [
            texture_handles[0].index(),
            texture_handles[1].index(),
            texture_handles[2].index(),
            texture_handles[3].index(),
        ];

        let material_asset = MaterialAsset {
            pipeline,
            vertex_binding,
            texture_indices,
            emission_index: 0,
            uses_bindless: is_bindless,
        };

        self.asset_registry.register_material(material_asset)
    }

    /// Returns the default white PBR material handle.
    ///
    /// The default material is a simple bindless PBR material that renders
    /// geometry with white albedo and default PBR parameters.
    ///
    /// # Panics
    /// Panics if `init_default_material()` has not been called.
    ///
    /// # Example
    /// ```ignore
    /// // Initialize first (typically during application startup)
    /// renderer.init_default_material(binding, PathBuf::from("shaders/pbr.wgsl"));
    ///
    /// // Then use the default material
    /// let material = renderer.default_material();
    /// let draw = DrawCall::new(mesh, material);
    /// ```
    pub fn default_material(&self) -> MaterialHandle {
        self.default_material_handle
            .expect("default_material() called before init_default_material()")
    }

    /// Submits a slice of DrawCalls for immediate rendering.
    ///
    /// This is a convenience method for simple cases with few draw calls.
    /// It creates a temporary DrawList, populates it with the provided draw calls,
    /// and submits it through the existing render infrastructure.
    ///
    /// # Performance
    /// For >100 draws/frame, use `DrawList` directly to avoid repeated
    /// submission overhead. This method is optimized for convenience in simple cases.
    ///
    /// # Arguments
    /// * `draw_calls` - Slice of DrawCall objects to render
    ///
    /// # Returns
    /// `Ok(())` on success, or an error if submission fails.
    ///
    /// # Example
    /// ```ignore
    /// let mesh = renderer.create_cube_mesh([1.0, 1.0, 1.0]);
    /// let material = renderer.default_material();
    ///
    /// let draw = DrawCall::new(mesh, material)
    ///     .with_transform(model_matrix)
    ///     .with_color([1.0, 0.0, 0.0, 1.0]);
    ///
    /// renderer.draw_immediate(&[draw])?;
    /// ```
    pub fn draw_immediate(&mut self, draw_calls: &[DrawCall]) -> Result<(), RendererError> {
        if draw_calls.is_empty() {
            return Ok(());
        }

        // Create a temporary DrawList and populate it
        let mut draw_list = DrawList::new();
        for draw in draw_calls {
            draw_list.push(draw.clone());
        }

        // Submit through the draw_list_cell
        *self.draw_list_cell.borrow_mut() = Some(draw_list);

        Ok(())
    }

    /// Convenience: draw a single mesh with default material.
    ///
    /// # Performance
    /// Zero heap allocation for single mesh drawing.
    /// For >100 draws/frame, use `DrawList` directly.
    #[inline]
    pub fn draw_mesh(
        &mut self,
        mesh: MeshHandle,
        transform: [f32; 16],
    ) -> Result<(), RendererError> {
        let material = self.default_material();
        self.draw_mesh_with_material(mesh, transform, material)
    }

    /// Convenience: draw a single mesh with custom material.
    ///
    /// # Performance
    /// Zero heap allocation using `std::slice::from_ref`.
    /// For >100 draws/frame, use `DrawList` directly.
    #[inline]
    pub fn draw_mesh_with_material(
        &mut self,
        mesh: MeshHandle,
        transform: [f32; 16],
        material: MaterialHandle,
    ) -> Result<(), RendererError> {
        let draw_call = DrawCall::new(mesh, material).with_transform(transform);
        self.draw_immediate(std::slice::from_ref(&draw_call))
    }

    /// Get the skeleton descriptor set for a handle.
    pub fn get_skeleton_descriptor(
        &self,
        handle: SkeletonHandle,
    ) -> Option<&SkeletonDescriptorSet> {
        self.skeleton_descriptors
            .get(handle.index() as usize)?
            .as_ref()
    }

    /// Queue UI for rendering in the next frame.
    ///
    /// Call this before `render_frame()` each frame. The data is consumed
    /// during `render_frame()` and rendered as an overlay.
    ///
    /// # Arguments
    /// * `vertex_bytes` - Raw vertex data (VertexUI as bytes)
    /// * `vertex_count` - Number of vertices
    /// * `indices` - Index data (u32)
    /// * `commands` - Draw commands with clip rects and texture indices
    /// * `screen_size` - Screen dimensions [width, height] in pixels
    pub fn render_ui(
        &mut self,
        vertex_bytes: &[u8],
        vertex_count: u32,
        indices: &[u32],
        commands: &[UiDrawCommand],
        screen_size: [f32; 2],
    ) {
        if vertex_bytes.is_empty() || indices.is_empty() || commands.is_empty() {
            return;
        }

        self.pending_ui = Some(UiFrameData {
            vertex_bytes: vertex_bytes.to_vec(),
            vertex_count,
            indices: indices.to_vec(),
            commands: commands.to_vec(),
            screen_size,
        });
    }
}

/// Offscreen render target for viewport rendering.
///
/// This holds the color and depth attachments for rendering the 3D scene
/// to a texture that can be sampled by the UI viewport panel.
pub struct ViewportRenderTarget {
    /// Color attachment image.
    color_image: vk::Image,
    color_memory: Option<gpu_allocator::vulkan::Allocation>,
    /// Color attachment image view (exposed for render graph).
    pub(crate) color_image_view: vk::ImageView,
    /// Depth attachment image.
    depth_image: vk::Image,
    depth_memory: Option<gpu_allocator::vulkan::Allocation>,
    depth_image_view: vk::ImageView,
    /// Render extent.
    pub extent: vk::Extent2D,
    /// Sampler for sampling the color texture.
    sampler: vk::Sampler,
    /// Context for cleanup.
    context: Rc<VulkanContext>,
}

impl ViewportRenderTarget {
    /// Create a new viewport render target with the given dimensions.
    pub fn new(context: Rc<VulkanContext>, width: u32, height: u32) -> Result<Self, vk::Result> {
        unsafe {
            let extent = vk::Extent2D { width, height };
            let extent3d = vk::Extent3D {
                width,
                height,
                depth: 1,
            };

            // Create color image (HDR format for PBR rendering with tonemapping)
            let color_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::R16G16B16A16_SFLOAT)
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .samples(vk::SampleCountFlags::TYPE_1);

            let (color_image, color_memory) =
                context.create_image(color_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create color image view
            let color_view_create_info = vk::ImageViewCreateInfo::default()
                .image(color_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R16G16B16A16_SFLOAT)
                .components(vk::ComponentMapping::default())
                .subresource_range(COLOR_SUBRESOURCE_RANGE);

            let color_image_view = context
                .device
                .create_image_view(&color_view_create_info, None)?;

            // Create depth image (D32_SFLOAT_S8_UINT to match pipeline formats)
            let depth_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::D32_SFLOAT_S8_UINT)
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .samples(vk::SampleCountFlags::TYPE_1);

            let (depth_image, depth_memory) =
                context.create_image(depth_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create depth image view
            let depth_view_create_info = vk::ImageViewCreateInfo::default()
                .image(depth_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::D32_SFLOAT_S8_UINT)
                .components(vk::ComponentMapping::default())
                .subresource_range(DEPTH_SUBRESOURCE_RANGE);

            let depth_image_view = context
                .device
                .create_image_view(&depth_view_create_info, None)?;

            // Create sampler
            let sampler = context.create_sampler_clamp_to_edge()?;

            // Transition images to their initial layouts
            let cmd_buffer = context.begin_single_time_commands();
            let cmd = cmd_buffer.vk_command_buffer();

            // Transition color to shader read only (since we blit to it, not render to it)
            // This matches the expected old_layout in the blit barrier
            let color_barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(color_image)
                .subresource_range(COLOR_SUBRESOURCE_RANGE)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            // Transition depth to depth stencil attachment optimal
            let depth_barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(depth_image)
                .subresource_range(DEPTH_SUBRESOURCE_RANGE)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(
                    vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                        | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                );

            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[color_barrier, depth_barrier],
            );

            context.end_single_time_commands(cmd_buffer);

            Ok(Self {
                color_image,
                color_memory: Some(color_memory),
                color_image_view,
                depth_image,
                depth_memory: Some(depth_memory),
                depth_image_view,
                extent,
                sampler: sampler.into(),
                context,
            })
        }
    }
}
impl Drop for ViewportRenderTarget {
    fn drop(&mut self) {
        unsafe {
            self.context.device.destroy_sampler(self.sampler, None);
            self.context
                .device
                .destroy_image_view(self.color_image_view, None);
            self.context.device.destroy_image(self.color_image, None);
            if let Some(memory) = self.color_memory.take() {
                self.context.allocator.borrow_mut().free(memory).ok();
            }
            self.context
                .device
                .destroy_image_view(self.depth_image_view, None);
            self.context.device.destroy_image(self.depth_image, None);
            if let Some(memory) = self.depth_memory.take() {
                self.context.allocator.borrow_mut().free(memory).ok();
            }
        }
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

            let color_barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(color_image)
                .subresource_range(COLOR_SUBRESOURCE_RANGE)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[color_barrier],
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
            if let Some(memory) = self.color_memory.take() {
                self.context.allocator.borrow_mut().free(memory).ok();
            }
        }
    }
}
