pub mod vulkan;
pub use vulkan::*;

use ash::vk;

use std::{ffi::CString, sync::Arc};

use winit::window::{Window, WindowBuilder};
use winit::{dpi::LogicalSize, event_loop::EventLoop};

pub use ash::vk::{Format, IndexType, PipelineBindPoint};

pub struct VulkanRenderer {
    pub window: Window,
    pub context: Arc<VulkanContext>,
    pub frame_context: VulkanFrameCtx,
    pub render_pass: RenderPass,
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

        let color_format = frame_context.swapchain.format.format;
        let depth_format = frame_context.depth_render_texture.format;
        let render_pass =
            RenderPass::create_opaque(context.device.clone(), color_format, depth_format);

        let swapchain_framebuffers: Vec<_> = frame_context
            .swapchain_image_views
            .iter()
            .map(|image_view| {
                let attachments = vec![*image_view, frame_context.depth_render_texture.image_view];
                let create_info = vk::FramebufferCreateInfo::builder()
                    .render_pass(render_pass.get_vk_renderpass())
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
            self.render_pass.destroy();
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
            self.render_pass.destroy();
            for &framebuffer in &self.swapchain_framebuffers {
                self.context.device.destroy_framebuffer(framebuffer, None);
            }
        }

        let color_format = self.frame_context.swapchain.format.format;
        let depth_format = self.frame_context.depth_render_texture.format;
        self.render_pass =
            RenderPass::create_opaque(self.context.device.clone(), color_format, depth_format);

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
                    .render_pass(self.render_pass.get_vk_renderpass())
                    .attachments(&attachments)
                    .width(self.frame_context.swapchain.get_extent().width)
                    .height(self.frame_context.swapchain.get_extent().height)
                    .layers(1);

                unsafe { self.context.device.create_framebuffer(&create_info, None) }.unwrap()
            })
            .collect();
    }

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

    pub fn get_commandbuffer_opaque_pass(&self) -> CommandBuffer {
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
        command_buffer.begin_render_pass(
            framebuffer,
            self.render_pass.get_vk_renderpass(),
            render_area,
            &clear_values,
        );
        command_buffer
    }

    pub fn submit_frame(&mut self, command_buffers: Vec<&CommandBuffer>) {
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
