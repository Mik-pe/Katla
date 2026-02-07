pub mod render_graph;
pub mod rendering;
pub mod sync;
pub mod vulkan;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
pub use render_graph::errors::RenderGraphError;
pub use render_graph::pass::{PassBuilder, PassExecutionContext};
pub use render_graph::resource::{
    CompiledResource, ResourceAccessType, ResourceId, ResourceKind, ResourceLifetime, ResourceUsage,
};
pub use render_graph::*;
pub use rendering::{
    registry::AssetRegistry, types::{DrawCall, DrawList, MaterialHandle, MaterialParams, MeshHandle},
};
pub use sync::{
    VkDescriptorPool, VkDescriptorSet, VkDescriptorSetLayout, VkFence, VkFramebuffer, VkImage, VkImageView,
    VkRenderPass, VkSampler, VkSemaphore,
};
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
    pub render_pass: RenderPass,
    pub swap_data: SwapData,
    pub current_framedata: Option<FrameData>,
    /// Asset registry for managing GPU resources (meshes, materials).
    /// This stores the actual Vulkan buffers and pipelines, while the application
    /// only holds opaque handles (MeshHandle, MaterialHandle).
    pub asset_registry: AssetRegistry,
    /// The render graph - single graph with multiple framebuffers (one per swapchain image)
    pub render_graph: Option<CompiledRenderGraph>,
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

        let color_format = frame_context.swapchain.format.format;
        let depth_format = frame_context.depth_render_texture.format;
        let render_pass =
            RenderPass::create_opaque(context.device.clone(), color_format, depth_format);

        let swap_data = SwapData::new(
            &context.device,
            &frame_context.swapchain_images,
            FRAMES_IN_FLIGHT,
        );

        Self {
            context,
            frame_context,
            render_pass,
            swap_data,
            current_framedata: None,
            asset_registry: AssetRegistry::new(),
            render_graph: None,
        }
    }

    pub fn destroy(&mut self) {
        // Destroy all registered assets first (materials, meshes)
        self.asset_registry.destroy();

        self.context.pre_destroy();
        self.swap_data.destroy(&self.context.device);
        self.render_pass.destroy();
        self.frame_context.destroy();
        println!("Clean shutdown!");
    }

    pub fn wait_for_device(&self) {
        unsafe {
            self.context.device.device_wait_idle().unwrap();
        }
    }

    pub fn recreate_swapchain(&mut self) {
        self.wait_for_device();

        let old_extent = self.frame_context.swapchain.get_extent();
        println!("=== Recreating swapchain ===");
        println!("  Old extent: {}x{}", old_extent.width, old_extent.height);

        self.frame_context.recreate_swapchain();

        let new_extent = self.frame_context.swapchain.get_extent();
        println!("  New extent: {}x{}", new_extent.width, new_extent.height);

        // Destroy the previous render pass
        self.render_pass.destroy();

        let color_format = self.frame_context.swapchain.format.format;
        let depth_format = self.frame_context.depth_render_texture.format;
        self.render_pass =
            RenderPass::create_opaque(self.context.device.clone(), color_format, depth_format);

        // Update render graph's active render pass if it exists
        // Collect swapchain data first to avoid borrow checker issues
        let swapchain_images: Vec<_> = self
            .frame_context
            .swapchain_images
            .iter()
            .zip(self.frame_context.swapchain_image_views.iter())
            .map(|(&image, &view)| {
                (
                    image,
                    view,
                    self.frame_context.swapchain.get_extent(),
                    self.frame_context.swapchain.format.format,
                )
            })
            .collect();

        if let Some(ref mut graph) = self.render_graph {
            let new_render_pass = self.render_pass.get_vk_renderpass();
            let new_extent = self.frame_context.swapchain.get_extent();
            for pass in &mut graph.passes {
                pass.active_render_pass = new_render_pass;
                pass.extent = new_extent;
            }

            // Destroy old framebuffers
            for pass in &graph.passes {
                for framebuffer in &pass.vk_framebuffers {
                    unsafe {
                        self.context.device.destroy_framebuffer(*framebuffer, None);
                    }
                }
            }

            // Get the new depth texture image view (depth texture is recreated during swapchain recreation)
            let new_depth_view = self.frame_context.depth_render_texture.image_view;

            // Recreate framebuffers with new swapchain images
            for (image_index, (_vk_image, image_view, extent, _format)) in
                swapchain_images.iter().enumerate()
            {
                for pass_idx in 0..graph.passes.len() {
                    let framebuffer = self
                        .context
                        .create_framebuffer(
                            new_render_pass,
                            &[*image_view, new_depth_view],
                            *extent,
                        )
                        .map_err(RenderGraphError::VulkanError)
                        .unwrap();

                    if image_index == 0 {
                        graph.passes[pass_idx].vk_framebuffers = vec![framebuffer];
                    } else {
                        graph.passes[pass_idx].vk_framebuffers.push(framebuffer);
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
                &self.context.swapchain_loader,
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
                vk_image: self.frame_context.swapchain_images[image_index as usize],
                image_view: self.frame_context.swapchain_image_views[image_index as usize],
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
                vk_image: self.frame_context.depth_render_texture.image,
                image_view: self.frame_context.depth_render_texture.image_view,
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
    ///
    /// # Returns
    /// A `MaterialHandle` that references the registered material.
    pub fn create_material(
        &mut self,
        pipeline: Rc<RefCell<MaterialPipeline>>,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
    ) -> MaterialHandle {
        use crate::rendering::registry::MaterialAsset;

        let material_asset = MaterialAsset {
            pipeline,
            texture,
            vertex_binding,
        };

        self.asset_registry.register_material(material_asset)
    }

    /// Setup a single render graph with multiple framebuffers (one per swapchain image).
    /// This creates the graph upfront during initialization to avoid
    /// destroying Vulkan objects while the GPU is still using them.
    ///
    /// The draw list will be provided each frame via `render_frame_with_drawlist`.
    pub fn setup_render_graph(&mut self) {
        // Debug: Print the immediate-mode render pass info
        println!("=== Immediate-mode RenderPass ===");
        println!(
            "  VkRenderPass handle: {:?}",
            self.render_pass.get_vk_renderpass()
        );

        // Build a single render graph
        let mut graph_builder = RenderGraphBuilder::new();

        // Create a placeholder swapchain resource (will be updated for each image)
        let swapchain_resource = graph_builder.add_resource(
            "swapchain",
            ResourceKind::ExternalImage {
                vk_image: self.frame_context.swapchain_images[0],
                image_view: self.frame_context.swapchain_image_views[0],
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

        let swapchain_res = swapchain_resource;
        let depth_res = depth_resource;

        graph_builder.add_pass("geometry_pass", move |pass| {
            pass.write(Attachment::Color(swapchain_res))
                .write(Attachment::DepthStencil(depth_res))
                .clear_color(swapchain_res, [0.3, 0.5, 0.3, 1.0])
                .clear_depth_stencil(depth_res, 1.0, 0)
                .execute("geometry_pass", move |ctx| {
                    // Get the draw list for this frame
                    let draw_list_opt = draw_list_cell_for_pass.borrow_mut().take();
                    if let Some(draw_list) = draw_list_opt {
                        // SAFETY: The asset_registry_ptr is valid for the entire lifetime of the renderer
                        // and this closure is only called while the renderer is alive
                        // We need mutable access to call get_material_mut for update_buffer
                        let registry = unsafe { &mut *asset_registry_ptr };

                        // Process each draw call
                        for draw in &draw_list.draws {
                            // Get the mesh data first (immutable borrow)
                            let mesh_data = registry.get_mesh(draw.mesh).map(|m| {
                                (
                                    m.index_buffer.as_ref().map(|ib| (ib.object(), ib.index_type, ib.count())),
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

                            // Upload uniform buffers
                            // The material's uniform layout determines the expected buffer size
                            // We always provide data for all fields to avoid uninitialized memory
                            let params_bytes = draw.params.as_bytes_with_color();
                            material.pipeline.borrow_mut().update_buffer(&params_bytes);

                            // Bind the graphics pipeline
                            let cmd_buf = ctx.command_buffer.vk_command_buffer();
                            material.pipeline.borrow().bind(cmd_buf);

                            // Bind vertex and index buffers and draw
                            if let Some((index_buffer, index_type, index_count)) = index_data {
                                ctx.command_buffer.bind_index_buffer(
                                    index_buffer,
                                    0,
                                    index_type,
                                );

                                if let Some((vertex_buffer, _)) = vertex_data {
                                    ctx.command_buffer.bind_vertex_buffers(0, &[vertex_buffer], &[0]);
                                    ctx.command_buffer.draw_indexed(index_count, 1, 0, 0, 0);
                                }
                            } else if let Some((vertex_buffer, vertex_count)) = vertex_data {
                                ctx.command_buffer.bind_vertex_buffers(0, &[vertex_buffer], &[0]);
                                ctx.command_buffer.draw_array(vertex_count, 1, 0, 0);
                            }
                        }
                    }
                });
        });

        let vulkan_context = self.context.clone();
        let existing_render_pass = self.render_pass.get_vk_renderpass();
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
                    .map(|(&image, &view)| {
                        (
                            image,
                            view,
                            self.frame_context.swapchain.get_extent(),
                            self.frame_context.swapchain.format.format,
                        )
                    })
                    .collect();

                if let Err(e) =
                    graph.create_swapchain_framebuffers(&swapchain_images, existing_render_pass)
                {
                    println!("Failed to create swapchain framebuffers: {:?}", e);
                } else {
                    self.render_graph = Some(graph);
                }
            }
            Err(e) => {
                println!("Failed to compile render graph: {:?}", e);
            }
        }
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

        // Set the draw list for this frame
        graph.set_draw_list(draw_list);

        let mut command_buffer = self.frame_context.command_buffers[image_index].clone();
        command_buffer.begin_command(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        // Execute the render graph with the current image index
        graph.execute(&mut command_buffer, image_index)?;

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
}
