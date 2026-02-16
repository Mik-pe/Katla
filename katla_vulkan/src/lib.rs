pub mod render_graph;
pub mod rendering;
pub mod sync;
pub mod vulkan;
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
    types::{DrawCall, DrawList, FrameUniforms, InstanceData, MaterialHandle, MeshHandle, ParticleDispatch, ParticleRender, SkeletonHandle},
};
pub use sync::{
    AccessFlags2, DependencyInfo, ImageMemoryBarrier2, PipelineStage2Flags,
    VkDescriptorPool, VkDescriptorSet, VkDescriptorSetLayout, VkFence, VkFramebuffer, VkImage,
    VkImageView, VkSampler, VkSemaphore,
};
pub use vulkan::context::{ValidationMessage, ValidationMessageType, ValidationSeverity};
pub use vulkan::material::storage_uniform::*;
pub use vulkan::*;

use ash::vk;
use std::{cell::RefCell, ffi::CString, rc::Rc};

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
    pub material_registry: RefCell<MaterialRegistry>,
    /// The render graph - single graph with multiple framebuffers (one per swapchain image)
    pub render_graph: Option<CompiledRenderGraph>,
    /// Storage uniform manager for storage buffer-based uniforms.
    /// When enabled, materials use storage buffers with instance indexing
    /// instead of descriptor-based uniforms.
    pub storage_manager: Option<StorageUniformManager>,
    /// Storage descriptor set for binding storage buffers to shaders (set 0).
    pub storage_descriptor_set: Option<StorageDescriptorSet>,
    /// Sky pipeline for procedural sky rendering.
    /// Created lazily when setup_render_graph_with_sky is called.
    pub sky_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    /// UI pipeline for overlay rendering.
    /// Created externally and passed via set_ui_pipeline.
    pub ui_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    /// Skeleton descriptor sets for GPU skeletal animation.
    /// Indexed by SkeletonHandle.
    skeleton_descriptors: Vec<Option<SkeletonDescriptorSet>>,
    /// Frame-level uniforms set once per frame via set_frame_uniforms().
    frame_uniforms: Option<FrameUniforms>,
    /// UI overlay data for immediate mode UI rendering.
    /// Set each frame via set_ui_data() and rendered in ui_pass.
    ui_data: RefCell<Option<UiDrawData>>,
    /// Persistent UI buffers (one set per frame in flight).
    ui_buffers: Vec<UIBuffers>,
    /// Current frame index for UI buffer selection.
    ui_frame_index: std::cell::Cell<usize>,
    /// UI textures (font atlas, white texture, descriptor set).
    ui_textures: Option<UITextures>,
    /// Offscreen render targets for viewport rendering (one per frame in flight to avoid races).
    viewport_targets: Vec<ViewportRenderTarget>,
    /// Output render target for final composition (UI renders here, then present_pass copies to swapchain).
    output_target: Option<OutputRenderTarget>,
    /// Viewport render graph (renders scene to viewport texture).
    viewport_render_graph: Option<CompiledRenderGraph>,
}

const FRAMES_IN_FLIGHT: usize = 2;

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
            material_registry: RefCell::new(MaterialRegistry::new()),
            render_graph: None,
            storage_manager: None,
            storage_descriptor_set: None,
            sky_pipeline: None,
            ui_pipeline: None,
            skeleton_descriptors: Vec::new(),
            frame_uniforms: None,
            ui_data: RefCell::new(None),
            ui_buffers: Vec::new(),
            ui_frame_index: std::cell::Cell::new(0),
            ui_textures: None,
            viewport_targets: Vec::new(),
            output_target: None,
            viewport_render_graph: None,
        }
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
        uniform_desc_layout: vk::DescriptorSetLayout,
    ) -> Result<(), vk::Result> {
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
    pub fn update_storage_object(
        &mut self,
        index: usize,
        model: &[[f32; 4]; 4],
        color: &[f32; 4],
    ) {
        if let Some(ref mut manager) = self.storage_manager {
            manager.update_object(index, model, color);
        }
    }

    /// Get storage descriptor set for binding (set 0).
    ///
    /// Returns None if storage system not initialized.
    pub fn storage_descriptor(&self) -> Option<vk::DescriptorSet> {
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
    pub fn init_storage_standard(&mut self) -> Result<(), vk::Result> {
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
                vk::Result::ERROR_INITIALIZATION_FAILED
            })?;

        // Initialize storage manager and descriptor set
        let manager = StorageUniformManager::new(self.context.clone())?;
        let descriptor_set = manager.create_descriptor_set(&self.context, uniform_set_layout)?;

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

    /// Initialize UI buffers for persistent buffer rendering.
    ///
    /// Creates one set of vertex/index buffers per frame in flight.
    /// Should be called once during initialization.
    ///
    /// # Arguments
    /// * `vertex_capacity` - Maximum vertex buffer size in bytes
    /// * `index_capacity` - Maximum index buffer size in bytes
    pub fn init_ui_buffers(&mut self, vertex_capacity: u64, index_capacity: u64) {
        // Create one set of buffers per frame in flight
        for _ in 0..FRAMES_IN_FLIGHT {
            let buffers = UIBuffers::new(&self.context, vertex_capacity, index_capacity);
            self.ui_buffers.push(buffers);
        }
        info!("UI buffers initialized ({} frames, {}KB vertex, {}KB index)",
            FRAMES_IN_FLIGHT,
            vertex_capacity / 1024,
            index_capacity / 1024);
    }

    /// Initialize UI textures for font atlas and solid color rendering.
    ///
    /// Creates font atlas texture and white fallback texture.
    ///
    /// # Arguments
    /// * `atlas_width` - Font atlas width in pixels
    /// * `atlas_height` - Font atlas height in pixels
    pub fn init_ui_textures(&mut self, atlas_width: u32, atlas_height: u32) -> Result<(), vk::Result> {
        let textures = UITextures::with_atlas_size(&self.context, atlas_width, atlas_height)?;
        info!("UI textures initialized ({}x{} font atlas)", atlas_width, atlas_height);
        self.ui_textures = Some(textures);
        Ok(())
    }

    /// Update font atlas texture with new pixel data.
    ///
    /// Call this when glyphs have been added to the font system's atlas.
    ///
    /// # Arguments
    /// * `pixels` - RGBA pixel data matching the atlas size
    pub fn update_font_atlas(&mut self, pixels: &[u8]) -> bool {
        if let Some(ref mut textures) = self.ui_textures {
            textures.update_font_atlas(&self.context, pixels)
        } else {
            false
        }
    }

    /// Get UI descriptor set for binding.
    pub fn ui_descriptor_set(&self) -> Option<vk::DescriptorSet> {
        self.ui_textures.as_ref().map(|t| t.descriptor_set)
    }

    /// Initialize or resize the viewport render target.
    ///
    /// This creates a single offscreen render target for rendering
    /// the 3D scene that can be sampled by the UI viewport panel.
    /// Single-buffered is fine with proper fence synchronization.
    pub fn init_viewport_target(&mut self, width: u32, height: u32) -> Result<(), vk::Result> {
        let needs_resize = self.viewport_targets.first()
            .map(|t| t.extent.width != width || t.extent.height != height)
            .unwrap_or(true);

        if needs_resize {
            // Destroy old target
            for target in self.viewport_targets.drain(..) {
                target.destroy(&self.context);
            }

            // Create single target
            let target = ViewportRenderTarget::new(&self.context, width, height)?;
            self.viewport_targets.push(target);
            info!("Viewport render target created/resized to {}x{}", width, height);

            // Update UI textures with the viewport image view
            if let Some(ref mut ui_textures) = self.ui_textures {
                if let Some(ref viewport_target) = self.viewport_targets.first() {
                    ui_textures.set_viewport_texture(&self.context, viewport_target.color_image_view);
                }
            }
        }
        Ok(())
    }

    /// Get the viewport color image view (for rendering).
    pub fn viewport_color_view(&self) -> Option<vk::ImageView> {
        self.viewport_targets.first().map(|t| t.color_image_view)
    }

    /// Get the viewport depth image (for rendering).
    pub fn viewport_depth_image(&self) -> Option<vk::Image> {
        self.viewport_targets.first().map(|t| t.depth_image)
    }

    /// Get the viewport color image (for rendering).
    pub fn viewport_color_image(&self) -> Option<vk::Image> {
        self.viewport_targets.first().map(|t| t.color_image)
    }

    /// Get the viewport depth image view (for rendering).
    pub fn viewport_depth_view(&self) -> Option<vk::ImageView> {
        self.viewport_targets.first().map(|t| t.depth_image_view)
    }

    /// Get the viewport sampler.
    pub fn viewport_sampler(&self) -> Option<vk::Sampler> {
        self.viewport_targets.first().map(|t| t.sampler)
    }

    /// Get viewport dimensions.
    pub fn viewport_extent(&self) -> Option<vk::Extent2D> {
        self.viewport_targets.first().map(|t| t.extent)
    }

    /// Initialize or resize the output render target.
    ///
    /// This creates a texture that the UI renders to, which is then
    /// copied to the swapchain by the present pass.
    pub fn init_output_target(&mut self, width: u32, height: u32) -> Result<(), vk::Result> {
        let needs_resize = self.output_target.as_ref()
            .map(|t| t.extent.width != width || t.extent.height != height)
            .unwrap_or(true);

        if needs_resize {
            let old_target = self.output_target.take();
            let target = OutputRenderTarget::resize(&self.context, old_target, width, height)?;
            self.output_target = Some(target);
            info!("Output render target created/resized to {}x{}", width, height);
        }
        Ok(())
    }

    /// Get the output color image view (for rendering UI).
    pub fn output_color_view(&self) -> Option<vk::ImageView> {
        self.output_target.as_ref().map(|t| t.color_image_view)
    }

    /// Get the output color image (for present pass blit).
    pub fn output_color_image(&self) -> Option<vk::Image> {
        self.output_target.as_ref().map(|t| t.color_image)
    }

    /// Get output dimensions.
    pub fn output_extent(&self) -> Option<vk::Extent2D> {
        self.output_target.as_ref().map(|t| t.extent)
    }

    pub fn destroy(&mut self) {
        // Destroy output render target
        if let Some(target) = self.output_target.take() {
            target.destroy(&self.context);
        }

        // Destroy viewport render targets
        for target in self.viewport_targets.drain(..) {
            target.destroy(&self.context);
        }

        // Destroy UI textures
        if let Some(textures) = self.ui_textures.take() {
            textures.destroy(&self.context);
        }

        // Destroy UI buffers
        for buffers in self.ui_buffers.drain(..) {
            buffers.destroy(&self.context);
        }

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

        // Update render graph's active render pass if it exists
        // Collect swapchain data first to avoid borrow checker issues
        let swapchain_images: Vec<_> = self
            .frame_context
            .swapchain_images
            .iter()
            .zip(self.frame_context.swapchain_image_views.iter())
            .map(|(image, view)| {
                let extent = self.frame_context.swapchain.get_extent();
                (
                    *image,
                    *view,
                    crate::render_graph::types::Extent2D::new(extent.width, extent.height),
                    self.frame_context.swapchain.format.format,
                )
            })
            .collect();

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

            // Collect pass indices that need swapchain attachment updates
            // ONLY ui_pass renders to swapchain with color attachments
            // copy_pass uses transfer operations, not color attachments
            // sky_pass and geometry_pass render to viewport texture (handled by setup_render_graph)
            let swapchain_pass_indices: Vec<usize> = graph.passes.iter().enumerate()
                .filter(|(_, pass)| pass.name == "ui_pass")
                .map(|(idx, _)| idx)
                .collect();

            // Recreate framebuffers with new swapchain images
            for (image_index, (_vk_image, image_view, _extent, _format)) in
                swapchain_images.iter().enumerate()
            {
                for &pass_idx in &swapchain_pass_indices {
                    // Ensure color_attachments array has an entry for this image index
                    while graph.passes[pass_idx].color_attachments.len() <= image_index {
                        graph.passes[pass_idx].color_attachments.push(vec![]);
                    }

                    // Update the color attachments for dynamic rendering
                    graph.passes[pass_idx].color_attachments[image_index] = vec![image_view.vk()];

                    // NOTE: For dynamic rendering, we don't create framebuffers
                    // Just ensure vk_framebuffers vector is initialized
                    if image_index == 0 && graph.passes[pass_idx].vk_framebuffers.is_empty() {
                        graph.passes[pass_idx].vk_framebuffers = vec![];
                    }
                }
            }
        }
    }

    pub fn num_images(&self) -> usize {
        self.frame_context.swapchain_image_views.len()
    }

    pub fn swap_frames(&mut self) -> Result<(), RenderGraphError> {
        self.swap_data.wait_for_fence(&self.context.device);

        let (available_sem, finished_sem, in_flight_fence, image_index) =
            self.swap_data.swap_images(
                &self.context.device,
                self.context
                    .swapchain_loader
                    .as_ref()
                    .expect("Swapchain loader required"),
                self.frame_context.swapchain.swapchain,
            )?;
        self.current_framedata = Some(FrameData {
            available_sem,
            finished_sem,
            in_flight_fence,
            image_index,
        });
        Ok(())
    }

    pub fn create_swapchain_resource(
        &self,
        builder: &mut RenderGraphBuilder,
        image_index: u32,
    ) -> ResourceId {
        // Get the actual format from the swapchain to ensure correctness
        let swapchain_format = self.frame_context.swapchain.format.format;
        builder.add_resource(
            format!("swapchain_{}", image_index),
            ResourceKind::ExternalImage {
                vk_image: self.frame_context.swapchain_images[image_index as usize].vk(),
                image_view: self.frame_context.swapchain_image_views[image_index as usize].vk(),
                format: swapchain_format,
                extent: self.frame_context.swapchain.get_extent(),
            },
        )
    }

    /// Create a depth resource for the render graph.
    /// Returns a ResourceId for the depth texture that can be used in render graph passes.
    pub fn create_depth_resource(&self, builder: &mut RenderGraphBuilder) -> ResourceId {
        // Use the actual depth texture format to ensure compatibility
        let depth_format = self.frame_context.depth_render_texture.format;
        builder.add_resource(
            "depth",
            ResourceKind::ExternalImage {
                vk_image: self.frame_context.depth_render_texture.image.vk(),
                image_view: self.frame_context.depth_render_texture.image_view.vk(),
                format: depth_format,
                extent: self.frame_context.swapchain.get_extent(),
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
        uniform: Option<crate::vulkan::material::UniformHandle>,
    ) -> MaterialHandle {
        use crate::rendering::registry::MaterialAsset;

        let material_asset = MaterialAsset {
            pipeline,
            texture,
            vertex_binding,
            uniform,
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
    ///
    /// # Returns
    /// A `MaterialHandle` that references the registered material.
    pub fn register_material_full(
        &mut self,
        pipeline: Rc<RefCell<MaterialPipeline>>,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        uniform: Option<crate::vulkan::material::UniformHandle>,
    ) -> MaterialHandle {
        self.create_material(pipeline, texture, vertex_binding, uniform)
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
        skeleton_set_layout: vk::DescriptorSetLayout,
    ) -> Option<SkeletonHandle> {
        // Create descriptor set for skeleton
        let descriptor = SkeletonDescriptorSet::new(
            self.context.clone(),
            skeleton_buffer,
            skeleton_set_layout,
        ).ok()?;

        // Find an empty slot or add new one
        let handle = if let Some(slot) = self.skeleton_descriptors.iter().position(|s| s.is_none()) {
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
    pub fn get_skeleton_descriptor(&self, handle: SkeletonHandle) -> Option<&SkeletonDescriptorSet> {
        self.skeleton_descriptors.get(handle.0 as usize)?.as_ref()
    }

    /// Setup a single render graph with multiple framebuffers (one per swapchain image).
    /// This creates the graph upfront during initialization to avoid
    /// destroying Vulkan objects while the GPU is still using them.
    ///
    /// The draw list will be provided each frame via `render_frame_with_drawlist`.
    ///
    /// When a viewport target exists, scene renders to viewport texture first,
    /// then copies to swapchain before UI rendering. This prevents UI recursion.
    pub fn setup_render_graph(&mut self) {
        // Build a single render graph
        let mut graph_builder = RenderGraphBuilder::new();

        // Create a placeholder swapchain resource (will be updated for each image)
        let swapchain_resource = graph_builder.add_resource(
            "swapchain",
            ResourceKind::ExternalImage {
                vk_image: self.frame_context.swapchain_images[0].vk(),
                image_view: self.frame_context.swapchain_image_views[0].vk(),
                format: self.frame_context.swapchain.format.format,
                extent: self.frame_context.swapchain.get_extent(),
            },
        );

        // Create viewport resources if we have viewport targets
        // Scene will render to viewport texture, then copy to swapchain for UI
        // Note: We use the first viewport texture here, but update_viewport_attachments
        // will switch to the correct texture each frame for double-buffering
        let (viewport_resource, viewport_depth_resource, _viewport_extent) = if let Some(first_target) = self.viewport_targets.first() {
            let color = graph_builder.add_resource(
                "viewport_color",
                ResourceKind::ExternalImage {
                    vk_image: first_target.color_image,
                    image_view: first_target.color_image_view,
                    format: vk::Format::R8G8B8A8_SRGB,
                    extent: first_target.extent,
                },
            );
            let depth = graph_builder.add_resource(
                "viewport_depth",
                ResourceKind::ExternalImage {
                    vk_image: first_target.depth_image,
                    image_view: first_target.depth_image_view,
                    format: vk::Format::D32_SFLOAT,
                    extent: first_target.extent,
                },
            );
            (Some(color), Some(depth), Some(first_target.extent))
        } else {
            (None, None, None)
        };

        let depth_resource = self.create_depth_resource(&mut graph_builder);

        // Create output resource for UI composition
        // UI renders to this texture, then present_pass copies it to swapchain
        // This decouples rendering from presentation for a cleaner architecture
        let output_resource = if let Some(ref output_target) = self.output_target {
            Some(graph_builder.add_resource(
                "output_color",
                ResourceKind::ExternalImage {
                    vk_image: output_target.color_image,
                    image_view: output_target.color_image_view,
                    format: vk::Format::B8G8R8A8_SRGB,
                    extent: output_target.extent,
                },
            ))
        } else {
            None
        };

        // Determine scene render targets based on viewport existence
        // When viewport exists: scene -> viewport texture, UI -> output texture
        // When no viewport: scene -> output texture directly
        let scene_color_res = viewport_resource.unwrap_or(output_resource.unwrap_or(swapchain_resource));
        let scene_depth_res = viewport_depth_resource.unwrap_or(depth_resource);
        let has_viewport = viewport_resource.is_some();
        let has_output = output_resource.is_some();

        // Create Rc<RefCell<>> for the draw list that will be set each frame
        let draw_list_cell: Rc<RefCell<Option<DrawList>>> = Rc::new(RefCell::new(None));
        let draw_list_cell_for_pass = draw_list_cell.clone(); // Clone for the closure

        // Store the asset registry pointer - we know it's valid for the lifetime of the renderer
        let asset_registry_ptr = &mut self.asset_registry as *mut AssetRegistry;

        // Store storage manager pointer for storage buffer-based uniforms
        let storage_manager_ptr = &mut self.storage_manager as *mut Option<StorageUniformManager>;
        let storage_descriptor_ptr = &mut self.storage_descriptor_set as *mut Option<StorageDescriptorSet>;

        // Store sky pipeline pointer
        let sky_pipeline_ptr = &mut self.sky_pipeline as *mut Option<Rc<RefCell<MaterialPipeline>>>;

        // Store skeleton descriptors pointer for GPU skeletal animation
        let skeleton_descriptors_ptr = &mut self.skeleton_descriptors as *mut Vec<Option<SkeletonDescriptorSet>>;

        // Store device pointer for particle rendering
        let device_ptr = self.context.device.clone();

        let scene_color = scene_color_res;
        let scene_depth = scene_depth_res;
        let uses_viewport = has_viewport;
        let device_for_sky = self.context.device.clone();

        // === SKY PASS ===
        // Renders first, clears color and depth, writes sky to color only
        // Uses scene_color/scene_depth which is either viewport or swapchain
        graph_builder.add_pass("sky_pass", move |pass| {
            pass.write(Attachment::Color(scene_color))
                .write(Attachment::DepthStencil(scene_depth))
                .clear_color(scene_color, [0.4, 0.6, 0.9, 1.0]) // Sky blue fallback
                .clear_depth_stencil(scene_depth, 1.0, 0)
                .execute("sky_pass", move |ctx| {
                    let cmd_buf = ctx.command_buffer.vk_command_buffer();

                    // When using viewport, we need to manually transition the viewport textures
                    // from SHADER_READ_ONLY_OPTIMAL (after UI sampling) to attachment optimal
                    if uses_viewport {
                        if let (Some((color_image, _)), Some((depth_image, _))) =
                            (ctx.get_image(scene_color), ctx.get_image(scene_depth))
                        {
                            let color_subresource = vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            };

                            let depth_subresource = vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::DEPTH,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            };

                            // Transition color: SHADER_READ_ONLY -> COLOR_ATTACHMENT_OPTIMAL
                            let color_barrier = ImageMemoryBarrier2::new(color_image)
                                .src_stage(PipelineStage2Flags::FRAGMENT_SHADER)
                                .src_access(AccessFlags2::SHADER_READ)
                                .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
                                .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
                                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                                .subresource_range(color_subresource);

                            // Transition depth: DEPTH_STENCIL_ATTACHMENT -> DEPTH_STENCIL_ATTACHMENT_OPTIMAL
                            // (depth stays in attachment format, just need proper stage sync)
                            let depth_barrier = ImageMemoryBarrier2::new(depth_image)
                                .src_stage(PipelineStage2Flags::LATE_FRAGMENT_TESTS)
                                .src_access(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
                                .dst_stage(PipelineStage2Flags::EARLY_FRAGMENT_TESTS)
                                .dst_access(AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
                                .old_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                                .subresource_range(depth_subresource);

                            DependencyInfo::new()
                                .add_image_barrier(color_barrier)
                                .add_image_barrier(depth_barrier)
                                .build(|dep_info| unsafe {
                                    device_for_sky.cmd_pipeline_barrier2(cmd_buf, dep_info);
                                });
                        }
                    }

                    // SAFETY: The pointers are valid for the entire lifetime of the renderer
                    let sky_pipeline_opt = unsafe { &mut *sky_pipeline_ptr };
                    let storage_descriptor_opt = unsafe { &mut *storage_descriptor_ptr };

                    if let (Some(sky_pipeline), Some(storage_descriptor)) =
                        (sky_pipeline_opt.as_ref(), storage_descriptor_opt.as_ref())
                    {
                        let pipeline_ref = sky_pipeline.borrow();

                        // Bind sky pipeline
                        unsafe {
                            pipeline_ref.context().device.cmd_bind_pipeline(
                                cmd_buf,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline_ref.vk_pipeline().handle,
                            );

                            // Bind storage descriptor set (set 0 = frame_data + objects)
                            pipeline_ref.context().device.cmd_bind_descriptor_sets(
                                cmd_buf,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline_ref.vk_layout(),
                                0,
                                &[storage_descriptor.set()],
                                &[],
                            );
                        }

                        drop(pipeline_ref);

                        // Draw fullscreen triangle (3 vertices, no vertex buffer)
                        ctx.command_buffer.draw_array(3, 1, 0, 0);
                    }
                });
        });

        // === GEOMETRY PASS ===
        // Renders after sky, uses load instead of clear (sky already filled background)
        // Uses scene_color/scene_depth which is either viewport or swapchain
        graph_builder.add_pass("geometry_pass", move |pass| {
            pass.write(Attachment::Color(scene_color))
                .write(Attachment::DepthStencil(scene_depth))
                // NO clear - sky pass already cleared and filled the background
                .execute("geometry_pass", move |ctx| {
                    // Get the draw list for this frame
                    let draw_list_opt = draw_list_cell_for_pass.borrow_mut().take();
                    if let Some(draw_list) = draw_list_opt {
                        // SAFETY: The pointers are valid for the entire lifetime of the renderer
                        // and this closure is only called while the renderer is alive
                        let registry = unsafe { &mut *asset_registry_ptr };
                        let storage_manager = unsafe { &mut *storage_manager_ptr };
                        let storage_descriptor = unsafe { &mut *storage_descriptor_ptr };

                        // Frame uniforms are now updated in render_frame() before the graph executes
                        // This ensures sky pass has valid data

                        // Track object index for storage mode
                        let mut next_object_index: u32 = 0;

                        // Process each draw call
                        for draw in &draw_list.draws {
                            // Get the mesh data first (immutable borrow)
                            let mesh_data = registry.get_mesh(draw.mesh).map(|m| {
                                (
                                    m.index_buffer
                                        .as_ref()
                                        .map(|ib| (ib.object(), ib.index_type, ib.count())),
                                    m.vertex_buffer.as_ref().map(|vb| (vb.object(), vb.count())),
                                )
                            });

                            // Then get material for mutable access
                            let material = match registry.get_material_mut(draw.material) {
                                Some(m) => m,
                                None => continue,
                            };

                            // Skip if mesh doesn't exist
                            let (index_data, vertex_data) = match mesh_data {
                                Some(data) => data,
                                None => continue,
                            };

                            // Auto-assign object index for storage buffer
                            let first_instance = next_object_index;
                            let instance_count = draw.instance_count();
                            next_object_index += instance_count;

                            let cmd_buf = ctx.command_buffer.vk_command_buffer();

                            // === Storage Buffer Mode: Upload instance data ===
                            // The shader uses @builtin(instance_index) to access objects[index]
                            if let Some(ref mut manager) = storage_manager.as_mut() {
                                if draw.is_instanced() {
                                    // Upload all instances to consecutive indices
                                    for (i, instance) in draw.instances.iter().enumerate() {
                                        let model: [[f32; 4]; 4] = bytemuck::cast(instance.model_matrix);
                                        manager.update_object_with_material(
                                            first_instance as usize + i,
                                            &model,
                                            &instance.color,
                                            instance.metallic,
                                            instance.roughness,
                                            instance.ao,
                                        );
                                    }
                                } else {
                                    // Single instance mode
                                    let model: [[f32; 4]; 4] = bytemuck::cast(draw.model_matrix);
                                    let color = draw.color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                                    manager.update_object_with_material(
                                        first_instance as usize,
                                        &model,
                                        &color,
                                        draw.metallic,
                                        draw.roughness,
                                        draw.ao,
                                    );
                                }
                            }

                            // Create texture descriptor if not already done
                            // Get image_info from material's uniform (storage mode uses material's uniform for texture info)
                            if material.pipeline.borrow().texture_descriptor.is_none() {
                                let image_info = material
                                    .uniform
                                    .as_ref()
                                    .and_then(|u| u.next_descriptor().image_info.clone());

                                if let Some(info) = image_info {
                                    let _ = material
                                        .pipeline
                                        .borrow_mut()
                                        .create_texture_descriptor_with_info(&info);
                                }
                            }

                            let pipeline_ref = material.pipeline.borrow();

                            // Bind pipeline
                            unsafe {
                                pipeline_ref.context().device.cmd_bind_pipeline(
                                    cmd_buf,
                                    vk::PipelineBindPoint::GRAPHICS,
                                    pipeline_ref.vk_pipeline().handle,
                                );
                            }

                            // Bind set 0: Storage uniforms (frame_data + objects)
                            if let Some(descriptor) = storage_descriptor.as_ref() {
                                unsafe {
                                    pipeline_ref.context().device.cmd_bind_descriptor_sets(
                                        cmd_buf,
                                        vk::PipelineBindPoint::GRAPHICS,
                                        pipeline_ref.vk_layout(),
                                        0,
                                        &[descriptor.set()],
                                        &[],
                                    );
                                }
                            }

                            // Bind set 1: Textures
                            if let Some(ref tex_descriptor) = pipeline_ref.texture_descriptor {
                                unsafe {
                                    pipeline_ref.context().device.cmd_bind_descriptor_sets(
                                        cmd_buf,
                                        vk::PipelineBindPoint::GRAPHICS,
                                        pipeline_ref.vk_layout(),
                                        1,
                                        &[tex_descriptor.set()],
                                        &[],
                                    );
                                }
                            }

                            // Bind set 2: Skeleton (for skinned meshes)
                            if let Some(skeleton_handle) = draw.skeleton {
                                let skeleton_descriptors = unsafe { &*skeleton_descriptors_ptr };
                                if let Some(Some(skeleton_desc)) = skeleton_descriptors.get(skeleton_handle.0 as usize) {
                                    unsafe {
                                        pipeline_ref.context().device.cmd_bind_descriptor_sets(
                                            cmd_buf,
                                            vk::PipelineBindPoint::GRAPHICS,
                                            pipeline_ref.vk_layout(),
                                            2,
                                            &[skeleton_desc.set()],
                                            &[],
                                        );
                                    }
                                }
                            }

                            drop(pipeline_ref);

                            // Bind vertex and index buffers and draw
                            // Use first_instance for storage buffer mode
                            // The shader accesses objects[instance_index] where instance_index = first_instance + gl_InstanceIndex
                            if let Some((index_buffer, index_type, index_count)) = index_data {
                                ctx.command_buffer
                                    .bind_index_buffer(index_buffer, 0, index_type);

                                if let Some((vertex_buffer, _)) = vertex_data {
                                    ctx.command_buffer.bind_vertex_buffers(
                                        0,
                                        &[vertex_buffer],
                                        &[0],
                                    );
                                    // draw_indexed(index_count, instance_count, first_index, vertex_offset, first_instance)
                                    ctx.command_buffer.draw_indexed(index_count, instance_count, 0, 0, first_instance);
                                }
                            } else if let Some((vertex_buffer, vertex_count)) = vertex_data {
                                ctx.command_buffer
                                    .bind_vertex_buffers(0, &[vertex_buffer], &[0]);
                                // draw_array(vertex_count, instance_count, first_vertex, first_instance)
                                ctx.command_buffer.draw_array(vertex_count, instance_count, 0, first_instance);
                            }
                        }

                        // === PARTICLE RENDERING ===
                        // Render particles as billboard quads after all geometry
                        for particle_render in &draw_list.particle_renders {
                            let cmd_buf = ctx.command_buffer.vk_command_buffer();

                            unsafe {
                                // Bind particle graphics pipeline
                                device_ptr.cmd_bind_pipeline(
                                    cmd_buf,
                                    vk::PipelineBindPoint::GRAPHICS,
                                    particle_render.pipeline,
                                );

                                // Bind set 0: Frame uniforms (storage buffer with view/proj)
                                device_ptr.cmd_bind_descriptor_sets(
                                    cmd_buf,
                                    vk::PipelineBindPoint::GRAPHICS,
                                    particle_render.pipeline_layout,
                                    0,
                                    &[particle_render.frame_descriptor_set],
                                    &[],
                                );

                                // Bind set 1: Particle buffer
                                device_ptr.cmd_bind_descriptor_sets(
                                    cmd_buf,
                                    vk::PipelineBindPoint::GRAPHICS,
                                    particle_render.pipeline_layout,
                                    1,
                                    &[particle_render.particle_descriptor_set],
                                    &[],
                                );

                                // Draw instanced quads (6 vertices per quad, one instance per particle)
                                // No vertex buffer - vertices generated in shader from vertex_id
                                device_ptr.cmd_draw(
                                    cmd_buf,
                                    6, // 6 vertices per quad (2 triangles)
                                    particle_render.particle_count, // One instance per particle
                                    0,
                                    0,
                                );
                            }
                        }
                    }
                });
        });

        // === UI PASS ===
        // Renders UI overlay to the output texture
        // When viewport exists: UI samples viewport texture and draws it in the viewport panel
        // The output texture is then copied to swapchain by present_pass
        // Get pointers for UI rendering
        let ui_data_ptr = &self.ui_data as *const RefCell<Option<UiDrawData>>;
        let ui_pipeline_ptr = &self.ui_pipeline as *const Option<Rc<RefCell<MaterialPipeline>>>;
        let ui_buffers_ptr = &self.ui_buffers as *const Vec<UIBuffers>;
        let ui_textures_ptr = &self.ui_textures as *const Option<UITextures>;
        let ui_frame_index_ptr = &self.ui_frame_index as *const std::cell::Cell<usize>;
        // UI renders to output texture (present_pass will copy to swapchain)
        let ui_target_res = output_resource.unwrap_or(swapchain_resource);

        graph_builder.add_pass("ui_pass", move |pass| {
            pass.write(Attachment::Color(ui_target_res))
                .clear_color(ui_target_res, [0.1, 0.1, 0.1, 1.0])
                .execute("ui_pass", move |ctx| {
                    // SAFETY: Pointers are valid for the renderer's lifetime
                    let ui_data_cell = unsafe { &*ui_data_ptr };
                    let ui_pipeline_opt = unsafe { &*ui_pipeline_ptr };
                    let ui_buffers = unsafe { &*ui_buffers_ptr };
                    let ui_textures = unsafe { &*ui_textures_ptr };
                    let ui_frame_index = unsafe { &*ui_frame_index_ptr };

                    let ui_data_ref = ui_data_cell.borrow();

                    if let (Some(ui_data), Some(ui_pipeline)) =
                        (ui_data_ref.as_ref(), ui_pipeline_opt.as_ref()) {
                        if ui_data.vertex_data.is_empty() || ui_data.index_data.is_empty() {
                            return;
                        }

                        let pipeline_ref = ui_pipeline.borrow();
                        let cmd_buf = ctx.command_buffer.vk_command_buffer();

                        // Get the current frame's buffers using frame index
                        let frame_idx = ui_frame_index.get();
                        let buffers = ui_buffers.get(frame_idx);

                        unsafe {
                            // Bind UI pipeline
                            pipeline_ref.context().device.cmd_bind_pipeline(
                                cmd_buf,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline_ref.vk_pipeline().handle,
                            );

                            // Bind UI texture descriptor set (set 0)
                            if let Some(textures) = ui_textures {
                                pipeline_ref.context().device.cmd_bind_descriptor_sets(
                                    cmd_buf,
                                    vk::PipelineBindPoint::GRAPHICS,
                                    pipeline_ref.vk_pipeline().layout,
                                    0,
                                    &[textures.descriptor_set],
                                    &[],
                                );
                            }

                            // Set viewport
                            let viewport = vk::Viewport {
                                x: 0.0,
                                y: 0.0,
                                width: ui_data.screen_size[0],
                                height: ui_data.screen_size[1],
                                min_depth: 0.0,
                                max_depth: 1.0,
                            };
                            pipeline_ref.context().device.cmd_set_viewport(cmd_buf, 0, &[viewport]);

                            // Set scissor
                            let scissor = vk::Rect2D {
                                offset: vk::Offset2D { x: 0, y: 0 },
                                extent: vk::Extent2D {
                                    width: ui_data.screen_size[0] as u32,
                                    height: ui_data.screen_size[1] as u32,
                                },
                            };
                            pipeline_ref.context().device.cmd_set_scissor(cmd_buf, 0, &[scissor]);

                            let vertex_size = ui_data.vertex_data.len() as u64;
                            let index_size = ui_data.index_data.len() as u64;

                            if vertex_size == 0 || index_size == 0 {
                                return;
                            }

                            // Use persistent buffers if available, otherwise fall back to temporary
                            if let Some(buffers) = buffers {
                                // Update persistent buffers
                                if !buffers.update_vertices(pipeline_ref.context(), &ui_data.vertex_data) {
                                    warn!("UI vertex data exceeds buffer capacity");
                                    return;
                                }
                                if !buffers.update_indices(pipeline_ref.context(), &ui_data.index_data) {
                                    warn!("UI index data exceeds buffer capacity");
                                    return;
                                }

                                // Bind persistent buffers
                                pipeline_ref.context().device.cmd_bind_vertex_buffers(
                                    cmd_buf,
                                    0,
                                    &[buffers.vertex_buffer],
                                    &[0],
                                );
                                pipeline_ref.context().device.cmd_bind_index_buffer(
                                    cmd_buf,
                                    buffers.index_buffer,
                                    0,
                                    vk::IndexType::UINT32,
                                );
                            } else {
                                // Fallback: create temporary staging buffers
                                let vertex_create_info = vk::BufferCreateInfo::default()
                                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                                    .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
                                    .size(vertex_size);

                                let (vertex_buffer, vertex_alloc) = pipeline_ref.context().allocate_buffer(
                                    &vertex_create_info,
                                    gpu_allocator::MemoryLocation::CpuToGpu,
                                );

                                let index_create_info = vk::BufferCreateInfo::default()
                                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                                    .usage(vk::BufferUsageFlags::INDEX_BUFFER)
                                    .size(index_size);

                                let (index_buffer, index_alloc) = pipeline_ref.context().allocate_buffer(
                                    &index_create_info,
                                    gpu_allocator::MemoryLocation::CpuToGpu,
                                );

                                // Upload vertex data
                                let vertex_ptr = pipeline_ref.context().map_buffer(&vertex_alloc);
                                std::ptr::copy_nonoverlapping(
                                    ui_data.vertex_data.as_ptr(),
                                    vertex_ptr,
                                    vertex_size as usize,
                                );

                                // Upload index data
                                let index_ptr = pipeline_ref.context().map_buffer(&index_alloc);
                                std::ptr::copy_nonoverlapping(
                                    ui_data.index_data.as_ptr(),
                                    index_ptr,
                                    index_size as usize,
                                );

                                // Bind vertex buffer
                                pipeline_ref.context().device.cmd_bind_vertex_buffers(
                                    cmd_buf,
                                    0,
                                    &[vertex_buffer],
                                    &[0],
                                );

                                // Bind index buffer
                                pipeline_ref.context().device.cmd_bind_index_buffer(
                                    cmd_buf,
                                    index_buffer,
                                    0,
                                    vk::IndexType::UINT32,
                                );

                                // Draw all indices
                                let index_count = (index_size / 4) as u32;
                                pipeline_ref.context().device.cmd_draw_indexed(
                                    cmd_buf,
                                    index_count,
                                    1,
                                    0,
                                    0,
                                    0,
                                );

                                // Cleanup staging buffers
                                pipeline_ref.context().free_buffer(vertex_buffer, vertex_alloc);
                                pipeline_ref.context().free_buffer(index_buffer, index_alloc);
                                return;
                            }

                            // Draw all indices
                            let index_count = (index_size / 4) as u32;
                            pipeline_ref.context().device.cmd_draw_indexed(
                                cmd_buf,
                                index_count,
                                1,
                                0,
                                0,
                                0,
                            );
                        }
                    }
                });
        });

        // === PRESENT PASS (conditional) ===
        // When output texture exists: blit output texture to swapchain for presentation
        // This is a transfer-only pass that handles the final copy to swapchain
        if let (Some(output_res), true) = (output_resource, has_output) {
            let output_src = output_res;
            let swapchain_dst = swapchain_resource;
            let device_for_present = self.context.device.clone();
            let output_extent_for_present = self.output_extent();

            graph_builder.add_pass("present_pass", move |pass| {
                pass.read_transfer(output_src)
                    .write_transfer(swapchain_dst)
                    .execute("present_pass", move |ctx| {
                        let cmd_buf = ctx.command_buffer.vk_command_buffer();

                        // Get images from resources
                        let (src_image, _) = ctx.get_image(output_src).expect("output image");
                        let (dst_image, _) = ctx.get_image(swapchain_dst).expect("swapchain image");

                        if let Some(extent) = output_extent_for_present {
                            let subresource_range = vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: 1,
                                base_array_layer: 0,
                                layer_count: 1,
                            };

                            // === PRE-BLIT BARRIERS ===
                            // Transition output: COLOR_ATTACHMENT_OPTIMAL -> TRANSFER_SRC_OPTIMAL
                            let src_barrier = ImageMemoryBarrier2::new(src_image)
                                .src_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
                                .src_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
                                .dst_stage(PipelineStage2Flags::TRANSFER)
                                .dst_access(AccessFlags2::TRANSFER_READ)
                                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                                .subresource_range(subresource_range);

                            // Transition swapchain: PRESENT_SRC_KHR -> TRANSFER_DST_OPTIMAL
                            let dst_barrier = ImageMemoryBarrier2::new(dst_image)
                                .src_stage(PipelineStage2Flags::BOTTOM_OF_PIPE)
                                .src_access(AccessFlags2::NONE)
                                .dst_stage(PipelineStage2Flags::TRANSFER)
                                .dst_access(AccessFlags2::TRANSFER_WRITE)
                                .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                                .subresource_range(subresource_range);

                            DependencyInfo::new()
                                .add_image_barrier(src_barrier)
                                .add_image_barrier(dst_barrier)
                                .build(|dep_info| unsafe {
                                    device_for_present.cmd_pipeline_barrier2(cmd_buf, dep_info);
                                });

                            // === BLIT ===
                            let src_subresource = vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .mip_level(0)
                                .base_array_layer(0)
                                .layer_count(1);

                            let dst_subresource = vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .mip_level(0)
                                .base_array_layer(0)
                                .layer_count(1);

                            let blit_region = vk::ImageBlit::default()
                                .src_subresource(src_subresource)
                                .src_offsets([
                                    vk::Offset3D { x: 0, y: 0, z: 0 },
                                    vk::Offset3D {
                                        x: extent.width as i32,
                                        y: extent.height as i32,
                                        z: 1,
                                    },
                                ])
                                .dst_subresource(dst_subresource)
                                .dst_offsets([
                                    vk::Offset3D { x: 0, y: 0, z: 0 },
                                    vk::Offset3D {
                                        x: extent.width as i32,
                                        y: extent.height as i32,
                                        z: 1,
                                    },
                                ]);

                            unsafe {
                                device_for_present.cmd_blit_image(
                                    cmd_buf,
                                    src_image,
                                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                                    dst_image,
                                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                                    &[blit_region],
                                    vk::Filter::LINEAR,
                                );
                            }

                            // === POST-BLIT BARRIERS ===
                            // Transition output: TRANSFER_SRC_OPTIMAL -> COLOR_ATTACHMENT_OPTIMAL (for next frame)
                            let src_barrier_back = ImageMemoryBarrier2::new(src_image)
                                .src_stage(PipelineStage2Flags::TRANSFER)
                                .src_access(AccessFlags2::TRANSFER_READ)
                                .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
                                .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
                                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                                .subresource_range(subresource_range);

                            // Transition swapchain: TRANSFER_DST_OPTIMAL -> PRESENT_SRC_KHR
                            // Note: The render graph will handle the final transition for the last pass
                            // But since this is a transfer pass, we need to do it manually
                            let dst_barrier_back = ImageMemoryBarrier2::new(dst_image)
                                .src_stage(PipelineStage2Flags::TRANSFER)
                                .src_access(AccessFlags2::TRANSFER_WRITE)
                                .dst_stage(PipelineStage2Flags::BOTTOM_OF_PIPE)
                                .dst_access(AccessFlags2::NONE)
                                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                                .subresource_range(subresource_range);

                            DependencyInfo::new()
                                .add_image_barrier(src_barrier_back)
                                .add_image_barrier(dst_barrier_back)
                                .build(|dep_info| unsafe {
                                    device_for_present.cmd_pipeline_barrier2(cmd_buf, dep_info);
                                });
                        }
                    });
            });
        }

        let vulkan_context = self.context.clone();
        match graph_builder.build(&vulkan_context) {
            Ok(mut graph) => {
                // Store the draw_list_cell so we can update it each frame
                graph.set_draw_list_cell(draw_list_cell);

                // Create framebuffers for each swapchain image using the immediate-mode render pass
                let swapchain_images: Vec<_> = self
                    .frame_context
                    .swapchain_images
                    .iter()
                    .zip(self.frame_context.swapchain_image_views.iter())
                    .map(|(image, view)| {
                        let extent = self.frame_context.swapchain.get_extent();
                        (
                            *image,
                            *view,
                            crate::render_graph::types::Extent2D::new(extent.width, extent.height),
                            self.frame_context.swapchain.format.format,
                        )
                    })
                    .collect();

                // Create framebuffers for swapchain images (uses null render pass for dynamic rendering)
                if let Err(e) = graph.create_swapchain_framebuffers(&swapchain_images)
                {
                    error!("Failed to create swapchain framebuffers: {:?}", e);
                } else {
                    // Initialize color_attachments and depth_attachments for all swapchain images
                    // Only update passes that render to swapchain (copy_pass, ui_pass)
                    // sky_pass and geometry_pass render to viewport texture (already set during compilation)
                    let swapchain_pass_indices: Vec<usize> = graph.passes.iter().enumerate()
                        .filter(|(_, pass)| pass.name == "copy_pass" || pass.name == "ui_pass")
                        .map(|(idx, _)| idx)
                        .collect();

                    for (image_index, (_vk_image, image_view, _extent, _format)) in
                        swapchain_images.iter().enumerate()
                    {
                        for &pass_idx in &swapchain_pass_indices {
                            // Ensure color_attachments array has an entry for this image index
                            while graph.passes[pass_idx].color_attachments.len() <= image_index {
                                graph.passes[pass_idx].color_attachments.push(vec![]);
                            }

                            // Update the color attachments for dynamic rendering (swapchain)
                            graph.passes[pass_idx].color_attachments[image_index] = vec![image_view.vk()];
                        }
                    }

                    // Set viewport resource IDs for double-buffering updates
                    if let (Some(color_id), Some(depth_id)) = (viewport_resource, viewport_depth_resource) {
                        graph.set_viewport_resource_ids(color_id, depth_id);
                    }

                    self.render_graph = Some(graph);
                }
            }
            Err(e) => {
                error!("Failed to compile render graph: {:?}", e);
            }
        }
    }

    /// Set the sky pipeline for procedural sky rendering.
    ///
    /// This must be called before setup_render_graph() if sky rendering is desired.
    /// The sky pipeline renders a fullscreen triangle with a procedural sky shader.
    pub fn set_sky_pipeline(&mut self, pipeline: Rc<RefCell<MaterialPipeline>>) {
        self.sky_pipeline = Some(pipeline);
    }

    /// Set the UI overlay pipeline.
    ///
    /// Call this before setup_render_graph() to enable UI rendering.
    pub fn set_ui_pipeline(&mut self, pipeline: Rc<RefCell<MaterialPipeline>>) {
        self.ui_pipeline = Some(pipeline);
    }

    pub fn render_frame(&mut self, draw_list: DrawList) -> Result<(), RenderGraphError> {
        // Acquire swapchain image
        if self.current_framedata.is_none() {
            match self.swap_frames() {
                Ok(()) => {}
                Err(RenderGraphError::SwapchainOutOfDate) => {
                    // Swapchain is out of date, recreate it and try again next frame
                    // Don't pass explicit extent here - let surface capabilities determine it
                    self.recreate_swapchain();
                    return Err(RenderGraphError::SwapchainOutOfDate);
                }
                Err(e) => return Err(e),
            }
        }

        let frame_data = self
            .current_framedata
            .as_ref()
            .ok_or(RenderGraphError::NoFrameData)?;

        let image_index = frame_data.image_index as usize;

        // Now we can safely borrow the graph
        let graph = self
            .render_graph
            .as_mut()
            .ok_or(RenderGraphError::CompilationError("No render graph".into()))?;

        // === UPDATE FRAME UNIFORMS BEFORE RENDER GRAPH EXECUTES ===
        // This must happen before any passes run (sky pass needs inv_view_proj)
        if let Some(ref frame) = self.frame_uniforms {
            if let Some(ref mut manager) = self.storage_manager {
                // Safe cast using bytemuck (both types are Pod with same layout)
                let view: [[f32; 4]; 4] = bytemuck::cast(frame.view_matrix);
                let proj: [[f32; 4]; 4] = bytemuck::cast(frame.proj_matrix);
                let inv_view_proj: [[f32; 4]; 4] = bytemuck::cast(frame.inv_view_proj_matrix);

                manager.update_frame_with_lighting(
                    &view,
                    &proj,
                    &inv_view_proj,
                    &frame.camera_position,
                    &frame.light_direction,
                    &frame.light_color,
                    frame.light_intensity,
                );
            }
        }

        // Set the draw list for this frame
        graph.set_draw_list(draw_list.clone());

        let mut command_buffer = self.frame_context.command_buffers[image_index].clone();
        command_buffer.begin_command(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        // Set frame index for UI buffer selection
        // Use swap_data.current_frame() which aligns with fence synchronization, NOT image_index
        // This ensures we don't reuse a buffer that's still being used by the GPU
        let frame_idx = self.swap_data.current_frame();
        self.ui_frame_index.set(frame_idx);

        // === DISPATCH PARTICLE COMPUTE SHADERS BEFORE RENDER GRAPH ===
        // This runs particle simulation on GPU before any rendering
        if !draw_list.particle_dispatches.is_empty() {
            for particle in &draw_list.particle_dispatches {
                // Bind compute pipeline
                unsafe {
                    self.context.device.cmd_bind_pipeline(
                        command_buffer.vk_command_buffer(),
                        vk::PipelineBindPoint::COMPUTE,
                        particle.pipeline,
                    );

                    // Bind descriptor set
                    self.context.device.cmd_bind_descriptor_sets(
                        command_buffer.vk_command_buffer(),
                        vk::PipelineBindPoint::COMPUTE,
                        particle.pipeline_layout,
                        0,
                        &[particle.descriptor_set],
                        &[],
                    );

                    // Push frame data constants
                    self.context.device.cmd_push_constants(
                        command_buffer.vk_command_buffer(),
                        particle.pipeline_layout,
                        vk::ShaderStageFlags::COMPUTE,
                        0,
                        bytemuck::cast_slice(&particle.frame_data),
                    );

                    // Dispatch compute workgroups
                    self.context.device.cmd_dispatch(
                        command_buffer.vk_command_buffer(),
                        particle.workgroup_count,
                        1,
                        1,
                    );
                }
            }

            // Barrier: compute write -> vertex read for rendering
            let memory_barriers = [vk::MemoryBarrier2KHR::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::VERTEX_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_READ)];

            let dep_info = vk::DependencyInfoKHR::default().memory_barriers(&memory_barriers);

            unsafe {
                self.context
                    .device
                    .cmd_pipeline_barrier2(command_buffer.vk_command_buffer(), &dep_info);
            }
        }

        // Execute the render graph with the current image index
        // The render graph handles:
        // 1. Rendering 3D scene to viewport texture (geometry_pass, sky_pass, particle_pass)
        // 2. Copying viewport texture to swapchain (copy_pass, when viewport exists)
        // 3. Rendering UI overlay on swapchain (ui_pass)
        graph.execute(
            &mut command_buffer,
            image_index,
            &self.frame_context.swapchain_images,
            self.frame_context.depth_render_texture.image,
        )?;

        command_buffer.end_command();

        let frame_data = self.current_framedata.take().unwrap();
        let wait_semaphores = vec![frame_data.available_sem.vk()];
        let wait_dst_stage_mask = vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores = vec![frame_data.finished_sem.vk()];
        let in_flight_fence = frame_data.in_flight_fence.vk();

        unsafe {
            self.context
                .device
                .reset_fences(&[in_flight_fence])
                .unwrap();
        }

        // Only submit the command buffer for the current frame index
        let command_buffer = &self.frame_context.command_buffers[frame_data.image_index as usize];
        let vk_command_buffers = vec![command_buffer.vk_command_buffer()];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_dst_stage_mask)
            .signal_semaphores(&signal_semaphores)
            .command_buffers(&vk_command_buffers);

        unsafe {
            self.context
                .device
                .queue_submit(self.context.graphics_queue, &[submit_info], in_flight_fence)
                .map_err(RenderGraphError::VulkanError)?;
        }

        let swapchains = vec![self.frame_context.swapchain.swapchain];
        let image_indices = vec![frame_data.image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let present_result = unsafe {
            self.context
                .swapchain_loader
                .as_ref()
                .expect("Swapchain loader required")
                .queue_present(self.context.graphics_queue, &present_info)
        };

        // Check if presentation failed due to out-of-date swapchain
        if let Err(e) = present_result {
            if e == vk::Result::ERROR_OUT_OF_DATE_KHR || e == vk::Result::SUBOPTIMAL_KHR {
                // Swapchain is out of date, recreate it and try again next frame
                // Don't pass explicit extent here - let surface capabilities determine it
                self.recreate_swapchain();
                return Err(RenderGraphError::SwapchainOutOfDate);
            } else {
                return Err(RenderGraphError::VulkanError(e));
            }
        }

        self.swap_data.step_frame();

        Ok(())
    }

    /// Set UI overlay data for rendering.
    ///
    /// Call this each frame before render_frame() to provide UI vertex/index data.
    /// The UI will be rendered after the geometry pass with alpha blending.
    ///
    /// # Arguments
    /// * `vertex_data` - Raw vertex data (position[2], uv[2], color[4] per vertex)
    /// * `index_data` - Index data as raw bytes (u32 indices)
    /// * `screen_size` - Screen dimensions in pixels
    pub fn set_ui_data(&self, vertex_data: Vec<u8>, index_data: Vec<u8>, screen_size: [f32; 2]) {
        *self.ui_data.borrow_mut() = Some(UiDrawData {
            vertex_data,
            index_data,
            screen_size,
            index_count: 0, // Will be calculated
        });
    }
}

/// UI draw data for rendering.
pub struct UiDrawData {
    pub vertex_data: Vec<u8>,
    pub index_data: Vec<u8>,
    pub screen_size: [f32; 2],
    pub index_count: u32,
}

/// Persistent buffers for UI rendering.
/// One set per frame in flight to avoid synchronization issues.
pub struct UIBuffers {
    /// Vertex buffer (CpuToGpu for easy updates).
    pub vertex_buffer: vk::Buffer,
    pub vertex_allocation: gpu_allocator::vulkan::Allocation,
    pub vertex_capacity: u64,
    /// Index buffer (CpuToGpu for easy updates).
    pub index_buffer: vk::Buffer,
    pub index_allocation: gpu_allocator::vulkan::Allocation,
    pub index_capacity: u64,
}

impl UIBuffers {
    /// Create new UI buffers with the given capacities.
    pub fn new(context: &VulkanContext, vertex_capacity: u64, index_capacity: u64) -> Self {
        let vertex_create_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER)
            .size(vertex_capacity);

        let (vertex_buffer, vertex_allocation) = context.allocate_buffer(
            &vertex_create_info,
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let index_create_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::INDEX_BUFFER)
            .size(index_capacity);

        let (index_buffer, index_allocation) = context.allocate_buffer(
            &index_create_info,
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        Self {
            vertex_buffer,
            vertex_allocation,
            vertex_capacity,
            index_buffer,
            index_allocation,
            index_capacity,
        }
    }

    /// Update vertex data. Returns true if data fits.
    pub fn update_vertices(&self, context: &VulkanContext, data: &[u8]) -> bool {
        if data.len() as u64 > self.vertex_capacity {
            return false;
        }
        let ptr = context.map_buffer(&self.vertex_allocation);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        }
        true
    }

    /// Update index data. Returns true if data fits.
    pub fn update_indices(&self, context: &VulkanContext, data: &[u8]) -> bool {
        if data.len() as u64 > self.index_capacity {
            return false;
        }
        let ptr = context.map_buffer(&self.index_allocation);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        }
        true
    }

    /// Destroy buffers (consumes self).
    pub fn destroy(self, context: &VulkanContext) {
        context.free_buffer(self.vertex_buffer, self.vertex_allocation);
        context.free_buffer(self.index_buffer, self.index_allocation);
    }
}

/// UI texture resources for font atlas and fallback.
pub struct UITextures {
    /// Font atlas texture image.
    pub font_image: vk::Image,
    pub font_image_memory: gpu_allocator::vulkan::Allocation,
    pub font_image_view: vk::ImageView,
    /// White 1x1 texture for solid color elements.
    pub white_image: vk::Image,
    pub white_image_memory: gpu_allocator::vulkan::Allocation,
    pub white_image_view: vk::ImageView,
    /// Sampler for textures.
    pub sampler: vk::Sampler,
    /// Descriptor set layout for UI textures.
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    /// Descriptor set with bound textures.
    pub descriptor_set: vk::DescriptorSet,
    /// Descriptor pool for UI textures.
    pub descriptor_pool: vk::DescriptorPool,
    /// Font atlas dimensions.
    pub atlas_width: u32,
    pub atlas_height: u32,
    /// Viewport texture image view (updated externally).
    pub viewport_image_view: Option<vk::ImageView>,
}

impl UITextures {
    /// Create UI textures with a default font atlas size.
    pub fn new(context: &VulkanContext) -> Result<Self, vk::Result> {
        Self::with_atlas_size(context, 512, 512)
    }

    /// Create UI textures with a specific font atlas size.
    pub fn with_atlas_size(
        context: &VulkanContext,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<Self, vk::Result> {
        unsafe {
            // Create sampler for UI textures (linear filtering, no mipmaps)
            let sampler_create_info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .max_anisotropy(1.0)
                .border_color(vk::BorderColor::INT_TRANSPARENT_BLACK)
                .unnormalized_coordinates(false)
                .compare_enable(false)
                .compare_op(vk::CompareOp::ALWAYS)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(0.0);

            let sampler = context.device.create_sampler(&sampler_create_info, None)?;

            // Create white 1x1 texture
            let (white_image, white_memory, white_view) =
                Self::create_texture(context, 1, 1, &[255, 255, 255, 255])?;

            // Create font atlas texture (initially white)
            let white_pixels = vec![255u8; (atlas_width * atlas_height * 4) as usize];
            let (font_image, font_memory, font_view) =
                Self::create_texture(context, atlas_width, atlas_height, &white_pixels)?;

            // Create descriptor set layout (3 bindings: font atlas, sampler, viewport texture)
            let bindings = [
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            ];

            let layout_create_info = vk::DescriptorSetLayoutCreateInfo::default()
                .bindings(&bindings);

            let descriptor_set_layout = context.device.create_descriptor_set_layout(&layout_create_info, None)?;

            // Create descriptor pool (2 sampled images + 1 sampler)
            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLED_IMAGE,
                    descriptor_count: 2,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLER,
                    descriptor_count: 1,
                },
            ];

            let pool_create_info = vk::DescriptorPoolCreateInfo::default()
                .pool_sizes(&pool_sizes)
                .max_sets(1);

            let descriptor_pool = context.device.create_descriptor_pool(&pool_create_info, None)?;

            // Allocate descriptor set
            let set_layouts = [descriptor_set_layout];
            let alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&set_layouts);

            let descriptor_sets = context.device.allocate_descriptor_sets(&alloc_info)?;
            let descriptor_set = descriptor_sets[0];

            // Update descriptor set with font texture, sampler, and placeholder viewport texture
            let font_image_info = vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: font_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            };

            let sampler_info = vk::DescriptorImageInfo {
                sampler,
                image_view: vk::ImageView::null(),
                image_layout: vk::ImageLayout::UNDEFINED,
            };

            // Viewport texture placeholder (use white texture initially)
            let viewport_image_info = vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: white_view, // Placeholder - will be updated when viewport is created
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            };

            let font_infos = [font_image_info];
            let sampler_infos = [sampler_info];
            let viewport_infos = [viewport_image_info];

            let font_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&font_infos);

            let sampler_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&sampler_infos);

            let viewport_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&viewport_infos);

            context.device.update_descriptor_sets(&[font_write, sampler_write, viewport_write], &[]);

            Ok(Self {
                font_image,
                font_image_memory: font_memory,
                font_image_view: font_view,
                white_image,
                white_image_memory: white_memory,
                white_image_view: white_view,
                sampler,
                descriptor_set_layout,
                descriptor_set,
                descriptor_pool,
                atlas_width,
                atlas_height,
                viewport_image_view: None,
            })
        }
    }

    /// Create a simple RGBA8 texture from pixel data.
    unsafe fn create_texture(
        context: &VulkanContext,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(vk::Image, gpu_allocator::vulkan::Allocation, vk::ImageView), vk::Result> {
        let extent = vk::Extent3D { width, height, depth: 1 };

        // Create image
        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(vk::Format::R8G8B8A8_SRGB)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1);

        let (image, memory) = context.create_image(image_create_info, gpu_allocator::MemoryLocation::GpuOnly);

        // Create staging buffer
        let staging_create_info = vk::BufferCreateInfo::default()
            .size(pixels.len() as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (staging_buffer, staging_memory) =
            context.allocate_buffer(&staging_create_info, gpu_allocator::MemoryLocation::CpuToGpu);

        // Copy pixels to staging buffer
        let staging_ptr = context.map_buffer(&staging_memory);
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), staging_ptr, pixels.len());

        // Transition and copy
        let cmd_buffer = context.begin_single_time_commands();
        let cmd = cmd_buffer.vk_command_buffer();

        // Transition to transfer dst
        let subresource = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);

        let barrier_undefined_to_dst = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(subresource)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

        context.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier_undefined_to_dst],
        );

        // Copy buffer to image
        let region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(extent);

        context.device.cmd_copy_buffer_to_image(
            cmd,
            staging_buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );

        // Transition to shader read only
        let barrier_dst_to_shader = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(subresource)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ);

        context.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier_dst_to_shader],
        );

        context.end_single_time_commands(cmd_buffer);

        // Cleanup staging buffer
        context.free_buffer(staging_buffer, staging_memory);

        // Create image view
        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_view = context.device.create_image_view(&view_create_info, None)?;

        Ok((image, memory, image_view))
    }

    /// Update font atlas texture with new pixel data.
    pub fn update_font_atlas(&mut self, context: &VulkanContext, pixels: &[u8]) -> bool {
        if pixels.len() != (self.atlas_width * self.atlas_height * 4) as usize {
            warn!("Font atlas pixel data size mismatch");
            return false;
        }

        unsafe {
            // Create staging buffer
            let staging_create_info = vk::BufferCreateInfo::default()
                .size(pixels.len() as u64)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let (staging_buffer, staging_memory) =
                context.allocate_buffer(&staging_create_info, gpu_allocator::MemoryLocation::CpuToGpu);

            // Copy pixels to staging buffer
            let staging_ptr = context.map_buffer(&staging_memory);
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), staging_ptr, pixels.len());

            // Transition and copy
            let cmd_buffer = context.begin_single_time_commands();
            let cmd = cmd_buffer.vk_command_buffer();

            let extent = vk::Extent3D {
                width: self.atlas_width,
                height: self.atlas_height,
                depth: 1,
            };

            let subresource = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);

            // Transition to transfer dst
            let barrier_to_dst = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.font_image)
                .subresource_range(subresource)
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_to_dst],
            );

            // Copy buffer to image
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(extent);

            context.device.cmd_copy_buffer_to_image(
                cmd,
                staging_buffer,
                self.font_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );

            // Transition back to shader read only
            let barrier_to_shader = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.font_image)
                .subresource_range(subresource)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_to_shader],
            );

            context.end_single_time_commands(cmd_buffer);

            // Cleanup staging buffer
            context.free_buffer(staging_buffer, staging_memory);
        }

        true
    }

    /// Update viewport texture binding in the descriptor set.
    /// Call this when the viewport render target is created or resized.
    pub fn set_viewport_texture(&mut self, context: &VulkanContext, image_view: vk::ImageView) {
        unsafe {
            let viewport_image_info = vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            };

            let viewport_infos = [viewport_image_info];

            let viewport_write = vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set)
                .dst_binding(2)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&viewport_infos);

            context.device.update_descriptor_sets(&[viewport_write], &[]);
        }
        self.viewport_image_view = Some(image_view);
    }

    /// Destroy UI textures.
    pub fn destroy(self, context: &VulkanContext) {
        unsafe {
            context.device.destroy_sampler(self.sampler, None);
            context.device.destroy_image_view(self.font_image_view, None);
            context.device.destroy_image(self.font_image, None);
            context.allocator.borrow_mut().free(self.font_image_memory).ok();
            context.device.destroy_image_view(self.white_image_view, None);
            context.device.destroy_image(self.white_image, None);
            context.allocator.borrow_mut().free(self.white_image_memory).ok();
            context.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            context.device.destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

/// Offscreen render target for viewport rendering.
///
/// This holds the color and depth attachments for rendering the 3D scene
/// to a texture that can be sampled by the UI viewport panel.
pub struct ViewportRenderTarget {
    /// Color attachment image.
    pub color_image: vk::Image,
    pub color_memory: gpu_allocator::vulkan::Allocation,
    pub color_image_view: vk::ImageView,
    /// Depth attachment image.
    pub depth_image: vk::Image,
    pub depth_memory: gpu_allocator::vulkan::Allocation,
    pub depth_image_view: vk::ImageView,
    /// Render extent.
    pub extent: vk::Extent2D,
    /// Sampler for sampling the color texture.
    pub sampler: vk::Sampler,
}

impl ViewportRenderTarget {
    /// Create a new viewport render target with the given dimensions.
    pub fn new(context: &VulkanContext, width: u32, height: u32) -> Result<Self, vk::Result> {
        unsafe {
            let extent = vk::Extent2D { width, height };
            let extent3d = vk::Extent3D { width, height, depth: 1 };

            // Create color image (RGBA8, can be sampled and used as color attachment)
            let color_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::R8G8B8A8_SRGB)
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .samples(vk::SampleCountFlags::TYPE_1);

            let (color_image, color_memory) = context.create_image(color_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create color image view
            let color_view_create_info = vk::ImageViewCreateInfo::default()
                .image(color_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_SRGB)
                .components(vk::ComponentMapping::default())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let color_image_view = context.device.create_image_view(&color_view_create_info, None)?;

            // Create depth image (D32_SFLOAT)
            let depth_create_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .extent(extent3d)
                .mip_levels(1)
                .array_layers(1)
                .format(vk::Format::D32_SFLOAT)
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .samples(vk::SampleCountFlags::TYPE_1);

            let (depth_image, depth_memory) = context.create_image(depth_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create depth image view
            let depth_view_create_info = vk::ImageViewCreateInfo::default()
                .image(depth_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::D32_SFLOAT)
                .components(vk::ComponentMapping::default())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::DEPTH,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let depth_image_view = context.device.create_image_view(&depth_view_create_info, None)?;

            // Create sampler
            let sampler_create_info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .max_anisotropy(1.0)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .compare_enable(false)
                .compare_op(vk::CompareOp::ALWAYS)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(0.0);

            let sampler = context.device.create_sampler(&sampler_create_info, None)?;

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
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            // Transition depth to depth stencil attachment optimal
            let depth_barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(depth_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::DEPTH,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);

            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[color_barrier, depth_barrier],
            );

            context.end_single_time_commands(cmd_buffer);

            Ok(Self {
                color_image,
                color_memory,
                color_image_view,
                depth_image,
                depth_memory,
                depth_image_view,
                extent,
                sampler,
            })
        }
    }

    /// Resize the render target by recreating it.
    pub fn resize(context: &VulkanContext, old: Option<Self>, width: u32, height: u32) -> Result<Self, vk::Result> {
        // Destroy old resources if any
        if let Some(old_target) = old {
            old_target.destroy(context);
        }

        // Create new resources
        Self::new(context, width, height)
    }

    /// Destroy the render target.
    pub fn destroy(self, context: &VulkanContext) {
        unsafe {
            context.device.destroy_sampler(self.sampler, None);
            context.device.destroy_image_view(self.color_image_view, None);
            context.device.destroy_image(self.color_image, None);
            context.allocator.borrow_mut().free(self.color_memory).ok();
            context.device.destroy_image_view(self.depth_image_view, None);
            context.device.destroy_image(self.depth_image, None);
            context.allocator.borrow_mut().free(self.depth_memory).ok();
        }
    }
}

/// Output render target for final UI composition.
/// The UI renders to this texture, then present_pass blits it to the swapchain.
/// This decouples rendering from presentation for a cleaner architecture.
pub struct OutputRenderTarget {
    /// Color attachment image.
    pub color_image: vk::Image,
    pub color_memory: gpu_allocator::vulkan::Allocation,
    pub color_image_view: vk::ImageView,
    /// Render extent (matches swapchain size).
    pub extent: vk::Extent2D,
}

impl OutputRenderTarget {
    /// Create a new output render target with the given dimensions.
    pub fn new(context: &VulkanContext, width: u32, height: u32) -> Result<Self, vk::Result> {
        unsafe {
            let extent = vk::Extent2D { width, height };
            let extent3d = vk::Extent3D { width, height, depth: 1 };

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

            let (color_image, color_memory) = context.create_image(color_create_info, gpu_allocator::MemoryLocation::GpuOnly);

            // Create color image view
            let color_view_create_info = vk::ImageViewCreateInfo::default()
                .image(color_image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::B8G8R8A8_SRGB)
                .components(vk::ComponentMapping::default())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let color_image_view = context.device.create_image_view(&color_view_create_info, None)?;

            // Transition image to COLOR_ATTACHMENT_OPTIMAL (ready for UI rendering)
            let cmd_buffer = context.begin_single_time_commands();
            let cmd = cmd_buffer.vk_command_buffer();

            let color_barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(color_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
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
                color_memory,
                color_image_view,
                extent,
            })
        }
    }

    /// Resize the render target by recreating it.
    pub fn resize(context: &VulkanContext, old: Option<Self>, width: u32, height: u32) -> Result<Self, vk::Result> {
        if let Some(old_target) = old {
            old_target.destroy(context);
        }
        Self::new(context, width, height)
    }

    /// Destroy the render target.
    pub fn destroy(self, context: &VulkanContext) {
        unsafe {
            context.device.destroy_image_view(self.color_image_view, None);
            context.device.destroy_image(self.color_image, None);
            context.allocator.borrow_mut().free(self.color_memory).ok();
        }
    }
}
