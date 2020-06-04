use context::VulkanCtx;
pub use pipeline::RenderPipeline;
use swapdata::*;
pub use texture::*;
pub use vertexbuffer::*;
mod context;
mod pipeline;
mod swapdata;
mod texture;
mod vertexbuffer;

use crate::rendering::Mesh;

use std::{ffi::CString, sync::Mutex};

use erupt::{
    extensions::{khr_surface::*, khr_swapchain::*},
    utils::{allocator::Allocator, loading::DefaultCoreLoader},
    vk1_0::*,
    DeviceLoader,
};
use lazy_static::lazy_static;

use winit::event::Event;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

lazy_static! {
    static ref CORE_LOADER: Mutex<DefaultCoreLoader> = {
        let core = Mutex::new(DefaultCoreLoader::new().unwrap());
        core.lock().unwrap().load_vk1_0().unwrap();
        core.lock().unwrap().load_vk1_1().unwrap();
        core
    };
}

pub struct VulkanRenderer {
    pub window: Window,
    pub context: VulkanCtx,
    pub render_pass: RenderPass,
    pub swapchain_framebuffers: Vec<Framebuffer>,
    swap_data: SwapData,
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
            .build(event_loop)
            .unwrap();
        let context = VulkanCtx::init(&window, with_validation_layers, app_name, engine_name);
        //TODO: This should be configurable and put elsewhere?

        // https://vulkan-tutorial.com/Drawing_a_triangle/Graphics_pipeline_basics/Render_passes
        let render_pass = {
            let candidate_formats = vec![
                Format::D32_SFLOAT,
                Format::D32_SFLOAT_S8_UINT,
                Format::D24_UNORM_S8_UINT,
            ];
            let depth_format = context.find_depth_format(candidate_formats);

            let attachments = vec![AttachmentDescriptionBuilder::new()
                .format(context.surface_format.format)
                .samples(SampleCountFlagBits::_1)
                .load_op(AttachmentLoadOp::CLEAR)
                .store_op(AttachmentStoreOp::STORE)
                .stencil_load_op(AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(AttachmentStoreOp::DONT_CARE)
                .initial_layout(ImageLayout::UNDEFINED)
                .final_layout(ImageLayout::PRESENT_SRC_KHR)];

            let color_attachment_refs = vec![AttachmentReferenceBuilder::new()
                .attachment(0)
                .layout(ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
            let subpasses = vec![SubpassDescriptionBuilder::new()
                .pipeline_bind_point(PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_attachment_refs)];
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
        };

        let swapchain_framebuffers: Vec<_> = context
            .swapchain_image_views
            .iter()
            .map(|image_view| {
                let attachments = vec![*image_view];
                let create_info = FramebufferCreateInfoBuilder::new()
                    .render_pass(render_pass)
                    .attachments(&attachments)
                    .width(context.surface_caps.current_extent.width)
                    .height(context.surface_caps.current_extent.height)
                    .layers(1);

                unsafe { context.device.create_framebuffer(&create_info, None, None) }.unwrap()
            })
            .collect();

        let swap_data = SwapData::new(&context.device, &context.swapchain_images, FRAMES_IN_FLIGHT);

        let renderer = Self {
            window,
            context,
            render_pass,
            swapchain_framebuffers,
            swap_data,
        };
        renderer
    }

    pub fn destroy(&mut self, mesh_data: &mut [Mesh]) {
        unsafe {
            self.context.pre_destroy();
            self.swap_data.destroy(&self.context.device);
            for mesh in mesh_data {
                mesh.destroy(&self.context.device, &mut self.context.allocator);
            }

            self.context
                .device
                .destroy_render_pass(self.render_pass, None);
            for &framebuffer in &self.swapchain_framebuffers {
                self.context.device.destroy_framebuffer(framebuffer, None);
            }

            self.context.destroy();
        }
        println!("Clean shutdown!");
    }

    pub fn device_and_allocator(&mut self) -> (&DeviceLoader, &mut Allocator) {
        (&self.context.device, &mut self.context.allocator)
    }

    pub fn surface_caps(&self) -> SurfaceCapabilitiesKHR {
        self.context.surface_caps
    }

    pub fn num_images(&self) -> usize {
        self.context.swapchain_image_views.len()
    }
    // TODO: Make designated functions for drawing, updating stuff, etc.
    // rather than sending the winit event here
    pub fn handle_event(
        &mut self,
        event: &Event<()>,
        meshes: &mut [Mesh],
        _delta_time: f32,
        proj: &mikpe_math::Mat4,
        view: &mikpe_math::Mat4,
    ) {
        match event {
            Event::MainEventsCleared => {
                self.swap_data.wait_for_fence(&self.context.device);

                let (available_sem, finished_sem, in_flight_fence, image_index) = self
                    .swap_data
                    .swap_images(&self.context.device, self.context.swapchain);

                let wait_semaphores = vec![available_sem];
                let command_buffers = vec![self.context.command_buffers[image_index as usize]];
                let swapchain_framebuffers =
                    vec![self.swapchain_framebuffers[image_index as usize]];
                for mesh in meshes.iter_mut() {
                    mesh.upload_pipeline_data(
                        &self.context.device,
                        image_index as usize,
                        view.clone(),
                        proj.clone(),
                    );
                }
                for (&command_buffer, &framebuffer) in
                    command_buffers.iter().zip(swapchain_framebuffers.iter())
                {
                    let begin_info = CommandBufferBeginInfoBuilder::new();
                    unsafe {
                        self.context
                            .device
                            .begin_command_buffer(command_buffer, &begin_info)
                    }
                    .unwrap();

                    let clear_values = vec![ClearValue {
                        color: ClearColorValue {
                            float32: [0.3, 0.5, 0.3, 1.0],
                        },
                    }];
                    let begin_info = RenderPassBeginInfoBuilder::new()
                        .render_pass(self.render_pass)
                        .framebuffer(framebuffer)
                        .render_area(Rect2D {
                            offset: Offset2D { x: 0, y: 0 },
                            extent: self.context.surface_caps.current_extent,
                        })
                        .clear_values(&clear_values);

                    unsafe {
                        self.context.device.cmd_begin_render_pass(
                            command_buffer,
                            &begin_info,
                            SubpassContents::INLINE,
                        );
                        for mesh in meshes.iter() {
                            mesh.add_draw_cmd(
                                &self.context.device,
                                command_buffer,
                                image_index as usize,
                            );
                        }
                        self.context.device.cmd_end_render_pass(command_buffer);

                        self.context
                            .device
                            .end_command_buffer(command_buffer)
                            .unwrap();
                    }
                }
                let signal_semaphores = vec![finished_sem];
                let submit_info = SubmitInfoBuilder::new()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&[PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
                    .command_buffers(&command_buffers)
                    .signal_semaphores(&signal_semaphores);
                unsafe {
                    let in_flight_fence = in_flight_fence;
                    self.context
                        .device
                        .reset_fences(&[in_flight_fence])
                        .unwrap();
                    self.context
                        .device
                        .queue_submit(self.context.queue, &[submit_info], in_flight_fence)
                        .unwrap()
                }

                let swapchains = vec![self.context.swapchain];
                let image_indices = vec![image_index];
                let present_info = PresentInfoKHRBuilder::new()
                    .wait_semaphores(&signal_semaphores)
                    .swapchains(&swapchains)
                    .image_indices(&image_indices);

                unsafe {
                    self.context
                        .device
                        .queue_present_khr(self.context.queue, &present_info)
                }
                .unwrap();

                self.swap_data.step_frame();
            }
            _ => {}
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
    }
}
