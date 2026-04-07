use super::*;

impl VulkanRenderer {
    /// Queue an asynchronous readback of the last presented swapchain image.
    ///
    /// This is useful for detecting black frames and synchronization issues.
    /// The readback is asynchronous - use `check_pending_readback()` on the next frame
    /// to retrieve the results.
    ///
    /// # Arguments
    /// * `frame` - Current frame number for tracking
    ///
    /// # Returns
    /// * `Ok(())` - Readback was queued successfully
    /// * `Err(RendererError)` - Failed to queue readback
    ///
    /// # Async Behavior
    /// This function queues a GPU copy operation and returns immediately.
    /// The results will be available on the next frame via `check_pending_readback()`.
    /// This allows catching cross-frame synchronization issues that synchronous readback would mask.
    pub fn queue_async_readback(&mut self, frame: usize) -> Result<(), RendererError> {
        use ash::vk;

        // Get the last presented image index
        let image_index = if let Some(idx) = self.last_presented_image_index {
            idx
        } else {
            return Ok(()); // No frame presented yet
        };

        let swapchain_image = self.frame_context.swapchain_images[image_index as usize].vk();
        let extent = self.frame_context.swapchain.get_extent();
        let width = extent.width;
        let height = extent.height;

        // Create a staging buffer for readback
        let buffer_size = (width * height * 4) as vk::DeviceSize;
        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (staging_buffer, staging_allocation) = self
            .context
            .allocate_buffer(&buffer_info, gpu_allocator::MemoryLocation::CpuToGpu)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to allocate readback staging buffer: {}",
                    e
                ))
            })?;

        // Create a fence for this readback operation
        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe {
            self.context
                .device
                .create_fence(&fence_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!("Failed to create fence: {}", e))
                })?
        };

        // Create a command buffer for the copy operation
        let command_buffer = crate::vulkan::commandbuffer::CommandBuffer::new(
            &self.context.device,
            &crate::vulkan::commandpool::CommandPool {
                device: self.context.device.clone(),
                command_pool: self.context.transfer_command_pool,
            },
        );

        // Begin command buffer
        command_buffer.begin_single_time_command()?;

        // Transition swapchain image to TRANSFER_SRC optimal layout
        let barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        unsafe {
            self.context.device.cmd_pipeline_barrier(
                command_buffer.vk_command_buffer(),
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }

        // Copy image to staging buffer
        let buffer_image_copy = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        unsafe {
            self.context.device.cmd_copy_image_to_buffer(
                command_buffer.vk_command_buffer(),
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                staging_buffer,
                &[buffer_image_copy],
            );
        }

        // End and submit command buffer with fence (async!)
        command_buffer.end_single_time_command()?;

        unsafe {
            // Submit with fence for async completion
            let command_buffers = [command_buffer.vk_command_buffer()];
            let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

            self.context
                .device
                .queue_submit(self.context.gfx_queue.vk_queue(), &[submit_info], fence)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!("Failed to submit queue: {}", e))
                })?;
        }

        // Store pending readback for later retrieval
        self.pending_readback = Some(PendingReadback {
            frame,
            fence,
            command_buffer,
            staging_buffer,
            staging_allocation,
            buffer_size,
        });

        Ok(())
    }

    /// Check if the pending async readback is complete and return the data.
    ///
    /// # Returns
    /// * `Ok(Some((frame, data)))` - Readback complete, returns frame number and image data
    /// * `Ok(None)` - Readback not complete yet or no readback pending
    /// * `Err(RendererError)` - Readback failed
    pub fn check_pending_readback(&mut self) -> Result<Option<(usize, Vec<u8>)>, RendererError> {
        // Take ownership to avoid borrow issues
        if let Some(readback) = self.pending_readback.take() {
            unsafe {
                // Check if fence is signaled (readback complete)
                match self.context.device.get_fence_status(readback.fence) {
                    Ok(true) => {
                        // Fence signaled - readback is complete!
                        let mapped_ptr = self.context.map_buffer(&readback.staging_allocation)?;
                        let data =
                            std::slice::from_raw_parts(mapped_ptr, readback.buffer_size as usize);
                        let result = data.to_vec();
                        let frame = readback.frame;

                        // Cleanup - use CommandBuffer's return_to_pool method
                        readback.command_buffer.return_to_pool();
                        self.context.device.destroy_fence(readback.fence, None);
                        self.context
                            .free_buffer(readback.staging_buffer, readback.staging_allocation);

                        log::debug!("Frame {} readback complete", frame);
                        Ok(Some((frame, result)))
                    }
                    Ok(false) => {
                        // Still processing - put it back
                        log::debug!("Frame {} readback not ready yet", readback.frame);
                        self.pending_readback = Some(readback);
                        Ok(None)
                    }
                    Err(e) => {
                        // Error checking fence status - put it back
                        log::warn!("Failed to check fence for frame {}: {}", readback.frame, e);
                        self.pending_readback = Some(readback);
                        Err(RendererError::InitializationFailed(format!(
                            "Failed to check fence status: {}",
                            e
                        )))
                    }
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Wait for any pending async readback to complete and return the data.
    ///
    /// This is useful during shutdown to ensure all readbacks complete before
    /// destroying resources like the swapchain.
    ///
    /// # Returns
    /// * `Ok(Some((frame, data)))` - Readback was pending and is now complete
    /// * `Ok(None)` - No readback was pending
    /// * `Err(RendererError)` - Failed to wait for or complete readback
    pub fn wait_for_pending_readback(&mut self) -> Result<Option<(usize, Vec<u8>)>, RendererError> {
        if let Some(readback) = self.pending_readback.take() {
            unsafe {
                // Wait for the fence to signal
                log::debug!(
                    "Waiting for pending readback (frame {}) to complete",
                    readback.frame
                );
                let _ = self
                    .context
                    .device
                    .wait_for_fences(&[readback.fence], true, u64::MAX);

                // Fence signaled - readback is complete!
                let mapped_ptr = self
                    .context
                    .map_buffer(&readback.staging_allocation)
                    .map_err(|e| {
                        RendererError::InvalidOperation(format!(
                            "Failed to map readback buffer: {}",
                            e
                        ))
                    })?;
                let data = std::slice::from_raw_parts(mapped_ptr, readback.buffer_size as usize);
                let result = data.to_vec();
                let frame = readback.frame;

                // Cleanup - use CommandBuffer's return_to_pool method
                readback.command_buffer.return_to_pool();
                self.context.device.destroy_fence(readback.fence, None);
                self.context
                    .free_buffer(readback.staging_buffer, readback.staging_allocation);

                log::debug!("Pending readback (frame {}) complete", frame);
                Ok(Some((frame, result)))
            }
        } else {
            Ok(None)
        }
    }
}
