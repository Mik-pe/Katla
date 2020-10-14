use context::{VulkanContext, VulkanFrameCtx};
pub use pipeline::{ImageInfo, RenderPipeline};
use swapdata::*;
pub use texture::*;
pub use vertexbuffer::*;
mod context;
mod pipeline;
mod swapdata;
mod texture;
mod vertexbuffer;

use std::{ffi::CString, rc::Rc, sync::Mutex};

use erupt::{extensions::khr_swapchain::*, vk1_0::*, DefaultEntryLoader, EntryLoader};
use lazy_static::lazy_static;

use winit::window::{Window, WindowBuilder};
use winit::{dpi::LogicalSize, event_loop::EventLoop};

lazy_static! {
    static ref ENTRY_LOADER: Mutex<DefaultEntryLoader> = {
        let core = Mutex::new(EntryLoader::new().unwrap());
        core
    };
}

pub struct VulkanRenderer {
    pub window: Window,
    pub context: Rc<VulkanContext>,
    pub frame_context: VulkanFrameCtx,
    pub render_pass: RenderPass,
    pub swapchain_framebuffers: Vec<Framebuffer>,
    swap_data: SwapData,
    current_framedata: Option<FrameData>,
}
struct FrameData {
    available_sem: Semaphore,
    finished_sem: Semaphore,
    in_flight_fence: Fence,
    image_index: u32,
}

const FRAMES_IN_FLIGHT: usize = 2;

impl VulkanRenderer {
    pub fn init(
        event_loop: &EventLoop<()>,
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Self {
        let window = WindowBuilder::new()
            .with_title("Erupt_Test_Mikpe")
            .with_resizable(true)
            .with_min_inner_size(LogicalSize {
                width: 1.0,
                height: 1.0,
            })
            .with_maximized(false)
            .build(event_loop)
            .unwrap();
        let context = Rc::new(VulkanContext::init(
            &window,
            with_validation_layers,
            app_name,
            engine_name,
        ));
        let frame_context = VulkanFrameCtx::init(&context);

        let render_pass = Self::create_render_pass(&context, &frame_context);

        let swapchain_framebuffers: Vec<_> = frame_context
            .swapchain_image_views
            .iter()
            .map(|image_view| {
                let attachments = vec![*image_view, frame_context.depth_render_texture.image_view];
                let create_info = FramebufferCreateInfoBuilder::new()
                    .render_pass(render_pass)
                    .attachments(&attachments)
                    .width(frame_context.current_extent.width)
                    .height(frame_context.current_extent.height)
                    .layers(1);

                unsafe { context.device.create_framebuffer(&create_info, None, None) }.unwrap()
            })
            .collect();

        let swap_data = SwapData::new(
            &context.device,
            &frame_context.swapchain_images,
            FRAMES_IN_FLIGHT,
        );

        let renderer = Self {
            window,
            context,
            frame_context,
            render_pass,
            swapchain_framebuffers,
            swap_data,
            current_framedata: None,
        };
        renderer
    }

    fn create_render_pass(context: &VulkanContext, frame_context: &VulkanFrameCtx) -> RenderPass {
        let color_attachment = AttachmentDescriptionBuilder::new()
            .format(frame_context.current_surface_format.format)
            .samples(SampleCountFlagBits::_1)
            .load_op(AttachmentLoadOp::CLEAR)
            .store_op(AttachmentStoreOp::STORE)
            .stencil_load_op(AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(AttachmentStoreOp::DONT_CARE)
            .initial_layout(ImageLayout::UNDEFINED)
            .final_layout(ImageLayout::PRESENT_SRC_KHR);

        let depth_attachment = AttachmentDescriptionBuilder::new()
            .format(frame_context.depth_render_texture.format)
            .samples(SampleCountFlagBits::_1)
            .load_op(AttachmentLoadOp::CLEAR)
            .store_op(AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(AttachmentStoreOp::DONT_CARE)
            .initial_layout(ImageLayout::UNDEFINED)
            .final_layout(ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let attachments = vec![color_attachment, depth_attachment];

        let color_attachment_refs = vec![AttachmentReferenceBuilder::new()
            .attachment(0)
            .layout(ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let depth_attachment_ref = AttachmentReferenceBuilder::new()
            .attachment(1)
            .layout(ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let subpasses = vec![SubpassDescriptionBuilder::new()
            .pipeline_bind_point(PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_refs)
            .depth_stencil_attachment(&depth_attachment_ref)];
        let dependencies = vec![SubpassDependencyBuilder::new()
            .src_subpass(SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(AccessFlags::empty())
            .dst_stage_mask(PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(AccessFlags::COLOR_ATTACHMENT_WRITE)];

        let create_info = RenderPassCreateInfoBuilder::new()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);

        unsafe { context.device.create_render_pass(&create_info, None, None) }.unwrap()
    }

    // fn create_framebuffers(context: &VulkanCtx) -> Vec<Framebuffer> {
    //     context
    //         .swapchain_image_views
    //         .iter()
    //         .map(|image_view| {
    //             let attachments = vec![*image_view];
    //             let create_info = FramebufferCreateInfoBuilder::new()
    //                 .render_pass(render_pass)
    //                 .attachments(&attachments)
    //                 .width(context.current_extent.width)
    //                 .height(context.current_extent.height)
    //                 .layers(1);

    //             unsafe { context.device.create_framebuffer(&create_info, None, None) }.unwrap()
    //         })
    //         .collect()
    // }

    pub fn destroy(&mut self) {
        unsafe {
            self.context.pre_destroy();
            self.swap_data.destroy(&self.context.device);
            self.context
                .device
                .destroy_render_pass(Some(self.render_pass), None);
            for &framebuffer in &self.swapchain_framebuffers {
                self.context
                    .device
                    .destroy_framebuffer(Some(framebuffer), None);
            }

            self.frame_context.destroy();
        }
        println!("Clean shutdown!");
    }

    pub fn recreate_swapchain(&mut self) {
        unsafe {
            self.context.device.device_wait_idle().unwrap();
        }
        self.frame_context.recreate_swapchain();
        //Destroy the previous state:
        unsafe {
            self.context
                .device
                .destroy_render_pass(Some(self.render_pass), None);
            for &framebuffer in &self.swapchain_framebuffers {
                self.context
                    .device
                    .destroy_framebuffer(Some(framebuffer), None);
            }
        }

        self.render_pass = Self::create_render_pass(&self.context, &self.frame_context);

        self.swapchain_framebuffers = self
            .frame_context
            .swapchain_image_views
            .iter()
            .map(|image_view| {
                let attachments = vec![
                    *image_view,
                    self.frame_context.depth_render_texture.image_view,
                ];
                let create_info = FramebufferCreateInfoBuilder::new()
                    .render_pass(self.render_pass)
                    .attachments(&attachments)
                    .width(self.frame_context.current_extent.width)
                    .height(self.frame_context.current_extent.height)
                    .layers(1);

                unsafe {
                    self.context
                        .device
                        .create_framebuffer(&create_info, None, None)
                }
                .unwrap()
            })
            .collect();
        // Whenever the window resizes we need to recreate everything dependent on the window size.
        // In this example that includes the swapchain, the framebuffers and the dynamic state viewport.
        // if self.internal_state.recreate_swapchain {

        // Get the new dimensions of the window.
        // let dimensions: [u32; 2] = self.surface.window().inner_size().into();
        // let (new_swapchain, new_images) = match self.swapchain.recreate_with_dimensions(dimensions)
        // {
        //     Ok(r) => r,
        //     // This error tends to happen when the user is manually resizing the window.
        //     // Simply restarting the loop is the easiest way to fix this issue.
        //     Err(SwapchainCreationError::UnsupportedDimensions) => return,
        //     Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
        // };

        // self.swapchain = new_swapchain;
        // self.internal_state.framebuffers = window_size_dependent_setup(
        //     &new_images,
        //     self.device.clone(),
        //     self.render_pass.clone(),
        //     &mut self.internal_state.dynamic_state,
        // );

        // self.internal_state.recreate_swapchain = false;
        // }
    }

    pub fn current_extent(&self) -> Extent2D {
        self.frame_context.current_extent
    }

    pub fn num_images(&self) -> usize {
        self.frame_context.swapchain_image_views.len()
    }

    pub fn swap_frames(&mut self) {
        self.swap_data.wait_for_fence(&self.context.device);

        let (available_sem, finished_sem, in_flight_fence, image_index) = self
            .swap_data
            .swap_images(&self.context.device, self.frame_context.swapchain);
        self.current_framedata = Some(FrameData {
            available_sem,
            finished_sem,
            in_flight_fence,
            image_index,
        });
    }

    pub fn get_commandbuffer_opaque_pass(&mut self) -> CommandBuffer {
        let (framebuffer, command_buffer) = {
            if let Some(frame_data) = &self.current_framedata {
                (
                    self.swapchain_framebuffers[frame_data.image_index as usize],
                    self.frame_context.command_buffers[frame_data.image_index as usize],
                )
            } else {
                panic!("No available frame index!");
            }
        };
        let begin_info = CommandBufferBeginInfoBuilder::new();
        unsafe {
            self.context
                .device
                .begin_command_buffer(command_buffer, &begin_info)
        }
        .unwrap();

        let clear_values = vec![
            ClearValue {
                color: ClearColorValue {
                    float32: [0.3, 0.5, 0.3, 1.0],
                },
            },
            ClearValue {
                depth_stencil: ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];
        let begin_info = RenderPassBeginInfoBuilder::new()
            .render_pass(self.render_pass)
            .framebuffer(framebuffer)
            .render_area(Rect2D {
                offset: Offset2D { x: 0, y: 0 },
                extent: self.frame_context.current_extent,
            })
            .clear_values(&clear_values);

        unsafe {
            self.context.device.cmd_begin_render_pass(
                command_buffer,
                &begin_info,
                SubpassContents::INLINE,
            );
            self.context.device.cmd_set_scissor(
                command_buffer,
                0,
                &[Rect2DBuilder::new().extent(self.frame_context.current_extent)],
            );
            self.context.device.cmd_set_viewport(
                command_buffer,
                0,
                &[ViewportBuilder::new()
                    .height(self.frame_context.current_extent.height as f32)
                    .width(self.frame_context.current_extent.width as f32)
                    .x(0.0)
                    .y(0.0)
                    .min_depth(0.0)
                    .max_depth(1.0)],
            )
        }
        command_buffer
    }

    pub fn submit_frame(&mut self, command_buffers: Vec<CommandBuffer>) {
        let frame_data = self.current_framedata.take().unwrap();

        let wait_semaphores = vec![frame_data.available_sem];

        let signal_semaphores = vec![frame_data.finished_sem];
        let submit_info = SubmitInfoBuilder::new()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&[PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        unsafe {
            let in_flight_fence = frame_data.in_flight_fence;
            self.context
                .device
                .reset_fences(&[in_flight_fence])
                .unwrap();
            self.context
                .device
                .queue_submit(
                    self.context.graphics_queue,
                    &[submit_info],
                    Some(in_flight_fence),
                )
                .unwrap()
        }

        let swapchains = vec![self.frame_context.swapchain];
        let image_indices = vec![frame_data.image_index];
        let present_info = PresentInfoKHRBuilder::new()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        unsafe {
            self.context
                .device
                .queue_present_khr(self.context.graphics_queue, &present_info)
        }
        .unwrap();

        self.swap_data.step_frame();
    }
}

/*
match event {
    Event::WindowEvent { event, .. } => match event {
        WindowEvent::Resized(_) => {
            self.internal_state.recreate_swapchain = true;
        }
        _ => {}
    },
    Event::RedrawEventsCleared => {
        // It is important to call this function from time to time, otherwise resources will keep
        // accumulating and you will eventually reach an out of memory error.
        // Calling this function polls various fences in order to determine what the GPU has
        // already processed, and frees the resources that are no longer needed.
        self.internal_state
            .previous_frame_end
            .as_mut()
            .unwrap()
            .cleanup_finished();
        self.internal_state.angle += delta_time;

        // Whenever the window resizes we need to recreate everything dependent on the window size.
        // In this example that includes the swapchain, the framebuffers and the dynamic state viewport.
        if self.internal_state.recreate_swapchain {
            // Get the new dimensions of the window.
            let dimensions: [u32; 2] = self.surface.window().inner_size().into();
            let (new_swapchain, new_images) =
                match self.swapchain.recreate_with_dimensions(dimensions) {
                    Ok(r) => r,
                    // This error tends to happen when the user is manually resizing the window.
                    // Simply restarting the loop is the easiest way to fix this issue.
                    Err(SwapchainCreationError::UnsupportedDimensions) => return,
                    Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                };

            self.swapchain = new_swapchain;
            self.internal_state.framebuffers = window_size_dependent_setup(
                &new_images,
                self.device.clone(),
                self.render_pass.clone(),
                &mut self.internal_state.dynamic_state,
            );
            self.internal_state.recreate_swapchain = false;
        }

        let uniform_buffer_subbuffer = {
            use mikpe_math::Mat4;

            let world = Mat4::from_rotaxis(&self.internal_state.angle, [0.0, 1.0, 0.0]);

            let uniform_data = my_pipeline::vs::ty::Data {
                world: world.into(),
                view: view.clone().into(),
                proj: projection.clone().into(),
            };

            self.internal_state
                .uniform_buffer
                .next(uniform_data)
                .unwrap()
        };

        let layout = self
            .renderpipeline
            .pipeline
            .descriptor_set_layout(0)
            .unwrap();

        let set = Arc::new(
            PersistentDescriptorSet::start(layout.clone())
                .add_buffer(uniform_buffer_subbuffer)
                .unwrap()
                .build()
                .unwrap(),
        );

        // Before we can draw on the output, we have to *acquire* an image from the swapchain. If
        // no image is available (which happens if you submit draw commands too quickly), then the
        // function will block.
        // This operation returns the index of the image that we are allowed to draw upon.
        //
        // This function can block if no image is available. The parameter is an optional timeout
        // after which the function call will return an error.
        let (image_num, suboptimal, acquire_future) =
            match swapchain::acquire_next_image(self.swapchain.clone(), None) {
                Ok(r) => r,
                Err(AcquireError::OutOfDate) => {
                    self.internal_state.recreate_swapchain = true;
                    return;
                }
                Err(e) => panic!("Failed to acquire next image: {:?}", e),
            };

        if suboptimal {
            self.internal_state.recreate_swapchain = true;
        }

        let clear_values = vec![[0.3, 0.5, 0.3, 1.0].into(), 1f32.into()];

        // In order to draw, we have to build a *command buffer*. The command buffer object holds
        // the list of commands that are going to be executed.
        //
        // Building a command buffer is an expensive operation (usually a few hundred
        // microseconds), but it is known to be a hot path in the driver and is expected to be
        // optimized.
        //
        // Note that we have to pass a queue family when we create the command buffer. The command
        // buffer will only be executable on that given queue family.
        let mut cmd_buffer_builder = AutoCommandBufferBuilder::primary_one_time_submit(
            self.device.clone(),
            self.command_queue.family(),
        )
        .unwrap()
        // Before we can draw, we have to *enter a render pass*. There are two methods to do
        // this: `draw_inline` and `draw_secondary`. The latter is a bit more advanced and is
        // not covered here.
        //
        // The third parameter builds the list of values to clear the attachments with. The API
        // is similar to the list of attachments when building the framebuffers, except that
        // only the attachments that use `load: Clear` appear in the list.
        .begin_render_pass(
            self.internal_state.framebuffers[image_num].clone(),
            false,
            clear_values,
        )
        .unwrap();
        for meshbuffer in meshbuffers {
            cmd_buffer_builder = meshbuffer.draw_data(
                cmd_buffer_builder,
                set.clone(),
                &self.internal_state.dynamic_state,
            );
        }
        let command_buffer = cmd_buffer_builder
            .end_render_pass()
            .unwrap()
            .build() // Finish building the command buffer by calling `build`.
            .unwrap();

        let future = self
            .internal_state
            .previous_frame_end
            .take()
            .unwrap()
            .join(acquire_future)
            .then_execute(self.command_queue.clone(), command_buffer)
            .unwrap()
            .then_swapchain_present(
                self.command_queue.clone(),
                self.swapchain.clone(),
                image_num,
            )
            .then_signal_fence_and_flush();

        match future {
            Ok(future) => {
                self.internal_state.previous_frame_end = Some(Box::new(future) as Box<_>);
            }
            Err(FlushError::OutOfDate) => {
                self.internal_state.recreate_swapchain = true;
                self.internal_state.previous_frame_end =
                    Some(Box::new(sync::now(self.device.clone())) as Box<_>);
            }
            Err(e) => {
                println!("Failed to flush future: {:?}", e);
                self.internal_state.previous_frame_end =
                    Some(Box::new(sync::now(self.device.clone())) as Box<_>);
            }
        }
    }
    _ => (),
}
*/
