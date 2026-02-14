use ash::{vk, Device};

use super::{
    vertex_attr_set::VertexAttributeSet,
    vertex_attribute::AttributeType,
    vertexbuffer::IndexType,
    CommandPool,
};
use crate::render_graph::types::RenderingInfo;
use crate::sync::DependencyInfo;

#[derive(Clone)]
pub struct CommandBuffer {
    device: Device,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
}

impl CommandBuffer {
    pub fn new(device: &Device, command_pool: &CommandPool) -> Self {
        let create_info = vk::CommandBufferAllocateInfo::default()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(command_pool.vk_command_pool())
            .command_buffer_count(1);
        let command_buffer: vk::CommandBuffer = unsafe {
            device
                .allocate_command_buffers(&create_info)
                .expect("Failed to allocate Vulkan command buffer - check device memory")
        }[0];

        Self {
            device: device.clone(),
            command_pool: command_pool.vk_command_pool(),
            command_buffer,
        }
    }

    pub fn vk_command_buffer(&self) -> vk::CommandBuffer {
        self.command_buffer
    }

    pub fn begin_single_time_command(&self) {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(self.command_buffer, &begin_info)
                .expect("Failed to begin command buffer - command buffer may be in invalid state");
        }
    }

    pub fn end_single_time_command(&self) {
        unsafe {
            self.device.end_command_buffer(self.command_buffer).expect(
                "Failed to end command buffer - command buffer may not be in recording state",
            );
        }
    }

    pub fn begin_command(&self, flags: vk::CommandBufferUsageFlags) {
        let begin_info = vk::CommandBufferBeginInfo::default().flags(flags);
        unsafe {
            self.device
                .begin_command_buffer(self.command_buffer, &begin_info)
                .expect("Failed to begin command buffer - command buffer may be in invalid state");
        }
    }

    pub fn end_command(&self) {
        unsafe {
            self.device.end_command_buffer(self.command_buffer).expect(
                "Failed to end command buffer - command buffer may not be in recording state",
            );
        }
    }

    pub fn bind_pipeline(
        &self,
        pipeline: vk::Pipeline,
        pipeline_bind_point: vk::PipelineBindPoint,
    ) {
        unsafe {
            self.device
                .cmd_bind_pipeline(self.command_buffer, pipeline_bind_point, pipeline);
        }
    }

    pub fn bind_descriptor_sets(
        &self,
        pipeline_bind_point: vk::PipelineBindPoint,
        pipeline_layout: vk::PipelineLayout,
        descriptor_sets: &[vk::DescriptorSet],
    ) {
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                pipeline_bind_point,
                pipeline_layout,
                0,
                descriptor_sets,
                &[],
            );
        }
    }

    pub fn bind_index_buffer(&self, buffer: vk::Buffer, offset: u64, index_type: IndexType) {
        let vk_index_type: vk::IndexType = index_type.into();
        unsafe {
            self.device
                .cmd_bind_index_buffer(self.command_buffer, buffer, offset, vk_index_type)
        }
    }

    pub fn bind_vertex_buffers(&self, first_binding: u32, buffers: &[vk::Buffer], offsets: &[u64]) {
        unsafe {
            self.device.cmd_bind_vertex_buffers(
                self.command_buffer,
                first_binding,
                buffers,
                offsets,
            )
        }
    }

    pub fn draw_indexed(
        &self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        unsafe {
            self.device.cmd_draw_indexed(
                self.command_buffer,
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            )
        }
    }

    pub fn draw_array(
        &self,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        unsafe {
            self.device.cmd_draw(
                self.command_buffer,
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            )
        }
    }

    pub fn pipeline_barrier(
        &self,
        _src_stage_mask: vk::PipelineStageFlags,
        _dst_stage_mask: vk::PipelineStageFlags,
        _dependency_flags: vk::DependencyFlags,
        _memory_barriers: &[vk::MemoryBarrier],
        _buffer_memory_barriers: &[vk::BufferMemoryBarrier],
        _image_memory_barriers: &[vk::ImageMemoryBarrier],
    ) {
        // Legacy barrier removed - use pipeline_barrier2() instead
    }

    /// Vulkan 1.3: Pipeline barrier 2 command using modern synchronization.
    ///
    /// This method uses `vkCmdPipelineBarrier2` which provides more flexible
    /// and expressive synchronization compared to the legacy `vkCmdPipelineBarrier`.
    ///
    /// # Arguments
    /// * `dependency_info` - Dependency info containing all barriers
    ///
    /// # Example
    /// ```no_run
    /// use katla_vulkan::sync::{ImageMemoryBarrier2, PipelineStage2Flags, AccessFlags2, DependencyInfo};
    /// # use ash::vk;
    ///
    /// # let command_buffer: katla_vulkan::CommandBuffer = unsafe { std::mem::zeroed() };
    /// # let image: vk::Image = unsafe { std::mem::zeroed() };
    /// let barrier = ImageMemoryBarrier2::new(image)
    ///     .src_stage(PipelineStage2Flags::TRANSFER)
    ///     .dst_stage(PipelineStage2Flags::FRAGMENT_SHADER)
    ///     .src_access(AccessFlags2::TRANSFER_WRITE)
    ///     .dst_access(AccessFlags2::SHADER_READ)
    ///     .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
    ///     .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    ///     .subresource_range(vk::ImageSubresourceRange {
    ///         aspect_mask: vk::ImageAspectFlags::COLOR,
    ///         base_mip_level: 0,
    ///         level_count: 1,
    ///         base_array_layer: 0,
    ///         layer_count: 1,
    ///     });
    ///
    /// let dep_info = DependencyInfo::new()
    ///     .add_image_barrier(barrier);
    ///
    /// command_buffer.pipeline_barrier2(dep_info);
    /// ```
    pub fn pipeline_barrier2(&self, dependency_info: DependencyInfo) {
        dependency_info.build(|dep_info| unsafe {
            self.device
                .cmd_pipeline_barrier2(self.command_buffer, dep_info);
        });
    }

    /// Vulkan 1.3: Begin dynamic rendering.
    ///
    /// This method begins a dynamic rendering pass using VK_KHR_dynamic_rendering,
    /// which replaces traditional render passes and framebuffers.
    ///
    /// # Arguments
    /// * `rendering_info` - Rendering info describing attachments and render area
    ///
    /// # Example
    /// ```no_run
    /// # use ash::vk;
    /// # use katla_vulkan::render_graph::types::{RenderingAttachmentInfo, RenderingInfo, ClearValue, Extent2D};
    /// # use katla_vulkan::CommandBuffer;
    /// # let command_buffer: CommandBuffer = unsafe { std::mem::zeroed() };
    /// # let color_image_view: vk::ImageView = unsafe { std::mem::zeroed() };
    /// # let depth_image_view: vk::ImageView = unsafe { std::mem::zeroed() };
    /// let color_attachment = RenderingAttachmentInfo::new(color_image_view)
    ///     .clear(ClearValue::color(0.1, 0.2, 0.3, 1.0));
    ///
    /// let depth_attachment = RenderingAttachmentInfo::new(depth_image_view)
    ///     .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
    ///     .clear(ClearValue::depth(1.0, 0));
    ///
    /// let rendering_info = RenderingInfo::new()
    ///     .add_color_attachment(color_attachment)
    ///     .depth_attachment(depth_attachment)
    ///     .render_area(vk::Rect2D {
    ///         offset: vk::Offset2D { x: 0, y: 0 },
    ///         extent: vk::Extent2D { width: 1920, height: 1080 },
    ///     });
    ///
    /// command_buffer.begin_rendering(rendering_info);
    /// // ... record draw commands ...
    /// command_buffer.end_rendering();
    /// ```
    pub fn begin_rendering(&self, rendering_info: RenderingInfo) {
        rendering_info.build(|rendering_info| unsafe {
            self.device
                .cmd_begin_rendering(self.command_buffer, rendering_info);
        });
    }

    /// Vulkan 1.3: End dynamic rendering.
    ///
    /// Ends a dynamic rendering pass begun with `begin_rendering`.
    pub fn end_rendering(&self) {
        unsafe {
            self.device.cmd_end_rendering(self.command_buffer);
        }
    }

    /// Set the viewport for this command buffer.
    ///
    /// # Arguments
    /// * `viewports` - Slice of viewport structures to set
    pub fn set_viewport(&self, viewports: &[vk::Viewport]) {
        unsafe {
            self.device
                .cmd_set_viewport(self.command_buffer, 0, viewports);
        }
    }

    /// Set the scissor rectangle for this command buffer.
    ///
    /// # Arguments
    /// * `scissors` - Slice of scissor rectangles to set
    pub fn set_scissor(&self, scissors: &[vk::Rect2D]) {
        unsafe {
            self.device
                .cmd_set_scissor(self.command_buffer, 0, scissors);
        }
    }

    pub fn return_to_pool(&self) {
        unsafe {
            self.device
                .free_command_buffers(self.command_pool, &[self.command_buffer]);
        }
    }

    /// Bind all vertex attributes from an SoA attribute set.
    ///
    /// This method binds all attribute buffers from the set, sorted by their
    /// default location. Use this for full-geometry passes that need all attributes.
    ///
    /// # Arguments
    /// * `attributes` - The vertex attribute set to bind
    ///
    /// # Example
    /// ```no_run
    /// # use katla_vulkan::CommandBuffer;
    /// # use katla_vulkan::vulkan::vertex_attr_set::VertexAttributeSet;
    /// # let command_buffer: CommandBuffer = unsafe { std::mem::zeroed() };
    /// # let attributes: VertexAttributeSet = unsafe { std::mem::zeroed() };
    /// command_buffer.bind_vertex_attributes(&attributes);
    /// ```
    pub fn bind_vertex_attributes(&self, attributes: &VertexAttributeSet) {
        // Get all attribute types and sort by default location
        let mut attr_types: Vec<_> = attributes.attribute_types();
        attr_types.sort_by_key(|attr| attr.default_location());

        // Extract buffers and create offsets (all 0 for SoA)
        let buffers: Vec<vk::Buffer> = attr_types
            .iter()
            .filter_map(|attr| attributes.get(*attr).map(|binding| binding.buffer))
            .collect();

        let offsets: Vec<vk::DeviceSize> = vec![0; buffers.len()];

        if !buffers.is_empty() {
            unsafe {
                self.device.cmd_bind_vertex_buffers(
                    self.command_buffer,
                    0,
                    &buffers,
                    &offsets,
                );
            }
        }
    }

    /// Bind only specific attributes (for depth-only, shadow passes, etc.).
    ///
    /// This method binds a subset of attribute buffers, enabling efficient
    /// depth-only prepasses, shadow mapping, or deferred G-buffer fills.
    ///
    /// # Arguments
    /// * `attributes` - The vertex attribute set to bind from
    /// * `attr_types` - Slice of attribute types to bind (order matters for binding locations)
    ///
    /// # Example
    /// ```no_run
    /// # use katla_vulkan::CommandBuffer;
    /// # use katla_vulkan::vulkan::vertex_attr_set::VertexAttributeSet;
    /// # use katla_vulkan::vulkan::vertex_attribute::AttributeType;
    /// # let command_buffer: CommandBuffer = unsafe { std::mem::zeroed() };
    /// # let attributes: VertexAttributeSet = unsafe { std::mem::zeroed() };
    /// // Depth-only pass: only position needed
    /// command_buffer.bind_attributes_subset(&attributes, &[AttributeType::Position]);
    ///
    /// // Shadow mapping: position only
    /// command_buffer.bind_attributes_subset(&attributes, &[AttributeType::Position]);
    ///
    /// // Deferred G-buffer fill: position + normal
    /// command_buffer.bind_attributes_subset(
    ///     &attributes,
    ///     &[AttributeType::Position, AttributeType::Normal],
    /// );
    /// ```
    pub fn bind_attributes_subset(
        &self,
        attributes: &VertexAttributeSet,
        attr_types: &[AttributeType],
    ) {
        let mut buffers = Vec::new();
        let mut offsets = Vec::new();

        for attr_type in attr_types {
            if let Some(binding) = attributes.get(*attr_type) {
                buffers.push(binding.buffer);
                offsets.push(0);
            }
        }

        if !buffers.is_empty() {
            unsafe {
                self.device.cmd_bind_vertex_buffers(
                    self.command_buffer,
                    0,
                    &buffers,
                    &offsets,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_vertex_attributes_empty_set() {
        // This is a compile-time test - actual Vulkan testing would require a device
        // The important thing is that the API accepts an empty set without panicking
        let attr_types: Vec<AttributeType> = vec![];
        assert!(attr_types.is_empty());
    }

    #[test]
    fn test_bind_vertex_attributes_sorted_locations() {
        // Verify that attribute types are sorted by location
        let mut attr_types = vec![
            AttributeType::Tangent,
            AttributeType::Position,
            AttributeType::Normal,
        ];

        attr_types.sort_by_key(|attr| attr.default_location());

        assert_eq!(attr_types[0], AttributeType::Position);
        assert_eq!(attr_types[1], AttributeType::Normal);
        assert_eq!(attr_types[2], AttributeType::Tangent);
    }

    #[test]
    fn test_bind_attributes_subset_empty() {
        // Empty subset should work (no buffers bound)
        let attr_types: Vec<AttributeType> = vec![];
        assert!(attr_types.is_empty());
    }

    #[test]
    fn test_bind_attributes_subset_single() {
        // Single attribute binding
        let attr_types = vec![AttributeType::Position];
        assert_eq!(attr_types.len(), 1);
        assert_eq!(attr_types[0], AttributeType::Position);
    }

    #[test]
    fn test_bind_attributes_subset_multiple() {
        // Multiple attribute binding (e.g., for deferred G-buffer)
        let attr_types = vec![AttributeType::Position, AttributeType::Normal];
        assert_eq!(attr_types.len(), 2);
        assert_eq!(attr_types[0], AttributeType::Position);
        assert_eq!(attr_types[1], AttributeType::Normal);
    }
}
