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
    types::{DrawCall, DrawList, FrameUniforms, InstanceData, MaterialHandle, MeshHandle, SkeletonHandle},
};
pub use sync::{
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

    pub fn destroy(&mut self) {
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
            let new_depth_view = self.frame_context.depth_render_texture.image_view.vk();

            // Recreate framebuffers with new swapchain images
            for (image_index, (_vk_image, image_view, _extent, _format)) in
                swapchain_images.iter().enumerate()
            {
                for pass_idx in 0..graph.passes.len() {
                    // Ensure color_attachments array has an entry for this image index
                    while graph.passes[pass_idx].color_attachments.len() <= image_index {
                        graph.passes[pass_idx].color_attachments.push(vec![]);
                    }

                    // Update the color attachments for dynamic rendering
                    graph.passes[pass_idx].color_attachments[image_index] = vec![image_view.vk()];

                    // Ensure depth_attachments array has an entry for this image index
                    while graph.passes[pass_idx].depth_attachments.len() <= image_index {
                        graph.passes[pass_idx].depth_attachments.push(None);
                    }

                    // Update the depth attachments for dynamic rendering
                    graph.passes[pass_idx].depth_attachments[image_index] = Some(new_depth_view);

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

        let depth_resource = self.create_depth_resource(&mut graph_builder);

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

        let swapchain_res = swapchain_resource;
        let depth_res = depth_resource;

        // === SKY PASS ===
        // Renders first, clears color and depth, writes sky to color only
        graph_builder.add_pass("sky_pass", move |pass| {
            pass.write(Attachment::Color(swapchain_res))
                .write(Attachment::DepthStencil(depth_res))
                .clear_color(swapchain_res, [0.4, 0.6, 0.9, 1.0]) // Sky blue fallback
                .clear_depth_stencil(depth_res, 1.0, 0)
                .execute("sky_pass", move |ctx| {
                    // SAFETY: The pointers are valid for the entire lifetime of the renderer
                    let sky_pipeline_opt = unsafe { &mut *sky_pipeline_ptr };
                    let storage_descriptor_opt = unsafe { &mut *storage_descriptor_ptr };

                    if let (Some(sky_pipeline), Some(storage_descriptor)) =
                        (sky_pipeline_opt.as_ref(), storage_descriptor_opt.as_ref())
                    {
                        let cmd_buf = ctx.command_buffer.vk_command_buffer();
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
        graph_builder.add_pass("geometry_pass", move |pass| {
            pass.write(Attachment::Color(swapchain_res))
                .write(Attachment::DepthStencil(depth_res))
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
                    }
                });
        });

        // === UI PASS ===
        // Renders UI overlay after all geometry, with alpha blending
        // Get pointers for UI rendering
        let ui_data_ptr = &self.ui_data as *const RefCell<Option<UiDrawData>>;
        let ui_pipeline_ptr = &self.ui_pipeline as *const Option<Rc<RefCell<MaterialPipeline>>>;

        graph_builder.add_pass("ui_pass", move |pass| {
            pass.write(Attachment::Color(swapchain_res))
                // Load existing color (don't clear), no depth needed
                .execute("ui_pass", move |ctx| {
                    // SAFETY: Pointers are valid for the renderer's lifetime
                    let ui_data_cell = unsafe { &*ui_data_ptr };
                    let ui_pipeline_opt = unsafe { &*ui_pipeline_ptr };

                    let ui_data_ref = ui_data_cell.borrow();

                    if let (Some(ui_data), Some(ui_pipeline)) =
                        (ui_data_ref.as_ref(), ui_pipeline_opt.as_ref()) {
                        if ui_data.vertex_data.is_empty() || ui_data.index_data.is_empty() {
                            return;
                        }

                        let pipeline_ref = ui_pipeline.borrow();
                        let cmd_buf = ctx.command_buffer.vk_command_buffer();

                        unsafe {
                            // Bind UI pipeline
                            pipeline_ref.context().device.cmd_bind_pipeline(
                                cmd_buf,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline_ref.vk_pipeline().handle,
                            );

                            // Push screen size constant
                            let screen_size_bytes: [u8; 8] = std::mem::transmute(ui_data.screen_size);
                            pipeline_ref.context().device.cmd_push_constants(
                                cmd_buf,
                                pipeline_ref.vk_layout(),
                                vk::ShaderStageFlags::VERTEX,
                                0,
                                &screen_size_bytes,
                            );

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

                            // Create temporary vertex/index buffers
                            let vertex_size = ui_data.vertex_data.len() as u64;
                            let index_size = ui_data.index_data.len() as u64;

                            if vertex_size == 0 || index_size == 0 {
                                return;
                            }

                            // Create staging buffers
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
                            let index_count = (index_size / 4) as u32; // u32 indices
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
                        }
                    }
                });
        });

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
                    let new_depth_view = self.frame_context.depth_render_texture.image_view.vk();

                    for (image_index, (_vk_image, image_view, _extent, _format)) in
                        swapchain_images.iter().enumerate()
                    {
                        for pass_idx in 0..graph.passes.len() {
                            // Ensure color_attachments array has an entry for this image index
                            while graph.passes[pass_idx].color_attachments.len() <= image_index {
                                graph.passes[pass_idx].color_attachments.push(vec![]);
                            }

                            // Update the color attachments for dynamic rendering
                            graph.passes[pass_idx].color_attachments[image_index] = vec![image_view.vk()];

                            // Ensure depth_attachments array has an entry for this image index
                            while graph.passes[pass_idx].depth_attachments.len() <= image_index {
                                graph.passes[pass_idx].depth_attachments.push(None);
                            }

                            // Update the depth attachments for dynamic rendering
                            graph.passes[pass_idx].depth_attachments[image_index] = Some(new_depth_view);
                        }
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
        graph.set_draw_list(draw_list);

        let mut command_buffer = self.frame_context.command_buffers[image_index].clone();
        command_buffer.begin_command(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        // Execute the render graph with the current image index
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
