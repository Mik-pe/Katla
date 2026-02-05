pub mod render_graph;
pub mod vulkan;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
pub use render_graph::errors::RenderGraphError;
pub use render_graph::pass::{PassBuilder, PassExecutionContext};
pub use render_graph::resource::{
    CompiledResource, ResourceAccessType, ResourceId, ResourceKind, ResourceLifetime, ResourceUsage,
};
pub use render_graph::*;
pub use vulkan::*;

use ash::vk;
use std::{cell::RefCell, ffi::CString, rc::Rc};

pub struct FrameData {
    pub available_sem: vk::Semaphore,
    pub finished_sem: vk::Semaphore,
    pub in_flight_fence: vk::Fence,
    pub image_index: u32,
}

/// Trait for rendering callbacks that can be invoked during render graph execution.
/// This allows katla_app to provide rendering logic without VulkanRenderer depending
/// on application-specific types like World or Camera.
pub trait RenderCallback: Send {
    /// Render a frame using the given command buffer.
    /// The callback receives delta time.
    /// View and projection matrices should be obtained by the callback implementation
    /// from its internal world/camera references.
    fn render(&mut self, command_buffer: &CommandBuffer, dt: f32);
}

pub struct VulkanRenderer {
    pub context: Rc<VulkanContext>,
    pub frame_context: VulkanFrameCtx,
    pub render_pass: RenderPass,
    pub swap_data: SwapData,
    pub current_framedata: Option<FrameData>,
    /// Optional callback for rendering during render graph execution.
    /// This is set by katla_app to provide drawing logic.
    /// Stored as Rc<RefCell<>> so render graph execution closures can also access it.
    pub render_callback: Option<Rc<RefCell<dyn RenderCallback>>>,
    /// Delta time for the current frame, stored as Rc<RefCell<>> so both
    /// the application and render graph closures can access it.
    pub frame_delta_time: Option<Rc<RefCell<f32>>>,
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
            render_callback: None,
            frame_delta_time: None,
            render_graph: None,
        }
    }

    pub fn destroy(&mut self) {
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
                        .map_err(|e| RenderGraphError::VulkanError(e))
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


    /// Set the delta time for the current frame.
    /// This should be called before render_frame() to pass dt to the render callback.
    pub fn set_delta_time(&mut self, dt: f32) {
        if let Some(dt_rc) = &self.frame_delta_time {
            *dt_rc.borrow_mut() = dt;
        }
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
                extent: self.frame_context.swapchain.get_extent().into(),
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
                extent: self.frame_context.swapchain.get_extent().into(),
            },
        )
    }

    /// Setup a single render graph with multiple framebuffers (one per swapchain image).
    /// This creates the graph upfront during initialization to avoid
    /// destroying Vulkan objects while the GPU is still using them.
    pub fn setup_render_graph(&mut self, callback: Rc<RefCell<dyn RenderCallback>>) {
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
                extent: self.frame_context.swapchain.get_extent().into(),
            },
        );

        let depth_resource = self.create_depth_resource(&mut graph_builder);

        let callback_for_graph = callback.clone();
        let dt_for_graph = self.frame_delta_time.clone();
        let swapchain_res = swapchain_resource;
        let depth_res = depth_resource;

        graph_builder.add_pass("geometry_pass", move |pass| {
            pass.write(Attachment::Color(swapchain_res))
                .write(Attachment::DepthStencil(depth_res))
                .clear_color(swapchain_res, [0.3, 0.5, 0.3, 1.0])
                .clear_depth_stencil(depth_res, 1.0, 0)
                .execute("geometry_pass", move |ctx| {
                    if let (Ok(mut cb), Some(dt_rc)) =
                        (callback_for_graph.try_borrow_mut(), dt_for_graph.as_ref())
                    {
                        let dt = *dt_rc.borrow();
                        cb.render(&ctx.command_buffer, dt);
                    }
                });
        });

        let vulkan_context = self.context.clone();
        let existing_render_pass = self.render_pass.get_vk_renderpass();
        match graph_builder.build(&vulkan_context) {
            Ok(mut graph) => {
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

    pub fn render_frame(&mut self) -> Result<(), RenderGraphError> {
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

        let mut command_buffer = self.frame_context.command_buffers[image_index].clone();
        command_buffer.begin_command(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        // Execute the render graph with the current image index
        graph.execute(&mut command_buffer, image_index)?;

        command_buffer.end_command();

        let frame_data = self.current_framedata.take().unwrap();
        let wait_semaphores = vec![frame_data.available_sem];
        let wait_dst_stage_mask = vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores = vec![frame_data.finished_sem];
        let in_flight_fence = frame_data.in_flight_fence;

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
