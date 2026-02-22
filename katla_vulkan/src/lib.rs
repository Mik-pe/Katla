pub mod error;
pub mod render_graph;
pub mod renderer;
pub mod rendering;
pub mod sync;
pub mod viewport;
pub mod vulkan;

pub use error::RendererError;
use log::{error, info, warn};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
pub use render_graph::errors::RenderGraphError;
pub use render_graph::pass::{PassBuilder, PassExecutionContext};
pub use render_graph::resource::{
    CompiledResource, ResourceAccessType, ResourceId, ResourceKind, ResourceLifetime, ResourceUsage,
};
pub use render_graph::*;
pub use rendering::{
    registry::AssetRegistry,
    types::{
        DrawCall, DrawList, FrameUniforms, InstanceData, MaterialHandle, MeshHandle,
        ParticleDispatch, ParticleRender, SkeletonHandle,
    },
};
pub use sync::{
    VkBuffer, VkCommandBuffer, VkDescriptorPool, VkDescriptorSet, VkDescriptorSetLayout, VkFence,
    VkFramebuffer, VkImage, VkImageView, VkPipeline, VkPipelineLayout, VkRenderPass, VkSampler,
    VkSemaphore,
};
pub use viewport::{DepthFormat, OutputMode, ViewportBuilder, ViewportHandle};
pub use vulkan::context::{ValidationMessage, ValidationMessageType, ValidationSeverity};
pub use vulkan::material::storage_uniform::*;
pub use vulkan::*;

use ash::vk;
use std::{cell::RefCell, ffi::CString, rc::Rc};
use viewport::Viewport;

// Internal imports (not re-exported)
use sync::{COLOR_SUBRESOURCE_RANGE, DEPTH_SUBRESOURCE_RANGE};

pub struct FrameData {
    pub available_sem: VkSemaphore,
    pub finished_sem: VkSemaphore,
    pub in_flight_fence: VkFence,
    pub image_index: u32,
}

pub struct VulkanRenderer {
    pub context: Rc<VulkanContext>,
    pub frame_context: VulkanFrameCtx,
    pub swap_data: SwapData,
    pub current_framedata: Option<FrameData>,
    /// Asset registry for managing GPU resources (meshes, materials).
    /// This stores the actual Vulkan buffers and pipelines, while the application
    /// only holds opaque handles (MeshHandle, MaterialHandle).
    pub asset_registry: AssetRegistry,
    /// Material registry for template-based materials with hot reload.
    /// Loads materials from TOML files and supports runtime shader reloading.
    /// Wrapped in Rc to allow cloning for safe access during model loading.
    pub material_registry: Rc<RefCell<MaterialRegistry>>,
    /// Bindless texture manager for efficient texture binding.
    /// When enabled, all textures are stored in a single array accessed by index.
    /// Textures indices are passed via ObjectUniforms.texture_indices.
    pub bindless_manager: Option<BindlessTextureManager>,
    /// The render graph - single graph with multiple framebuffers (one per swapchain image)
    pub render_graph: Option<CompiledRenderGraph>,
    /// Storage uniform manager for storage buffer-based uniforms.
    /// When enabled, materials use storage buffers with instance indexing
    /// instead of descriptor-based uniforms.
    pub storage_manager: Option<StorageUniformManager>,
    /// Storage descriptor set for binding storage buffers to shaders (set 0).
    pub storage_descriptor_set: Option<StorageDescriptorSet>,
    /// Draw list cell for geometry pass (shared with render graph).
    pub draw_list_cell: Option<Rc<RefCell<Option<DrawList>>>>,
    /// Skeleton descriptor sets for GPU skeletal animation.
    /// Indexed by SkeletonHandle.
    pub skeleton_descriptors: Vec<Option<SkeletonDescriptorSet>>,
    /// Frame-level uniforms set once per frame via set_frame_uniforms().
    pub frame_uniforms: Option<FrameUniforms>,
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
}

/// Number of frames that can be in flight on the GPU at once.
/// Used for double-buffering of per-frame resources.
pub const FRAMES_IN_FLIGHT: usize = 2;

impl VulkanRenderer {
    pub fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Self {
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

        Self {
            context,
            frame_context,
            swap_data,
            current_framedata: None,
            asset_registry: AssetRegistry::new(),
            material_registry: Rc::new(RefCell::new(MaterialRegistry::new())),
            bindless_manager: None,
            render_graph: None,
            storage_manager: None,
            storage_descriptor_set: None,
            draw_list_cell: None,
            skeleton_descriptors: Vec::new(),
            frame_uniforms: None,
            render_targets: Vec::new(),
            output_target: None,
            viewports: Vec::new(),
        }
    }

    /// Initialize bindless texture system.
    ///
    /// This creates the bindless texture manager for efficient texture binding.
    /// All textures are stored in a single array and accessed by index.
    ///
    /// # Returns
    /// Ok(()) on success, or an error if initialization fails
    pub fn init_bindless(&mut self) -> Result<(), RendererError> {
        let manager = BindlessTextureManager::new(self.context.clone())?;
        self.bindless_manager = Some(manager);
        info!(
            "Bindless texture system initialized (max {} textures)",
            MAX_BINDLESS_TEXTURES
        );
        Ok(())
    }

    /// Get the bindless texture manager.
    pub fn bindless_manager(&self) -> Option<&BindlessTextureManager> {
        self.bindless_manager.as_ref()
    }

    /// Get the bindless texture manager mutably.
    pub fn bindless_manager_mut(&mut self) -> Option<&mut BindlessTextureManager> {
        self.bindless_manager.as_mut()
    }

    /// Initialize storage uniform system.
    ///
    /// This creates the storage uniform manager and descriptor set for
    /// storage buffer-based uniform access with instance indexing.
    /// Must be called before using storage buffer rendering.
    ///
    /// # Arguments
    /// * `uniform_desc_layout` - Descriptor set layout for uniform set (set 0)
    ///
    /// # Returns
    /// Ok(()) on success, or an error if initialization fails
    pub fn init_storage(
        &mut self,
        uniform_desc_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<(), RendererError> {
        let manager = StorageUniformManager::new(self.context.clone())?;
        let descriptor_set = manager.create_descriptor_set(&self.context, uniform_desc_layout)?;

        self.storage_manager = Some(manager);
        self.storage_descriptor_set = Some(descriptor_set);

        info!("Storage uniform system initialized (20KB buffer, 256 objects max)");
        Ok(())
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
    /// * `view` - View matrix (world-to-camera)
    /// * `proj` - Projection matrix (camera-to-clip)
    pub fn update_storage_frame(&mut self, view: &[[f32; 4]; 4], proj: &[[f32; 4]; 4]) {
        if let Some(ref mut manager) = self.storage_manager {
            manager.update_frame(view, proj);
        }
    }

    /// Update object uniforms in storage buffer.
    ///
    /// # Arguments
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world)
    /// * `color` - Color tint (RGBA)
    pub fn update_storage_object(&mut self, index: usize, model: &[[f32; 4]; 4], color: &[f32; 4]) {
        if let Some(ref mut manager) = self.storage_manager {
            manager.update_object(index, model, color);
        }
    }

    /// Get storage descriptor set for binding (set 0).
    ///
    /// Returns None if storage system not initialized.
    pub fn storage_descriptor(&self) -> Option<VkDescriptorSet> {
        self.storage_descriptor_set.as_ref().map(|ds| ds.set())
    }

    /// Check if storage uniform system is initialized.
    pub fn is_storage_initialized(&self) -> bool {
        self.storage_manager.is_some() && self.storage_descriptor_set.is_some()
    }

    /// Create and initialize storage system with standard layout.
    ///
    /// This creates the uniform descriptor set layout and initializes
    /// the storage manager. Should be called before any materials are created.
    pub fn init_storage_standard(&mut self) -> Result<(), RendererError> {
        use vulkan::material::DescriptorLayoutBuilder;

        // Create standard storage uniform layout (set 0)
        let uniform_set_layout = DescriptorLayoutBuilder::new()
            // Binding 0: Frame uniforms (view/proj) as storage buffer
            .add_binding(
                0,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                1,
            )
            // Binding 1: Object array (model/color per object) as storage buffer
            .add_binding(
                1,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                1,
            )
            .build(&self.context.device)
            .map_err(|e| {
                error!("Failed to create storage uniform layout: {:?}", e);
                RendererError::InitializationFailed(
                    "Failed to create storage uniform layout".to_string(),
                )
            })?;

        // Initialize storage manager and descriptor set
        let manager = StorageUniformManager::new(self.context.clone())?;
        let descriptor_set = manager.create_descriptor_set(
            &self.context,
            crate::sync::VkDescriptorSetLayout::new(uniform_set_layout),
        )?;

        self.storage_manager = Some(manager);
        self.storage_descriptor_set = Some(descriptor_set);

        // Clean up the layout (materials will create their own)
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(uniform_set_layout, None);
        }

        info!("Storage uniform system initialized (20KB buffer, 256 objects max)");
        Ok(())
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

    /// Get the output color image view (for rendering UI).
    pub fn output_color_view(&self) -> Option<VkImageView> {
        self.output_target
            .as_ref()
            .map(|t| VkImageView::new(t.color_image_view))
    }

    /// Get the output color image (for present pass blit).
    pub fn output_color_image(&self) -> Option<VkImage> {
        self.output_target
            .as_ref()
            .map(|t| VkImage::new(t.color_image))
    }

    /// Get output dimensions.
    pub fn output_extent(&self) -> Option<render_graph::types::Extent2D> {
        self.output_target
            .as_ref()
            .map(|t| render_graph::types::Extent2D::from(t.extent))
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

    /// Get the viewport color image view.
    pub fn viewport_color_view(&self) -> Option<VkImageView> {
        self.get_render_target_first(Self::VIEWPORT_TEXTURE_ID)
            .map(|t| t.color_view())
    }

    /// Get the viewport depth image.
    pub fn viewport_depth_image(&self) -> Option<VkImage> {
        self.get_render_target_first(Self::VIEWPORT_TEXTURE_ID)
            .map(|t| VkImage::new(t.depth_image()))
    }

    /// Get the viewport color image.
    pub fn viewport_color_image(&self) -> Option<VkImage> {
        self.get_render_target_first(Self::VIEWPORT_TEXTURE_ID)
            .map(|t| VkImage::new(t.color_image()))
    }

    /// Get viewport dimensions.
    pub fn viewport_extent(&self) -> Option<render_graph::types::Extent2D> {
        self.get_render_target_first(Self::VIEWPORT_TEXTURE_ID)
            .map(|t| render_graph::types::Extent2D::from(t.extent))
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

    /// Build a viewport from a builder configuration.
    ///
    /// This creates everything needed for rendering:
    /// - Render target (color + depth textures)
    /// - Storage uniform manager (independent camera)
    /// - Render graph with sky and geometry passes
    ///
    /// After building, use:
    /// - `set_viewport_camera()` to set the view/projection
    /// - `set_viewport_draw_list()` to set what to render
    /// - `viewport_texture_id()` to get the texture for UI display
    pub fn build_viewport(
        &mut self,
        builder: ViewportBuilder,
    ) -> Result<ViewportHandle, RenderGraphError> {
        // 1. Create the render target
        let mut viewport = Viewport::new(&builder, &self.context)?;
        let handle = ViewportHandle::new(self.viewports.len());

        // 2. Initialize storage manager for independent camera
        let manager = StorageUniformManager::new(self.context.clone()).map_err(|e| {
            RenderGraphError::CompilationError(format!("Failed to create storage: {:?}", e))
        })?;

        // Create descriptor set layout (same as main scene)
        use vulkan::material::DescriptorLayoutBuilder;

        let uniform_set_layout = DescriptorLayoutBuilder::new()
            .add_binding(
                0,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                1,
            )
            .add_binding(
                1,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                1,
            )
            .build(&self.context.device)
            .map_err(|e| {
                RenderGraphError::CompilationError(format!("Failed to create layout: {:?}", e))
            })?;

        let descriptor_set = manager
            .create_descriptor_set(
                &self.context,
                crate::sync::VkDescriptorSetLayout::new(uniform_set_layout),
            )
            .map_err(|e| {
                RenderGraphError::CompilationError(format!("Failed to create descriptor: {:?}", e))
            })?;

        // Clean up the layout (descriptor set holds a reference)
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(uniform_set_layout, None);
        }

        viewport.storage_manager = Some(manager);
        viewport.storage_descriptor = Some(descriptor_set);

        // 3. Build render graph for this viewport (sky + geometry → viewport texture)
        // All viewports use the same simple render graph that renders to their texture
        let render_graph = self.build_viewport_render_graph(&viewport, handle);
        viewport.render_graph = render_graph;

        // 4. Store viewport
        self.viewports.push(viewport);

        info!("Viewport '{}' created", builder.get_label());
        Ok(handle)
    }

    /// Build render graph for a viewport (sky + geometry passes → viewport texture).
    fn build_viewport_render_graph(
        &mut self,
        viewport: &Viewport,
        handle: ViewportHandle,
    ) -> Option<CompiledRenderGraph> {
        use crate::render_graph::types::ImageFormat;

        let mut graph_builder = RenderGraphBuilder::new();

        // Add viewport color resource
        let viewport_color = graph_builder.add_resource(
            format!("{}_color", viewport.label),
            ResourceKind::ExternalImage {
                image: viewport.color_image(),
                image_view: viewport.color_view(),
                format: ImageFormat::R16G16B16A16Sfloat,
                extent: viewport.extent,
            },
        );

        // Add viewport depth resource
        let viewport_depth = graph_builder.add_resource(
            format!("{}_depth", viewport.label),
            ResourceKind::ExternalImage {
                image: viewport.depth_image(),
                image_view: viewport.depth_view(),
                format: ImageFormat::D32SfloatS8Uint,
                extent: viewport.extent,
            },
        );

        // Store pointers for closures
        let viewport_index = handle.0;
        let clear_color = viewport.clear_color;

        // === SKY PASS ===
        // Note: Layout transitions are handled automatically by execute_pass_dynamic
        // No pre_execute callback needed - the render graph manages barriers
        let p_color = viewport_color;
        let p_depth = viewport_depth;
        graph_builder.add_pass("viewport_sky_pass", move |pass| {
            pass.write(Attachment::Color(p_color))
                .write(Attachment::DepthStencil(p_depth))
                .clear_color(p_color, clear_color)
                .clear_depth_stencil(p_depth, 1.0, 0)
                .execute("viewport_sky_pass", move |ctx| {
                    // Sky rendering handled by geometry pass for now
                    // (simplified - just clear and move on)
                    let _ = ctx;
                });
        });

        // === GEOMETRY PASS ===
        let p_color = viewport_color;
        let p_depth = viewport_depth;
        let viewport_index_capture = viewport_index;

        graph_builder.add_pass("viewport_geometry_pass", move |pass| {
            pass.write(Attachment::Color(p_color))
                .write(Attachment::DepthStencil(p_depth))
                .execute("viewport_geometry_pass", move |ctx| {
                    // Geometry rendering is handled externally via render_viewport()
                    // This pass just ensures proper barriers are in place
                    let _ = (ctx, viewport_index_capture);
                });
        });

        graph_builder.build(&self.context).ok()
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

    /// Get the color image view for a viewport (by handle).
    pub fn get_viewport_color_view(&self, handle: ViewportHandle) -> Option<VkImageView> {
        self.viewports.get(handle.0).map(|v| v.color_view())
    }

    /// Get the viewport extent (by handle).
    pub fn get_viewport_extent(
        &self,
        handle: ViewportHandle,
    ) -> Option<crate::render_graph::types::Extent2D> {
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

    /// Register a viewport texture with the UI system for sampling.
    ///
    /// Note: With the UI system moved to katla_app, this method is now a no-op.
    /// The application layer should manage viewport textures directly through UIRenderer.
    pub fn register_viewport_texture(&mut self, _handle: ViewportHandle) {
        // No-op - UI texture registration is now handled by the application layer
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
        self.viewports.get(handle.0).is_some_and(|v| {
            v.storage_manager.is_some() && v.storage_descriptor.is_some()
        })
    }

    /// Update viewport camera and lighting.
    ///
    /// Call this each frame before rendering to update the viewport's view/projection
    /// matrices and lighting parameters.
    pub fn update_viewport_camera(
        &mut self,
        handle: ViewportHandle,
        view_matrix: &[[f32; 4]; 4],
        proj_matrix: &[[f32; 4]; 4],
        inv_view_proj: &[[f32; 4]; 4],
        camera_position: &[f32; 4],
        light_direction: &[f32; 4],
        light_color: &[f32; 4],
        light_intensity: f32,
    ) {
        if let Some(viewport) = self.viewports.get_mut(handle.0) {
            if let Some(ref mut manager) = viewport.storage_manager {
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
    }

    pub fn destroy(&mut self) {
        // Destroy output render target (Drop handles cleanup)
        self.output_target = None;

        // Destroy all render targets (Drop handles cleanup)
        self.render_targets.clear();

        // Destroy render graph (holds framebuffers and resources)
        self.render_graph = None;

        // Destroy all registered assets first (materials, meshes)
        self.asset_registry.destroy();

        // Destroy material templates
        match self.material_registry.try_borrow_mut() {
            Ok(mut registry) => registry.destroy(),
            Err(_) => {
                // Already borrowed or other issue - log and continue
                warn!("Warning: Could not access material registry for destruction");
            }
        }

        // Destroy storage uniform resources (Drop handles cleanup)
        self.storage_descriptor_set = None;
        self.storage_manager = None;

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

        if let Some(ref mut graph) = self.render_graph {
            let extent_vk = self.frame_context.swapchain.get_extent();
            let new_extent =
                crate::render_graph::types::Extent2D::new(extent_vk.width, extent_vk.height);
            for pass in &mut graph.passes {
                pass.extent = new_extent;
            }

            // Destroy old framebuffers
            for pass in &graph.passes {
                for framebuffer in &pass.vk_framebuffers {
                    unsafe {
                        self.context
                            .device
                            .destroy_framebuffer(framebuffer.vk(), None);
                    }
                }
            }

            // Get the new depth texture image view (depth texture is recreated during swapchain recreation)
            let _new_depth_view = self.frame_context.depth_render_texture.image_view.vk();

            // No passes render directly to swapchain with color attachments anymore.
            // - sky_pass and geometry_pass render to viewport/output texture
            // - ui_pass renders to output texture
            // - present_pass uses transfer operations (blit) to copy output to swapchain
            // Therefore, no swapchain attachment updates are needed here.
        }
    }

    pub fn num_images(&self) -> usize {
        self.frame_context.swapchain_image_views.len()
    }

    /// Create a depth resource for the render graph.
    /// Returns a ResourceId for the depth texture that can be used in render graph passes.
    pub fn create_depth_resource(&self, builder: &mut RenderGraphBuilder) -> ResourceId {
        // Use the actual depth texture format to ensure compatibility
        use crate::render_graph::types::{Extent2D, ImageFormat};

        let depth_format = self.frame_context.depth_render_texture.format;
        let extent = self.frame_context.swapchain.get_extent();
        builder.add_resource(
            "depth",
            ResourceKind::ExternalImage {
                image: self.frame_context.depth_render_texture.image,
                image_view: self.frame_context.depth_render_texture.image_view,
                format: ImageFormat::from_vk(depth_format).expect("Unsupported depth format"),
                extent: Extent2D::new(extent.width, extent.height),
            },
        )
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
        use crate::rendering::registry::MeshAsset;
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
        use crate::rendering::registry::MeshAsset;

        let mesh_asset = MeshAsset {
            vertex_buffer,
            index_buffer,
        };

        self.asset_registry.register_mesh(mesh_asset)
    }

    /// Create a material from a material pipeline and optional texture.
    ///
    /// Returns a handle that can be used in DrawCall objects.
    ///
    /// # Arguments
    /// * `pipeline` - The material pipeline (shaders, descriptors, etc.)
    /// * `texture` - Optional texture bound to the material
    /// * `vertex_binding` - Vertex binding description for the pipeline
    /// * `uniform` - Optional per-material uniform buffer (for template-based materials)
    ///
    /// # Returns
    /// A `MaterialHandle` that references the registered material.
    pub fn create_material(
        &mut self,
        pipeline: Rc<RefCell<MaterialPipeline>>,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
    ) -> MaterialHandle {
        self.create_material_with_indices(pipeline, texture, vertex_binding, [0; 4], 0)
    }

    /// Create a material with bindless texture indices.
    pub fn create_material_with_indices(
        &mut self,
        pipeline: Rc<RefCell<MaterialPipeline>>,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> MaterialHandle {
        use crate::rendering::registry::MaterialAsset;

        let material_asset = MaterialAsset {
            pipeline,
            texture,
            vertex_binding,
            pbr_textures: None,
            texture_indices,
            emission_index,
            uses_bindless: true,
        };

        self.asset_registry.register_material(material_asset)
    }

    /// Register a material with all its data including optional per-material uniform buffer.
    ///
    /// This is a convenience method for registering materials from the application layer.
    ///
    /// # Arguments
    /// * `pipeline` - The material pipeline
    /// * `texture` - Optional texture
    /// * `vertex_binding` - Vertex binding description
    /// * `uniform` - Optional per-material uniform buffer
    /// * `texture_indices` - Bindless texture indices [albedo, normal, mr, ao]
    /// * `emission_index` - Bindless emission texture index
    ///
    /// # Returns
    /// A `MaterialHandle` that references the registered material.
    pub fn register_material_full(
        &mut self,
        pipeline: Rc<RefCell<MaterialPipeline>>,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> MaterialHandle {
        self.create_material_with_indices(
            pipeline,
            texture,
            vertex_binding,
            texture_indices,
            emission_index,
        )
    }

    /// Register a material with PBR textures.
    pub fn register_material_pbr(
        &mut self,
        pipeline: Rc<RefCell<MaterialPipeline>>,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        pbr_textures: crate::vulkan::material::PbrTextureSet,
        textures: Vec<Rc<Texture>>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> MaterialHandle {
        use crate::rendering::registry::MaterialAsset;

        let material_asset = MaterialAsset {
            pipeline,
            texture,
            vertex_binding,
            pbr_textures: None,
            texture_indices,
            emission_index,
            uses_bindless: true,
        };

        self.asset_registry
            .register_material_pbr(material_asset, pbr_textures, textures)
    }

    /// Register a skeleton buffer for GPU skeletal animation.
    ///
    /// Creates a descriptor set for the skeleton buffer and returns a handle
    /// that can be used to reference it in draw calls.
    ///
    /// # Arguments
    /// * `skeleton_buffer` - The skeleton buffer containing joint matrices
    /// * `skeleton_set_layout` - The descriptor set layout for skeleton binding (Set 2)
    ///
    /// # Returns
    /// A `SkeletonHandle` that references the registered skeleton.
    pub fn register_skeleton(
        &mut self,
        skeleton_buffer: Rc<RefCell<SkeletonBuffer>>,
        skeleton_set_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Option<SkeletonHandle> {
        // Create descriptor set for skeleton
        let descriptor =
            SkeletonDescriptorSet::new(self.context.clone(), skeleton_buffer, skeleton_set_layout)
                .ok()?;

        // Find an empty slot or add new one
        let handle = if let Some(slot) = self.skeleton_descriptors.iter().position(|s| s.is_none())
        {
            self.skeleton_descriptors[slot] = Some(descriptor);
            SkeletonHandle(slot as u32)
        } else {
            let handle = SkeletonHandle(self.skeleton_descriptors.len() as u32);
            self.skeleton_descriptors.push(Some(descriptor));
            handle
        };

        Some(handle)
    }

    /// Get the skeleton descriptor set for a handle.
    pub fn get_skeleton_descriptor(
        &self,
        handle: SkeletonHandle,
    ) -> Option<&SkeletonDescriptorSet> {
        self.skeleton_descriptors.get(handle.0 as usize)?.as_ref()
    }

    // ========================================================================
    // Render Graph Infrastructure (Generic)
    // ========================================================================

    /// Set the main render graph for rendering.
    ///
    /// The application builds its own render graph with the passes it needs,
    /// then passes it here. VulkanRenderer executes it during `render_frame()`.
    ///
    /// This makes the render graph generic - VulkanRenderer doesn't know about
    /// any application-specific passes.
    pub fn set_render_graph(&mut self, graph: CompiledRenderGraph) {
        self.render_graph = Some(graph);
    }

    /// Get a reference to the main render graph.
    pub fn render_graph(&self) -> Option<&CompiledRenderGraph> {
        self.render_graph.as_ref()
    }

    /// Get a mutable reference to the main render graph.
    pub fn render_graph_mut(&mut self) -> Option<&mut CompiledRenderGraph> {
        self.render_graph.as_mut()
    }

    // ========================================================================
    // Rendering Configuration (High-level API - application doesn't deal with Vulkan)
    // ========================================================================

    /// Setup render graph with pipelines.
    // ========================================================================
    // Render Graph Building (Application owns passes)
    // ========================================================================

    /// Create a render graph builder with resources pre-registered.
    ///
    /// This creates a builder with swapchain, viewport, and output resources
    /// already added. The returned `FrameResources` contains handles for
    /// referencing these resources in pass definitions.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (mut builder, resources) = renderer.create_render_graph_with_resources();
    ///
    /// // Add passes using resources
    /// builder.add_pass("sky_pass", |pass| {
    ///     pass.write_color(&resources.viewport_color)
    ///         .clear_color([0.4, 0.6, 0.9, 1.0]);
    /// });
    ///
    /// // Compile the graph with swapchain ID for proper layout transitions
    /// renderer.compile_render_graph(builder, Some(resources.swapchain.resource_id()))?;
    /// ```
    pub fn create_render_graph_with_resources(
        &self,
    ) -> (RenderGraphBuilder, render_graph::FrameResources) {
        use crate::render_graph::types::{Extent2D, ImageFormat};

        let mut builder = RenderGraphBuilder::new();

        // Get swapchain info
        let swapchain_format = self.frame_context.swapchain.format.format;
        let swapchain_extent = self.frame_context.swapchain.get_extent();

        // Add swapchain resource
        let swapchain_id = builder.add_resource(
            "swapchain",
            ResourceKind::ExternalImage {
                image: self.frame_context.swapchain_images[0],
                image_view: self.frame_context.swapchain_image_views[0],
                format: ImageFormat::from_vk(swapchain_format)
                    .expect("Unsupported swapchain format"),
                extent: Extent2D::new(swapchain_extent.width, swapchain_extent.height),
            },
        );

        // Add viewport resources if available
        let (viewport_color_id, viewport_depth_id) = if let Some(viewport) = self.viewports.first()
        {
            let color = builder.add_resource(
                "viewport_color",
                ResourceKind::ExternalImage {
                    image: viewport.color_image(),
                    image_view: viewport.color_view(),
                    format: ImageFormat::R16G16B16A16Sfloat,
                    extent: viewport.extent,
                },
            );
            let depth = builder.add_resource(
                "viewport_depth",
                ResourceKind::ExternalImage {
                    image: viewport.depth_image(),
                    image_view: viewport.depth_view(),
                    format: ImageFormat::D32SfloatS8Uint,
                    extent: viewport.extent,
                },
            );
            (color, depth)
        } else {
            // Fallback: use placeholder IDs (will need proper depth buffer later)
            let depth_id = self.create_depth_resource(&mut builder);
            // In legacy mode, output_color is the same as viewport_color
            (swapchain_id, depth_id)
        };

        // Add output resource if available
        let output_id = if let Some(ref output) = self.output_target {
            builder.add_resource(
                "output_color",
                ResourceKind::ExternalImage {
                    image: output.color_image.into(),
                    image_view: output.color_image_view.into(),
                    format: ImageFormat::R16G16B16A16Sfloat,
                    extent: Extent2D::new(output.extent.width, output.extent.height),
                },
            )
        } else {
            // No output target - use viewport color directly
            viewport_color_id
        };

        let resources = render_graph::FrameResources::new(
            swapchain_id,
            viewport_color_id,
            viewport_depth_id,
            output_id,
        );

        (builder, resources)
    }

    /// Compile a render graph from a builder.
    ///
    /// This builds the render graph and stores it internally for execution.
    /// After calling this, the graph can be executed each frame via `render_frame()`.
    ///
    /// # Arguments
    /// * `builder` - The render graph builder containing passes and resources
    /// * `swapchain_resource_id` - Optional ResourceId of the swapchain for proper layout transitions
    pub fn compile_render_graph(
        &mut self,
        builder: RenderGraphBuilder,
        swapchain_resource_id: Option<render_graph::ResourceId>,
    ) -> Result<(), render_graph::RenderGraphError> {
        let mut graph = builder.build(&self.context)?;

        // Set up renderer context for safe access to renderer state
        let renderer_context = self.create_renderer_context();
        graph.set_renderer_context(Rc::new(renderer_context));

        // Set swapchain resource ID for proper layout transitions during present
        if let Some(id) = swapchain_resource_id {
            graph.set_swapchain_resource_id(id);
        }

        self.render_graph = Some(graph);
        Ok(())
    }

    /// Create a RendererContext for the current renderer state.
    ///
    /// This is used to safely pass renderer state to render graph passes
    /// without requiring unsafe pointer patterns.
    fn create_renderer_context(&self) -> render_graph::RendererContext {
        // SAFETY: These pointers are valid for the lifetime of VulkanRenderer.
        // The render graph is stored in VulkanRenderer and will not outlive it.
        render_graph::RendererContext {
            pointers: render_graph::RendererContextPointers {
                asset_registry: std::ptr::addr_of!(self.asset_registry) as *mut _,
                storage_manager: std::ptr::addr_of!(self.storage_manager) as *mut _,
                storage_descriptor_set: std::ptr::addr_of!(self.storage_descriptor_set),
                skeleton_descriptors: std::ptr::addr_of!(self.skeleton_descriptors),
                bindless_manager: std::ptr::addr_of!(self.bindless_manager) as *mut _,
                vk_device: Some(self.context.device.clone()),
                push_descriptor_loader: Some(self.context.push_descriptor_loader.clone()),
            },
            draw_list: self.draw_list_cell.clone(),
        }
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

    /// Get the color image view for render graph and UI sampling.
    pub fn color_view(&self) -> VkImageView {
        VkImageView::new(self.color_image_view)
    }

    /// Get the depth image view for render graph.
    pub(crate) fn depth_view(&self) -> vk::ImageView {
        self.depth_image_view
    }

    /// Get the color image handle.
    pub(crate) fn color_image(&self) -> vk::Image {
        self.color_image
    }

    /// Get the depth image handle.
    pub(crate) fn depth_image(&self) -> vk::Image {
        self.depth_image
    }

    /// Get the sampler for this render target.
    pub fn vk_sampler(&self) -> vk::Sampler {
        self.sampler
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

            // Create color image (RGBA8, can be used as color attachment and transfer source)
            let color_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::B8G8R8A8_SRGB) // Match swapchain format
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
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
