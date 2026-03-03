//! Frame rendering implementation.
//!
//! This module handles the actual GPU frame rendering, including:
//! - Acquiring swapchain images
//! - Recording command buffers
//! - Executing render passes
//! - Submitting to GPU
//! - Presenting to swapchain

use super::Application;
use ash::vk;
use katla_gfx::VulkanContext;
use log::debug;

impl Application {
    /// Render a single frame.
    ///
    /// This method handles the complete frame rendering pipeline:
    /// 1. Wait for previous frame to complete
    /// 2. Acquire next swapchain image
    /// 3. Record command buffer with render passes
    /// 4. Submit command buffer to GPU
    /// 5. Present to swapchain
    pub fn render_frame(&mut self) {
        let frame_index = self.renderer.swap_data.current_frame();
        let extent = self.renderer.frame_context.swapchain.get_extent();

        let swapchain = self.renderer.frame_context.swapchain.swapchain;

        // Wait for previous frame to complete
        self.renderer
            .swap_data
            .wait_for_fence(&self.renderer.context.device);

        // Acquire next swapchain image
        let (image_index, _suboptimal) = unsafe {
            let swapchain_loader = self
                .renderer
                .context
                .swapchain_loader
                .as_ref()
                .expect("Swapchain loader required");

            swapchain_loader
                .acquire_next_image(
                    swapchain,
                    u64::MAX,
                    self.renderer.swap_data.image_available_semaphore(),
                    vk::Fence::null(),
                )
                .expect("Failed to acquire swapchain image")
        };

        // Begin command buffer recording
        let command_buffer = &self.renderer.frame_context.command_buffers[frame_index];

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.renderer
                .context
                .device
                .begin_command_buffer(command_buffer.vk_command_buffer(), &begin_info)
                .expect("Failed to begin command buffer");
        };

        // Transition swapchain image from undefined to color attachment optimal
        let swapchain_images = self.renderer.frame_context.swapchain_images();
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
            self.renderer.context.device.cmd_pipeline_barrier(
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
        let swapchain_image_views = self.renderer.frame_context.swapchain_image_views();
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
            self.renderer
                .context
                .device
                .cmd_begin_rendering(command_buffer.vk_command_buffer(), &render_info);
        }

        // For now, just clear the screen (GeometryPass and UIPass will come later)
        // End rendering
        unsafe {
            self.renderer
                .context
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
            self.renderer.context.device.cmd_pipeline_barrier(
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
            self.renderer
                .context
                .device
                .end_command_buffer(command_buffer.vk_command_buffer())
                .expect("Failed to end command buffer");
        }

        // Submit command buffer to GPU
        self.submit_command_buffer(frame_index, image_index);

        // Present to swapchain
        self.present_swapchain(image_index);

        // Step to next frame
        self.renderer.swap_data.step_frame();
    }

    /// Submit command buffer to GPU queue.
    fn submit_command_buffer(&mut self, frame_index: usize, image_index: u32) {
        let command_buffer = &self.renderer.frame_context.command_buffers[frame_index];

        let wait_semaphore = self.renderer.swap_data.image_available_semaphore();
        let signal_semaphore = self
            .renderer
            .swap_data
            .render_finished_semaphore(image_index);
        let in_flight_fence = self.renderer.swap_data.in_flight_fence();

        // Reset fence
        unsafe {
            self.renderer
                .context
                .device
                .reset_fences(std::slice::from_ref(&in_flight_fence))
                .expect("Failed to reset fence");
        }

        // Submit command buffer
        self.renderer.context.gfx_queue.submit(
            &[command_buffer],
            &[wait_semaphore],
            &[signal_semaphore],
            in_flight_fence,
        );
    }

    /// Present the swapchain image to screen.
    fn present_swapchain(&self, image_index: u32) {
        let signal_semaphore = self
            .renderer
            .swap_data
            .render_finished_semaphore(image_index);

        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(std::slice::from_ref(&signal_semaphore))
            .swapchains(std::slice::from_ref(
                &self.renderer.frame_context.swapchain.swapchain,
            ))
            .image_indices(std::slice::from_ref(&image_index));

        unsafe {
            self.renderer
                .context
                .swapchain_loader
                .as_ref()
                .unwrap()
                .queue_present(self.renderer.context.gfx_queue.vk_queue(), &present_info)
                .expect("Failed to present swapchain");
        }
    }
}
