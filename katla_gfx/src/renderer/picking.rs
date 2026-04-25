use crate::RendererError;
use crate::barrier::ImageBarrier;
use crate::vulkan::context::VulkanContext;
use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::rc::Rc;

/// Pending picking readback operation.
pub struct PickingReadback {
    /// The frame number when the pick was triggered.
    pub frame: usize,
    /// Fence for GPU completion.
    pub fence: vk::Fence,
    /// Command buffer used for the copy.
    pub command_buffer: crate::vulkan::commandbuffer::CommandBuffer,
    /// Staging buffer for the 4-byte pixel readback.
    pub staging_buffer: vk::Buffer,
    /// Staging buffer allocation.
    pub staging_allocation: Allocation,
}

#[derive(Default)]
/// Owns all picking readback state.
///
/// Lifecycle:
/// - No explicit `init()` needed (state is created lazily on first pick).
/// - `destroy()` — cleans up any pending readback resources.
pub(crate) struct PickingSubsystem {
    /// Pending picking readback operation, if any.
    pending_picking_readback: Option<PickingReadback>,
}

impl PickingSubsystem {
    /// Queue a picking readback for a specific pixel in the object-ID texture.
    ///
    /// Copies a single 4-byte pixel from the object-ID texture at (x, y) to a
    /// staging buffer. The result is available on the next frame via `check_picking_readback()`.
    ///
    /// # Arguments
    /// * `context` - Vulkan context for GPU operations
    /// * `frame` - Current frame number for tracking
    /// * `object_id_image` - The Vulkan image containing object IDs (R32Uint)
    /// * `current_layout` - The current layout of the object_id image (must match GPU state)
    /// * `x` - Pixel x coordinate (physical pixels)
    /// * `y` - Pixel y coordinate (physical pixels)
    pub fn queue_picking_readback(
        &mut self,
        context: &Rc<VulkanContext>,
        frame: usize,
        object_id_image: vk::Image,
        current_layout: vk::ImageLayout,
        x: u32,
        y: u32,
    ) -> Result<(), RendererError> {
        let buffer_size = 4u64; // single u32 pixel

        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (staging_buffer, staging_allocation) = context
            .allocate_buffer(&buffer_info, gpu_allocator::MemoryLocation::CpuToGpu)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to allocate picking staging buffer: {}",
                    e
                ))
            })?;

        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe {
            context
                .device
                .create_fence(&fence_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create picking fence: {}",
                        e
                    ))
                })?
        };

        let command_buffer =
            crate::vulkan::commandbuffer::CommandBuffer::new(&context.device, &context.gfx_cmdpool);

        command_buffer.begin_single_time_command()?;

        let cmd_vk = command_buffer.vk_command_buffer();

        // Transition object-ID image from its current layout to TRANSFER_SRC
        ImageBarrier::transition(
            &cmd_vk,
            &context.device,
            object_id_image,
            current_layout,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );

        // Copy single pixel to staging buffer
        let buffer_image_copy = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D {
                x: x as i32,
                y: y as i32,
                z: 0,
            })
            .image_extent(vk::Extent3D {
                width: 1,
                height: 1,
                depth: 1,
            });

        unsafe {
            context.device.cmd_copy_image_to_buffer(
                command_buffer.vk_command_buffer(),
                object_id_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                staging_buffer,
                &[buffer_image_copy],
            );
        }

        // Transition back to the original layout for the next frame's pre-barriers
        ImageBarrier::transition(
            &cmd_vk,
            &context.device,
            object_id_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            current_layout,
        );

        command_buffer.end_single_time_command()?;

        unsafe {
            let command_buffers = [command_buffer.vk_command_buffer()];
            let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

            context
                .device
                .queue_submit(context.gfx_queue.vk_queue(), &[submit_info], fence)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to submit picking readback: {}",
                        e
                    ))
                })?;
        }

        // Store pending readback
        self.pending_picking_readback = Some(PickingReadback {
            frame,
            fence,
            command_buffer,
            staging_buffer,
            staging_allocation,
        });

        Ok(())
    }

    /// Check if the pending picking readback is complete.
    ///
    /// Returns `Ok(Some((frame, instance_index)))` where instance_index is 1-based
    /// (0 = no object, background was clicked).
    /// Returns `Ok(None)` if no readback is pending or it's not ready yet.
    pub fn check_picking_readback(
        &mut self,
        context: &Rc<VulkanContext>,
    ) -> Result<Option<(usize, u32)>, RendererError> {
        if let Some(readback) = self.pending_picking_readback.take() {
            let frame = readback.frame;
            unsafe {
                match context.device.get_fence_status(readback.fence) {
                    Ok(true) => {
                        context.invalidate_mapped_memory(&readback.staging_allocation, 0, 4)?;
                        let mapped_ptr = context.map_buffer(&readback.staging_allocation)?;
                        let data = std::ptr::read(mapped_ptr as *const u32);

                        readback.command_buffer.return_to_pool();
                        context.device.destroy_fence(readback.fence, None);
                        context.free_buffer(readback.staging_buffer, readback.staging_allocation);

                        // Return data as-is; 0 means background/no object.
                        Ok(Some((frame, data)))
                    }
                    Ok(false) => {
                        // Not ready yet — put it back
                        self.pending_picking_readback = Some(readback);
                        Ok(None)
                    }
                    Err(e) => {
                        // Error — put it back
                        self.pending_picking_readback = Some(readback);
                        Err(RendererError::InitializationFailed(format!(
                            "Failed to check picking fence: {}",
                            e
                        )))
                    }
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Wait for the pending picking readback to complete (blocking).
    pub fn wait_for_picking_readback(
        &mut self,
        context: &Rc<VulkanContext>,
    ) -> Result<Option<(usize, u32)>, RendererError> {
        if let Some(readback) = self.pending_picking_readback.take() {
            let frame = readback.frame;
            unsafe {
                let _ = context
                    .device
                    .wait_for_fences(&[readback.fence], true, u64::MAX);

                context.invalidate_mapped_memory(&readback.staging_allocation, 0, 4)?;

                let mapped_ptr = context
                    .map_buffer(&readback.staging_allocation)
                    .map_err(|e| {
                        RendererError::InvalidOperation(format!(
                            "Failed to map picking buffer: {}",
                            e
                        ))
                    })?;
                let data = std::ptr::read(mapped_ptr as *const u32);

                readback.command_buffer.return_to_pool();
                context.device.destroy_fence(readback.fence, None);
                context.free_buffer(readback.staging_buffer, readback.staging_allocation);

                Ok(Some((frame, data)))
            }
        } else {
            Ok(None)
        }
    }

    /// Check if a picking readback is currently pending.
    pub fn has_pending_picking_readback(&self) -> bool {
        self.pending_picking_readback.is_some()
    }

    /// Destroy picking subsystem resources.
    ///
    /// Cleans up any pending readback (fence, command buffer, staging buffer).
    pub fn destroy(&mut self, context: &Rc<VulkanContext>) {
        if let Some(readback) = self.pending_picking_readback.take() {
            unsafe {
                let _ = context
                    .device
                    .wait_for_fences(&[readback.fence], true, u64::MAX);
                readback.command_buffer.return_to_pool();
                context.device.destroy_fence(readback.fence, None);
                context.free_buffer(readback.staging_buffer, readback.staging_allocation);
            }
        }
    }
}

impl super::VulkanRenderer {
    /// Queue a picking readback for a specific pixel in the object-ID texture.
    ///
    /// Copies a single 4-byte pixel from the object-ID texture at (x, y) to a
    /// staging buffer. The result is available on the next frame via `check_picking_readback()`.
    ///
    /// # Arguments
    /// * `frame` - Current frame number for tracking
    /// * `object_id_image` - The Vulkan image containing object IDs (R32Uint)
    /// * `current_layout` - The current layout of the object_id image (must match GPU state)
    /// * `x` - Pixel x coordinate (physical pixels)
    /// * `y` - Pixel y coordinate (physical pixels)
    pub fn queue_picking_readback(
        &mut self,
        frame: usize,
        object_id_image: vk::Image,
        current_layout: vk::ImageLayout,
        x: u32,
        y: u32,
    ) -> Result<(), RendererError> {
        self.picking.queue_picking_readback(
            &self.context,
            frame,
            object_id_image,
            current_layout,
            x,
            y,
        )
    }

    /// Check if the pending picking readback is complete.
    ///
    /// Returns `Ok(Some((frame, instance_index)))` where instance_index is 1-based
    /// (0 = no object, background was clicked).
    /// Returns `Ok(None)` if no readback is pending or it's not ready yet.
    pub fn check_picking_readback(&mut self) -> Result<Option<(usize, u32)>, RendererError> {
        self.picking.check_picking_readback(&self.context)
    }

    /// Wait for the pending picking readback to complete (blocking).
    pub fn wait_for_picking_readback(&mut self) -> Result<Option<(usize, u32)>, RendererError> {
        self.picking.wait_for_picking_readback(&self.context)
    }

    /// Check if a picking readback is currently pending.
    pub fn has_pending_picking_readback(&self) -> bool {
        self.picking.has_pending_picking_readback()
    }
}
