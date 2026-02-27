//! Frame rendering implementation.
//!
//! Contains frame acquisition, rendering, and presentation methods.

use ash::vk;
use log::debug;

use super::{DrawList, FrameData, VulkanRenderer};
use crate::RenderGraphError;

impl VulkanRenderer {
    /// Acquire the next swapchain image for rendering.
    pub fn swap_frames(&mut self) -> Result<(), RenderGraphError> {
        debug!("swap_frames: waiting for fence");
        self.swap_data.wait_for_fence(&self.context.device);
        debug!("swap_frames: fence waited");

        let (available_sem, finished_sem, in_flight_fence, image_index) =
            self.swap_data.swap_images(
                &self.context.device,
                self.context
                    .swapchain_loader
                    .as_ref()
                    .expect("Swapchain loader required"),
                self.frame_context.swapchain.swapchain,
            )?;
        debug!("swap_frames: got image_index={}", image_index);

        self.current_framedata = Some(FrameData {
            available_sem,
            finished_sem,
            in_flight_fence,
            image_index,
        });
        debug!("swap_frames: done");
        Ok(())
    }

    /// Render a frame using the render graph system.
    ///
    /// This is the main entry point for rendering. It:
    /// 1. Acquires a swapchain image
    /// 2. Executes all viewport render graphs
    /// 3. Executes the main render graph (UI + present)
    /// 4. Submits the command buffer and presents
    pub fn render_frame(&mut self, draw_list: DrawList) -> Result<(), RenderGraphError> {
        debug!("render_frame: start");

        // Acquire swapchain image
        if self.current_framedata.is_none() {
            debug!("render_frame: acquiring swapchain image");
            match self.swap_frames() {
                Ok(()) => {}
                Err(RenderGraphError::SwapchainOutOfDate) => {
                    self.recreate_swapchain();
                    return Err(RenderGraphError::SwapchainOutOfDate);
                }
                Err(e) => return Err(e),
            }
            debug!("render_frame: swapchain image acquired");
        }

        let frame_data = self
            .current_framedata
            .as_ref()
            .ok_or(RenderGraphError::NoFrameData)?;

        let image_index = frame_data.image_index as usize;
        debug!("render_frame: image_index={}", image_index);

        // Borrow the render graph
        let graph = self
            .render_graph
            .as_mut()
            .ok_or(RenderGraphError::CompilationError("No render graph".into()))?;
        debug!("render_frame: got render graph");

        // Update frame uniforms
        if let Some(ref frame) = self.frame_uniforms {
            self.storage_manager.update_from_frame_uniforms(frame);
        }
        debug!("render_frame: frame uniforms updated");

        // Set the draw list for this frame
        graph.set_draw_list(draw_list.clone());
        debug!("render_frame: draw list set");

        let mut command_buffer = self.frame_context.command_buffers[image_index].clone();
        command_buffer.begin_command(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        debug!("render_frame: command buffer begun");

        // Set frame index for UI buffer selection
        let frame_idx = self.swap_data.current_frame();
        debug!("render_frame: frame_idx={}", frame_idx);

        // === DISPATCH PARTICLE COMPUTE SHADERS ===
        if !draw_list.particle_dispatches.is_empty() {
            debug!(
                "render_frame: dispatching particle compute shaders (count={})",
                draw_list.particle_dispatches.len()
            );
            for (i, particle) in draw_list.particle_dispatches.iter().enumerate() {
                // Resolve handles to actual Vulkan objects
                let pipeline = match self.particle_pipelines.get(particle.pipeline.index()) {
                    Some(p) => p.vk(),
                    None => {
                        log::warn!("render_frame: particle {} has invalid pipeline handle", i);
                        continue;
                    }
                };
                let layout = match self.particle_layouts.get(particle.pipeline_layout.index()) {
                    Some(l) => l.vk(),
                    None => {
                        log::warn!("render_frame: particle {} has invalid layout handle", i);
                        continue;
                    }
                };
                let descriptor = match self
                    .particle_descriptors
                    .get(particle.descriptor_set.index())
                {
                    Some(d) => d.vk(),
                    None => {
                        log::warn!("render_frame: particle {} has invalid descriptor handle", i);
                        continue;
                    }
                };

                // Bind compute pipeline and descriptors
                command_buffer.bind_pipeline(pipeline, vk::PipelineBindPoint::COMPUTE);
                command_buffer.bind_descriptor_sets(
                    vk::PipelineBindPoint::COMPUTE,
                    layout,
                    &[descriptor],
                );

                // Push constants and dispatch
                unsafe {
                    self.context.device.cmd_push_constants(
                        command_buffer.vk_command_buffer(),
                        layout,
                        vk::ShaderStageFlags::COMPUTE,
                        0,
                        bytemuck::cast_slice(&particle.frame_data),
                    );
                    self.context.device.cmd_dispatch(
                        command_buffer.vk_command_buffer(),
                        particle.workgroup_count,
                        1,
                        1,
                    );
                }
                debug!("render_frame: particle {} dispatched", i);
            }

            // Barrier: compute write -> vertex read
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
            debug!("render_frame: compute barrier inserted");
        }

        // === EXECUTE ALL VIEWPORT RENDER GRAPHS ===
        for (idx, viewport) in self.viewports.iter_mut().enumerate() {
            if viewport.render_graph.is_none() {
                continue;
            }

            let has_draw_list = viewport.draw_list_cell.borrow().is_some();
            if !has_draw_list {
                continue;
            }

            debug!("render_frame: executing viewport {} render graph", idx);

            if let Some(ref frame) = viewport.frame_uniforms {
                if let Some(ref mut manager) = viewport.storage_manager {
                    manager.update_from_frame_uniforms(frame);
                }
            }

            if let Some(ref mut viewport_graph) = viewport.render_graph {
                viewport_graph.execute_no_swapchain(&mut command_buffer, frame_idx)?;
                debug!("render_frame: viewport {} graph.execute complete", idx);

                viewport
                    .transition_to_sample(command_buffer.vk_command_buffer(), &self.context.device);
                debug!("render_frame: viewport {} texture transitioned", idx);
            }
        }

        // === EXECUTE MAIN RENDER GRAPH ===
        debug!("render_frame: executing main render graph (UI + present)");
        graph.update_swapchain_image(
            self.frame_context.swapchain_images[image_index],
            self.frame_context.swapchain_image_views[image_index],
        );
        debug!("render_frame: calling graph.execute");
        graph.execute(
            &mut command_buffer,
            image_index,
            &self.frame_context.swapchain_images,
            self.frame_context.depth_render_texture.image,
            frame_idx,
        )?;
        debug!("render_frame: graph.execute complete");

        command_buffer.end_command();

        // Submit and present
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

        if let Err(e) = present_result {
            if e == vk::Result::ERROR_OUT_OF_DATE_KHR || e == vk::Result::SUBOPTIMAL_KHR {
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
