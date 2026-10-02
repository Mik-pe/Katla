//! Descriptor sets, immutable uploads and sampled views owned by one frame slot.

use std::rc::Rc;

use ash::vk;

use super::context::VulkanContext;
use super::descriptor_arena::DescriptorArena;
use crate::RendererError;
use crate::render_graph::transient_buffer::VulkanGraphBuffer;
use crate::render_graph::{BufferDesc, BufferMemoryPolicy, BufferUsages};

#[cfg(test)]
mod tests;

const UPLOAD_BLOCK_BYTES: u64 = 64 * 1024;

pub(crate) struct FrameResources {
    context: Rc<VulkanContext>,
    pub(crate) descriptors: DescriptorArena,
    uploads: Vec<UploadBlock>,
    current_upload: usize,
    image_views: Vec<vk::ImageView>,
    #[cfg(test)]
    uploaded_ranges: usize,
}

struct UploadBlock {
    buffer: VulkanGraphBuffer,
    used: u64,
}

impl FrameResources {
    pub(crate) fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            descriptors: DescriptorArena::new(context.gfx_cmdpool.owner.native.clone()),
            context,
            uploads: Vec::new(),
            current_upload: 0,
            image_views: Vec::new(),
            #[cfg(test)]
            uploaded_ranges: 0,
        }
    }

    /// Copy immutable bytes into a disjoint range retained until slot retirement.
    pub(crate) fn upload(
        &mut self,
        bytes: &[u8],
    ) -> Result<vk::DescriptorBufferInfo, RendererError> {
        if bytes.is_empty() {
            return Err(RendererError::InvalidOperation(
                "Frame upload must contain bytes".into(),
            ));
        }
        let size = bytes.len() as u64;
        let alignment = self
            .context
            .limits
            .min_uniform_buffer_offset_alignment
            .max(self.context.limits.min_storage_buffer_offset_alignment)
            .max(16);
        loop {
            if self.current_upload == self.uploads.len() {
                self.uploads.push(self.create_upload_block(size)?);
            }
            let block = &mut self.uploads[self.current_upload];
            let offset = block
                .used
                .checked_next_multiple_of(alignment)
                .ok_or_else(|| {
                    RendererError::InvalidOperation("Frame upload offset overflow".into())
                })?;
            let Some(end) = offset
                .checked_add(size)
                .filter(|end| *end <= block.buffer.size())
            else {
                self.current_upload += 1;
                continue;
            };
            block.buffer.write(offset, bytes)?;
            block.used = end;
            #[cfg(test)]
            {
                self.uploaded_ranges += 1;
            }
            return Ok(vk::DescriptorBufferInfo::default()
                .buffer(block.buffer.vk_buffer())
                .offset(offset)
                .range(size));
        }
    }

    fn create_upload_block(&self, required: u64) -> Result<UploadBlock, RendererError> {
        let desc = BufferDesc::new(
            required.max(UPLOAD_BLOCK_BYTES),
            BufferUsages::UNIFORM
                | BufferUsages::STORAGE
                | BufferUsages::VERTEX
                | BufferUsages::INDEX,
            BufferMemoryPolicy::CpuVisible,
        );
        let info = vk::BufferCreateInfo::default()
            .size(desc.size)
            .usage(crate::render_graph::vk_buffer_usages(desc.usages))
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (buffer, allocation) = self.context.allocate_buffer_named(
            &info,
            gpu_allocator::MemoryLocation::CpuToGpu,
            "Frame Upload",
        )?;
        Ok(UploadBlock {
            buffer: VulkanGraphBuffer::new(self.context.clone(), buffer, allocation, desc),
            used: 0,
        })
    }

    pub(crate) fn create_image_view(
        &mut self,
        info: &vk::ImageViewCreateInfo<'_>,
    ) -> Result<vk::ImageView, RendererError> {
        let view =
            unsafe { self.context.device.create_image_view(info, None) }.map_err(|error| {
                RendererError::VulkanError("Failed to create graphics image view".into(), error)
            })?;
        self.image_views.push(view);
        Ok(view)
    }

    /// Recycle storage after the slot's fence completes and its command buffer resets.
    pub(crate) fn reset(&mut self) -> Result<(), RendererError> {
        self.descriptors.reset()?;
        for block in &mut self.uploads {
            block.used = 0;
        }
        self.current_upload = 0;
        self.clear_image_views();
        #[cfg(test)]
        {
            self.uploaded_ranges = 0;
        }
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        self.descriptors.clear();
        self.uploads.clear();
        self.current_upload = 0;
        self.clear_image_views();
        #[cfg(test)]
        {
            self.uploaded_ranges = 0;
        }
    }

    fn clear_image_views(&mut self) {
        for view in self.image_views.drain(..) {
            unsafe { self.context.device.destroy_image_view(view, None) };
        }
    }

    #[cfg(test)]
    pub(crate) fn upload_block_count(&self) -> usize {
        self.uploads.len()
    }

    #[cfg(test)]
    pub(crate) fn uploaded_ranges(&self) -> usize {
        self.uploaded_ranges
    }
}

impl Drop for FrameResources {
    fn drop(&mut self) {
        self.clear_image_views();
    }
}
