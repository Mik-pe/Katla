pub mod vulkan;

use ash::vk;

pub use pipeline::{ImageInfo, RenderPipeline};
use swapdata::*;
pub use texture::*;
pub use vertexbuffer::*;

use vulkan::context::{VulkanContext, VulkanFrameCtx};
use vulkan::pipeline;
use vulkan::swapdata;
use vulkan::texture;
use vulkan::vertexbuffer;

use std::{ffi::CString, sync::Arc};

use winit::window::{Window, WindowBuilder};
use winit::{dpi::LogicalSize, event_loop::EventLoop};

pub struct VulkanRenderer {
    pub window: Window,
    pub context: Arc<VulkanContext>,
    pub frame_context: VulkanFrameCtx,
    pub render_pass: vk::RenderPass,
    pub swapchain_framebuffers: Vec<vk::Framebuffer>,
    swap_data: SwapData,
    current_framedata: Option<FrameData>,
}
struct FrameData {
    available_sem: vk::Semaphore,
    finished_sem: vk::Semaphore,
    in_flight_fence: vk::Fence,
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
            .with_title(app_name.to_str().expect("Invalid app name"))
            .with_resizable(true)
            .with_min_inner_size(LogicalSize {
                width: 1.0,
                height: 1.0,
            })
            .with_maximized(false)
            .build(event_loop)
            .unwrap();
        let context = Arc::new(VulkanContext::init(
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
                let create_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(render_pass)
                    .attachments(&attachments)
                    .width(frame_context.swapchain.get_extent().width)
                    .height(frame_context.swapchain.get_extent().height)
                    .layers(1);

                unsafe { context.device.create_framebuffer(&create_info, None) }.unwrap()
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

    fn create_render_pass(
        context: &VulkanContext,
        frame_context: &VulkanFrameCtx,
    ) -> vk::RenderPass {
        let color_attachment = vk::AttachmentDescription::builder()
            .format(frame_context.swapchain.format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let depth_attachment = vk::AttachmentDescription::builder()
            .format(frame_context.depth_render_texture.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let attachments = [color_attachment.build(), depth_attachment.build()];

        let color_attachment_refs = [vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build()];
        let depth_attachment_ref = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
        let subpasses = [vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_refs)
            .depth_stencil_attachment(&depth_attachment_ref)
            .build()];
        let dependencies = [vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .build()];

        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);

        unsafe { context.device.create_render_pass(&create_info, None) }.unwrap()
    }

    // fn create_framebuffers(context: &VulkanCtx) -> Vec<Framebuffer> {
    //     context
    //         .swapchain_image_views
    //         .iter()
    //         .map(|image_view| {
    //             let attachments = vec![*image_view];
    //             let create_info = FramebufferCreateInfo::builder()
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
                .destroy_render_pass(self.render_pass, None);
            for &framebuffer in &self.swapchain_framebuffers {
                self.context.device.destroy_framebuffer(framebuffer, None);
            }

            self.frame_context.destroy();
        }
        println!("Clean shutdown!");
    }
    pub fn wait_for_device(&self) {
        unsafe {
            self.context.device.device_wait_idle().unwrap();
        }
    }

    pub fn recreate_swapchain(&mut self) {
        self.wait_for_device();
        self.frame_context.recreate_swapchain();
        //Destroy the previous state:
        unsafe {
            self.context
                .device
                .destroy_render_pass(self.render_pass, None);
            for &framebuffer in &self.swapchain_framebuffers {
                self.context.device.destroy_framebuffer(framebuffer, None);
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
                let create_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(self.render_pass)
                    .attachments(&attachments)
                    .width(self.frame_context.swapchain.get_extent().width)
                    .height(self.frame_context.swapchain.get_extent().height)
                    .layers(1);

                unsafe { self.context.device.create_framebuffer(&create_info, None) }.unwrap()
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

    // pub fn current_extent(&self) -> vk::Extent2D {
    //     self.frame_context.current_extent
    // }

    pub fn num_images(&self) -> usize {
        self.frame_context.swapchain_image_views.len()
    }

    pub fn swap_frames(&mut self) {
        self.swap_data.wait_for_fence(&self.context.device);

        let (available_sem, finished_sem, in_flight_fence, image_index) =
            self.swap_data.swap_images(
                &self.context.device,
                &self.context.swapchain_loader,
                self.frame_context.swapchain.swapchain,
            );
        self.current_framedata = Some(FrameData {
            available_sem,
            finished_sem,
            in_flight_fence,
            image_index,
        });
    }

    pub fn get_commandbuffer_opaque_pass(&self) -> vulkan::CommandBuffer {
        let (framebuffer, command_buffer) = {
            if let Some(frame_data) = &self.current_framedata {
                (
                    self.swapchain_framebuffers[frame_data.image_index as usize],
                    self.frame_context.command_buffers[frame_data.image_index as usize].clone(),
                )
            } else {
                panic!("No available frame index!");
            }
        };
        command_buffer.begin_command(vk::CommandBufferUsageFlags::default());

        let clear_values = vec![
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.3, 0.5, 0.3, 1.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];
        let current_extent = self.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: current_extent,
        };
        command_buffer.begin_render_pass(framebuffer, self.render_pass, render_area, &clear_values);
        command_buffer
    }

    pub fn submit_frame(&mut self, command_buffers: Vec<&vulkan::CommandBuffer>) {
        let frame_data = self.current_framedata.take().unwrap();

        let wait_semaphores = vec![frame_data.available_sem];

        let signal_semaphores = vec![frame_data.finished_sem];
        let in_flight_fence = frame_data.in_flight_fence;
        unsafe {
            self.context
                .device
                .reset_fences(&[in_flight_fence])
                .unwrap();
        }
        self.context.gfx_queue.submit(
            &command_buffers,
            &wait_semaphores,
            &signal_semaphores,
            in_flight_fence,
        );

        let swapchains = vec![self.frame_context.swapchain.swapchain];
        let image_indices = vec![frame_data.image_index];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        unsafe {
            self.context
                .swapchain_loader
                .queue_present(self.context.graphics_queue, &present_info)
        }
        .unwrap();

        self.swap_data.step_frame();
    }
}
