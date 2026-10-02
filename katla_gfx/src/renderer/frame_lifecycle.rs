//! Frame-scoped lifecycle for the Vulkan backend.
//!
//! [`acquire_frame`] waits for a free frame slot (fence + retirement drain) and
//! acquires the next swapchain image when windowed. The returned [`FrameToken`]
//! owns the slot until [`GpuRenderer::present`] consumes it (submit + present) or
//! the frame is aborted (no submit, slot not advanced).

use ash::vk;

use crate::error::RendererError;
use crate::render_graph::Frame;
use crate::renderer::VulkanRenderer;
use crate::renderer::frame_scope::{FrameAcquisition, FrameToken, PresentOutcome, SurfaceStatus};
use crate::renderer::types::{DrawList, InstanceData};

/// Marker methods implementing the frame-scoped contract on Vulkan.
impl VulkanRenderer {
    fn commit_graph_buffer_consumers(&mut self) {
        self.commit_texture_exports();
        let fence = self.swap_data.in_flight_fence();
        self.last_submission = Some((
            self.current_frame(),
            self.frame_generation.saturating_sub(1),
            Some(fence),
        ));
        for buffer in self.pending_graph_buffers.drain() {
            self.graph_buffer_consumers.insert(buffer, Some(fence));
        }
    }

    fn retire_buffer_consumers(&mut self, fence: vk::Fence) {
        for owner in self.graph_buffer_consumers.values_mut() {
            if *owner == Some(fence) {
                *owner = None;
            }
        }
        if let Some((_, _, owner)) = &mut self.last_submission
            && *owner == Some(fence)
        {
            *owner = None;
        }
    }

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

    pub(crate) fn frame_write_check(&self, frame: &FrameToken) -> Result<(), RendererError> {
        self.frame_check(frame)?;
        if self.frame_rendered {
            return Err(RendererError::InvalidOperation(
                "Frame-local writes must precede render".into(),
            ));
        }
        Ok(())
    }

    /// Abort any still-open frame, either explicitly (caller aborts) or
    /// implicitly (a new frame is acquired while this one is still open).
    /// A windowed acquisition stays reserved until a successful submission
    /// consumes its semaphore, so retrying reuses the same surface image.
    pub(crate) fn frame_clear(&mut self) {
        if self.active_frame.take().is_some() {
            log::debug!(
                "Vulkan frame aborted without present (slot {})",
                self.current_frame()
            );
        }
        self.pending_texture_exports.clear();
        self.pending_graph_buffers.clear();
        self.frame_rendered = false;
        self.frame_poisoned = None;
        self.frame_context.pending_output_contents.set(None);
        self.frame_context
            .pending_transient_layouts
            .borrow_mut()
            .rollback();
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
        let fence = self.swap_data.in_flight_fence();
        self.retire_buffer_consumers(fence);
        let slot = self.current_frame();
        self.graphics_descriptor_sets[slot].clear();
        self.graphics_constants[slot].clear();
        for view in self.graphics_image_views[slot].drain(..) {
            unsafe {
                self.context.device.destroy_image_view(view, None);
            }
        }
        let expired_slots = self.retirements.drain_completed(
            self.swap_data.frame_counter(),
            self.swap_data.frames_in_flight(),
        );
        for slot in expired_slots {
            self.bindless_manager.release_texture_slot(slot);
        }
        // Release staged mesh uploads whose copy submissions finished.
        self.context.drain_completed_submissions();
        Ok(())
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
        if self.frame_rendered {
            return Err(RendererError::InvalidOperation(
                "Frame graph already rendered for this acquired token".into(),
            ));
        }
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

        frame_graph.set_backbuffer_final_state(if headless {
            crate::render_graph::ResourceState::TransferSrc
        } else {
            crate::render_graph::ResourceState::PresentSrc
        });

        if let Err(e) = frame_graph.execute(self, image_index, f) {
            let error = RendererError::RenderGraphError(e);
            self.frame_poisoned = Some(format!("{error:?}"));
            return Err(error);
        }

        unsafe {
            self.context.device.end_command_buffer(cmd).map_err(|e| {
                let error = RendererError::VulkanError("Failed to end command buffer".into(), e);
                self.frame_poisoned = Some(format!("{error:?}"));
                error
            })?;
        }

        self.prepare_texture_exports(frame_graph, image_index as usize);
        self.frame_rendered = true;
        Ok(())
    }

    fn commit_output_state(&self, image: usize, layout: vk::ImageLayout) {
        self.frame_context
            .pending_transient_layouts
            .borrow_mut()
            .commit();
        self.frame_context.swapchain_image_layouts[image].set(layout);
        if let Some((pending_image, contents)) = self.frame_context.pending_output_contents.take()
            && pending_image == image
        {
            self.frame_context.swapchain_image_contents[image].set(contents);
        }
    }

    /// Present implementation: submit the recorded work and present. Consumes
    /// the open frame; the slot advances and becomes busy until a later
    /// acquire waits for its fence.
    pub(crate) fn present_frame(
        &mut self,
        frame: FrameToken,
    ) -> Result<PresentOutcome, RendererError> {
        self.present_frame_impl(frame, None, None)
    }

    fn present_frame_impl(
        &mut self,
        frame: FrameToken,
        injected_submit_error: Option<vk::Result>,
        injected_surface_result: Option<Result<bool, vk::Result>>,
    ) -> Result<PresentOutcome, RendererError> {
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
        let slot = self.current_frame();
        let fence = self.swap_data.in_flight_fence();
        unsafe { self.context.device.reset_fences(&[fence]) }.map_err(|error| {
            RendererError::VulkanError("Failed to reset frame fence".into(), error)
        })?;

        let waits = (!headless).then(|| {
            (
                self.swap_data.image_available_semaphore(),
                vk::PipelineStageFlags2::ALL_COMMANDS,
            )
        });
        let signal = (!headless).then(|| self.swap_data.render_finished_semaphore(image_index));
        let submit = injected_submit_error.map_or_else(
            || {
                self.context.gfx_queue.submit(
                    &[&self.frame_context.command_buffers[slot]],
                    waits.as_slice(),
                    signal.as_slice(),
                    fence,
                )
            },
            |error| {
                Err(RendererError::VulkanError(
                    "Failed to submit frame".into(),
                    error,
                ))
            },
        );
        if let Err(error) = submit {
            self.retire_buffer_consumers(fence);
            self.frame_clear();
            return Err(error);
        }

        self.swap_data.mark_submitted();
        self.acquired_surface_image = None;
        self.commit_graph_buffer_consumers();
        self.commit_output_state(
            image_index as usize,
            if headless {
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL
            } else {
                vk::ImageLayout::PRESENT_SRC_KHR
            },
        );
        let surface = if headless {
            match self.swap_data.wait_for_fence(&self.context.device) {
                Ok(()) => {
                    self.retire_buffer_consumers(fence);
                    surface_status(injected_surface_result.unwrap_or(Ok(false)))
                }
                Err(error) => Err(error),
            }
        } else {
            let swapchain = self
                .frame_context
                .swapchain
                .as_ref()
                .expect("window swapchain");
            let signals = [self.swap_data.render_finished_semaphore(image_index)];
            let swapchains = [swapchain.swapchain];
            let indices = [image_index];
            let info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signals)
                .swapchains(&swapchains)
                .image_indices(&indices);
            surface_status(injected_surface_result.unwrap_or_else(|| unsafe {
                swapchain
                    .swapchain_loader
                    .queue_present(self.context.gfx_queue.vk_queue(), &info)
            }))
        };
        self.surface_recreation_required =
            !headless && !matches!(surface, Ok(SurfaceStatus::Presented));
        self.swap_data.step_frame();
        Ok(PresentOutcome { surface })
    }

    #[cfg(test)]
    pub(super) fn present_frame_injected(
        &mut self,
        frame: FrameToken,
        submit_error: Option<vk::Result>,
        surface_result: Option<Result<bool, vk::Result>>,
    ) -> Result<PresentOutcome, RendererError> {
        self.present_frame_impl(frame, submit_error, surface_result)
    }
}

fn surface_status(result: Result<bool, vk::Result>) -> Result<SurfaceStatus, RendererError> {
    match result {
        Ok(false) => Ok(SurfaceStatus::Presented),
        Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(SurfaceStatus::RecreateRequired),
        Err(error) => Err(RendererError::VulkanError(
            "Failed to present submitted frame".into(),
            error,
        )),
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
    if renderer.surface_recreation_required {
        return Ok(FrameAcquisition::OutOfDate);
    }
    // An unfinished frame from an earlier acquisition is abandoned here.
    renderer.frame_clear();
    renderer.wait_for_frame()?;

    match renderer.frame_context.swapchain.as_ref() {
        Some(swapchain) if renderer.acquired_surface_image.is_none() => {
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
                    renderer.acquired_surface_image = Some(image_index);
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
        Some(_) => {}
        None => renderer.last_presented_image_index = Some(renderer.current_frame() as u32),
    }

    let token = FrameToken::new(renderer.current_frame());
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
