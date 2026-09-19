//! Frame-scoped lifecycle for the Vulkan backend.
//!
//! [`acquire_frame`] waits for a free frame slot (fence + retirement drain) and
//! acquires the next swapchain image when windowed. The returned [`FrameToken`]
//! owns the slot until [`GpuRenderer::present`] consumes it (submit + present) or
//! the frame is aborted (no submit, slot not advanced).

use ash::vk;

use crate::barrier::ImageBarrier;
use crate::error::RendererError;
use crate::render_graph::Frame;
use crate::renderer::VulkanRenderer;
use crate::renderer::frame_scope::{FrameAcquisition, FrameToken};
use crate::renderer::types::{DrawCall, DrawList, FrameUniforms, InstanceData};

/// Marker methods implementing the frame-scoped contract on Vulkan.
impl VulkanRenderer {
    pub(crate) fn frame_check(&self, frame: &FrameToken) -> Result<(), RendererError> {
        match &self.active_frame {
            Some(active) if active == frame => Ok(()),
            Some(_) => Err(RendererError::InvalidOperation(
                "stale frame token: the frame was already finished or superseded".into(),
            )),
            None => Err(RendererError::InvalidOperation(
                "no frame is currently acquired; call acquire_frame first".into(),
            )),
        }
    }

    /// Abort any still-open frame, either explicitly (caller aborts) or
    /// implicitly (a new frame is acquired while this one is still open).
    pub(crate) fn frame_clear(&mut self) {
        if self.active_frame.take().is_some() {
            log::debug!(
                "Vulkan frame aborted without present (slot {})",
                self.current_frame()
            );
        }
        self.frame_poisoned = None;
    }

    /// Wait for the current frame slot's previous GPU submission to complete.
    ///
    /// Called by `acquire_frame` before any CPU writes to per-frame resources.
    /// This slot's previous submission completing also retires every resource
    /// replaced at least FRAMES_IN_FLIGHT frames ago; bindless slots freed here
    /// return to the free list only now, so no new texture can resolve through
    /// a slot an older submission still references.
    pub(crate) fn wait_for_frame(&mut self) -> Result<(), RendererError> {
        self.swap_data.wait_for_fence(&self.context.device)?;
        let expired_slots = self.retirements.drain_completed(
            self.swap_data.frame_counter(),
            self.swap_data.frames_in_flight(),
        );
        for slot in expired_slots {
            self.bindless_manager.release_texture_slot(slot);
        }
        // Release staged mesh uploads whose copy submissions finished.
        self.context.drain_completed_staged_uploads();
        Ok(())
    }

    /// Set frame-level uniforms for the current frame slot.
    pub(crate) fn set_frame_uniforms(&mut self, mut uniforms: FrameUniforms) {
        // Get frame index from swap_data (the source of truth for frame advancement)
        let frame_idx = self.swap_data.current_frame();

        // Inject depth texture bindless index into light_intensity.y for screen-space effects
        if let Some(depth_base) = self.depth_texture_base_index {
            uniforms.light_intensity = [
                uniforms.light_intensity[0],
                (depth_base + frame_idx as u32) as f32,
                uniforms.light_intensity[2],
                uniforms.light_intensity[3],
            ];
        }

        // Write frame uniforms to storage buffer for current frame
        self.storage_manager
            .update_from_frame_uniforms(frame_idx, &uniforms);

        // Store for reference
        self.frame_uniforms = uniforms;
    }

    /// Get the current frame uniforms (view/proj matrices, camera, lighting).
    pub fn frame_uniforms(&self) -> &FrameUniforms {
        &self.frame_uniforms
    }

    /// Write all per-object data from draw calls to this slot's storage buffer.
    pub(crate) fn execute_draw_calls(&mut self, draw_list: &DrawList) -> Result<(), RendererError> {
        let frame_idx = self.current_frame();

        for draw_call in &draw_list.draws {
            let base = draw_call.instance_index as usize;
            let count = draw_call.instance_count().max(1) as usize;

            if base + count > super::MAX_OBJECTS_PER_FRAME as usize {
                return Err(RendererError::ObjectLimitExceeded {
                    index: base,
                    limit: super::MAX_OBJECTS_PER_FRAME as usize,
                });
            }

            // Material parameters and texture indices are shared by all
            // instances; handles resolve to slots (with per-role fallback)
            // here, right before the upload. Emission resolves to 0 for
            // NONE/stale handles, keeping the shader's no-emission sentinel.
            let emission_idx = self.resolve_emission_texture_slot(draw_call.emission) as f32;
            let texture_indices = self.resolve_material_texture_slots(draw_call.material);

            for (i, instance) in draw_call
                .instances
                .iter()
                .chain(std::iter::repeat(&InstanceData::default()))
                .take(count)
                .enumerate()
            {
                self.storage_manager.update_object_bindless(
                    frame_idx,
                    base + i,
                    &crate::vulkan::material::storage_uniform::ObjectBindlessParams {
                        model: &instance.model_matrix,
                        color: &instance.color,
                        metallic: instance.metallic,
                        roughness: instance.roughness,
                        ao: instance.ao,
                        emission_idx,
                        texture_indices,
                    },
                );
            }
        }
        Ok(())
    }

    /// Simple immediate mode draw: set uniforms + write draw calls, return the DrawList.
    pub(crate) fn draw(
        &mut self,
        uniforms: &FrameUniforms,
        draw_calls: &[DrawCall],
    ) -> Result<DrawList, RendererError> {
        self.set_frame_uniforms(uniforms.clone());

        let mut draw_list = DrawList::new();
        for draw in draw_calls {
            draw_list.push(draw.clone());
        }

        self.execute_draw_calls(&draw_list)?;

        Ok(draw_list)
    }

    /// Execute the frame graph for an open frame: begin the command buffer,
    /// record all passes, transition the swapchain image for present.
    ///
    /// A failure poisons the frame so `present` cannot submit half-encoded work.
    pub fn render<F>(
        &mut self,
        frame: &FrameToken,
        frame_graph: &mut crate::render_graph::FrameGraph<VulkanRenderer>,
        f: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(&mut Frame<'_, VulkanRenderer>),
    {
        self.frame_check(frame)?;
        if let Some(reason) = &self.frame_poisoned {
            return Err(RendererError::InvalidOperation(format!(
                "frame is poisoned by a previous render failure and cannot render again: {reason}"
            )));
        }

        let headless = self.frame_context.swapchain.is_none();
        let image_index = self
            .last_presented_image_index
            .unwrap_or(frame.slot() as u32);
        let frame_idx = self.current_frame();
        let cmd = self.frame_context.command_buffers[frame_idx].vk_command_buffer();

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.context
                .device
                .begin_command_buffer(cmd, &begin_info)
                .map_err(|e| {
                    let error =
                        RendererError::VulkanError("Failed to begin command buffer".into(), e);
                    self.frame_poisoned = Some(format!("{error:?}"));
                    error
                })?;
        }

        // Transition the surface image to COLOR_ATTACHMENT_OPTIMAL for rendering.
        // Use transition_from_undefined because after acquire the actual layout
        // is platform-specific and load_op=CLEAR discards contents. Headless
        // targets take the same path (their readback expects the final
        // TRANSFER_SRC_OPTIMAL below).
        let swapchain_image = self.frame_context.swapchain_images[image_index as usize].vk();
        ImageBarrier::transition_from_undefined(
            &cmd,
            &self.context.device,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        );

        if let Err(e) = frame_graph.execute(self, image_index, f) {
            let error = RendererError::RenderGraphError(e);
            self.frame_poisoned = Some(format!("{error:?}"));
            return Err(error);
        }

        let swapchain_image = self.frame_context.swapchain_images[image_index as usize].vk();
        ImageBarrier::transition(
            &cmd,
            &self.context.device,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            if headless {
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL
            } else {
                vk::ImageLayout::PRESENT_SRC_KHR
            },
        );

        unsafe {
            self.context.device.end_command_buffer(cmd).map_err(|e| {
                let error = RendererError::VulkanError("Failed to end command buffer".into(), e);
                self.frame_poisoned = Some(format!("{error:?}"));
                error
            })?;
        }

        Ok(())
    }

    /// Present implementation: submit the recorded work and present. Consumes
    /// the open frame; the slot advances and becomes busy until a later
    /// acquire waits for its fence.
    pub(crate) fn present_frame(&mut self, frame: FrameToken) -> Result<(), RendererError> {
        self.frame_check(&frame)?;
        if let Some(reason) = self.frame_poisoned.take() {
            self.active_frame = None;
            return Err(RendererError::InvalidOperation(format!(
                "frame is poisoned by a previous render failure: {reason}"
            )));
        }
        self.active_frame = None;

        let headless = self.frame_context.swapchain.is_none();
        let image_index = self
            .last_presented_image_index
            .unwrap_or(frame.slot() as u32);
        let frame_idx = self.current_frame();

        unsafe {
            self.context
                .device
                .reset_fences(&[self.swap_data.in_flight_fence()])
        }
        .map_err(|e| RendererError::VulkanError("Failed to reset frame fence".into(), e))?;

        if headless {
            self.context.gfx_queue.submit(
                &[&self.frame_context.command_buffers[frame_idx]],
                &[],
                &[],
                self.swap_data.in_flight_fence(),
            );
            self.swap_data.wait_for_fence(&self.context.device)?;
            self.swap_data.step_frame();
            return Ok(());
        }

        let swapchain = self
            .frame_context
            .swapchain
            .as_ref()
            .expect("window swapchain");
        let render_finished_semaphore = self.swap_data.render_finished_semaphore(image_index);
        let frame_complete_semaphore = self.swap_data.frame_complete_semaphore();
        let signal_semaphores = [render_finished_semaphore, frame_complete_semaphore];
        let swapchains = [swapchain.swapchain];
        let image_indices = [image_index];

        // On the first frame there's no previous frame to wait on.
        // After that, wait on the previous frame's completion semaphore at ALL_COMMANDS
        // to cover TRANSFER/CLEAR from vkCmdUpdateBuffer and TRANSFER_READ from vkCmdCopyBuffer.
        if self.first_frame_rendered {
            let wait_semaphores = [
                self.swap_data.image_available_semaphore(),
                self.swap_data.previous_frame_complete_semaphore(),
            ];
            let wait_stage_masks = [
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::ALL_COMMANDS,
            ];
            self.context.gfx_queue.submit_with_stages(
                &[&self.frame_context.command_buffers[frame_idx]],
                &wait_semaphores,
                &signal_semaphores,
                self.swap_data.in_flight_fence(),
                &wait_stage_masks,
            );
        } else {
            self.first_frame_rendered = true;
            let wait_semaphores = [self.swap_data.image_available_semaphore()];
            self.context.gfx_queue.submit(
                &[&self.frame_context.command_buffers[frame_idx]],
                &wait_semaphores,
                &signal_semaphores,
                self.swap_data.in_flight_fence(),
            );
        }

        let present_wait_semaphores = [render_finished_semaphore];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&present_wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        unsafe {
            let present_result = swapchain
                .swapchain_loader
                .queue_present(self.context.gfx_queue.vk_queue(), &present_info);

            match present_result {
                Ok(is_suboptimal) => {
                    // Suboptimal is very common on macOS/MoltenVK (especially first frame).
                    // Still rendered successfully, but signal that swapchain should be recreated.
                    if is_suboptimal {
                        log::debug!("Present suboptimal, signaling swapchain recreation");
                        self.swap_data.step_frame();
                        return Err(RendererError::SwapchainOutOfDate);
                    }
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    // Frame was presented but swapchain is stale, signal recreation.
                    log::debug!("Present out of date, signaling swapchain recreation");
                    self.swap_data.step_frame();
                    return Err(RendererError::SwapchainOutOfDate);
                }
                Err(e) => {
                    return Err(RendererError::SwapchainError(format!(
                        "Failed to present: {:?}",
                        e
                    )));
                }
            }
        }

        self.swap_data.step_frame();
        Ok(())
    }
}

/// Wait for a free frame slot and acquire the next surface image.
///
/// Windowed, `ERROR_OUT_OF_DATE_KHR` at acquire maps to [`FrameAcquisition::OutOfDate`]
/// with no renderer state touched; the caller recreates the swapchain and retries.
/// Headless renderers always have a free slot after the fence wait.
pub(crate) fn acquire_frame(
    renderer: &mut VulkanRenderer,
) -> Result<FrameAcquisition, RendererError> {
    // An unfinished frame from an earlier acquisition is abandoned here.
    renderer.frame_clear();
    renderer.wait_for_frame()?;

    match renderer.frame_context.swapchain.as_ref() {
        Some(swapchain) => {
            let acquire_result = unsafe {
                swapchain.swapchain_loader.acquire_next_image(
                    swapchain.swapchain,
                    u64::MAX,
                    renderer.swap_data.image_available_semaphore(),
                    vk::Fence::null(),
                )
            };
            match acquire_result {
                Ok((image_index, is_suboptimal)) => {
                    if is_suboptimal {
                        log::debug!("Swapchain suboptimal at acquire, will recreate after present");
                    }
                    // Store image index for readback debugging
                    renderer.last_presented_image_index = Some(image_index);
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    log::info!("Swapchain out of date at acquire, signaling recreation");
                    return Ok(FrameAcquisition::OutOfDate);
                }
                Err(e) => {
                    return Err(RendererError::SwapchainError(format!(
                        "Failed to acquire swapchain image: {:?}",
                        e
                    )));
                }
            }
        }
        None => renderer.last_presented_image_index = Some(renderer.current_frame() as u32),
    }

    let token = FrameToken::new(renderer.current_frame(), renderer.frame_generation);
    renderer.frame_generation += 1;
    renderer.active_frame = Some(token);
    Ok(FrameAcquisition::Ready(token))
}

/// Abort implementation: nothing is submitted or presented, the slot is not
/// advanced, and the next acquire waits for it as usual.
pub(crate) fn abort_frame(renderer: &mut VulkanRenderer, frame: FrameToken) {
    if renderer.frame_check(&frame).is_ok() {
        renderer.frame_clear();
    }
}
