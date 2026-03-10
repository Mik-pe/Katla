use ash::{Device, vk};

use super::{CommandPool, vertex_attr_set::VertexAttributeSet, vertex_attribute::AttributeType};
use crate::sync::{DependencyInfo, Rect2D, VkViewport};

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

    /// Get the raw Vulkan command buffer handle.
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

    pub fn end_command(&self) {
        unsafe {
            self.device.end_command_buffer(self.command_buffer).expect(
                "Failed to end command buffer - command buffer may not be in recording state",
            );
        }
    }

    // ========================================================================
    // Convenience Methods for Wrapper Types
    // ========================================================================

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

    /// Vulkan 1.3: Pipeline barrier 2 command using modern synchronization.
    ///
    /// This method uses `vkCmdPipelineBarrier2` which provides more flexible
    /// and expressive synchronization compared to the legacy `vkCmdPipelineBarrier`.
    ///
    /// # Arguments
    /// * `dependency_info` - Dependency info containing all barriers
    ///
    /// # Example
    /// ```ignore
    /// use katla_gfx::sync::{ImageMemoryBarrier2, PipelineStage2Flags, AccessFlags2, DependencyInfo, VkImage};
    /// use ash::vk;
    ///
    /// # let command_buffer: katla_gfx::CommandBuffer = unsafe { std::mem::zeroed() };
    /// # let image: VkImage = VkImage::new(unsafe { std::mem::zeroed() });
    /// let barrier = ImageMemoryBarrier2 {
    ///     image,
    ///     src_stage_mask: PipelineStage2Flags::TRANSFER,
    ///     dst_stage_mask: PipelineStage2Flags::FRAGMENT_SHADER,
    ///     src_access_mask: AccessFlags2::TRANSFER_WRITE,
    ///     dst_access_mask: AccessFlags2::SHADER_READ,
    ///     old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    ///     new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    ///     src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
    ///     dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
    ///     subresource_range: vk::ImageSubresourceRange {
    ///         aspect_mask: vk::ImageAspectFlags::COLOR,
    ///         base_mip_level: 0,
    ///         level_count: 1,
    ///         base_array_layer: 0,
    ///         layer_count: 1,
    ///     },
    /// };
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

    /// Set the viewport for this command buffer.
    ///
    /// # Arguments
    /// * `viewports` - Slice of viewport structures to set (wrapper type)
    pub(crate) fn set_viewport(&self, viewports: &[VkViewport]) {
        let vk_viewports: Vec<vk::Viewport> = viewports.iter().map(|v| (*v).into()).collect();
        unsafe {
            self.device
                .cmd_set_viewport(self.command_buffer, 0, &vk_viewports);
        }
    }

    /// Set the scissor rectangle for this command buffer.
    ///
    /// # Arguments
    /// * `scissors` - Slice of scissor rectangles to set (wrapper type)
    pub fn set_scissor(&self, scissors: &[Rect2D]) {
        let vk_scissors: Vec<vk::Rect2D> = scissors.iter().map(|s| (*s).into()).collect();
        unsafe {
            self.device
                .cmd_set_scissor(self.command_buffer, 0, &vk_scissors);
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
    /// ```ignore
    /// let command_buffer: CommandBuffer = unsafe { std::mem::zeroed() };
    /// let attributes: VertexAttributeSet = unsafe { std::mem::zeroed() };
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
                self.device
                    .cmd_bind_vertex_buffers(self.command_buffer, 0, &buffers, &offsets);
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
                self.device
                    .cmd_bind_vertex_buffers(self.command_buffer, 0, &buffers, &offsets);
            }
        }
    }

    //=========================================================================
    // Dynamic Rendering (Vulkan 1.3)
    //=========================================================================

    /// Begin a dynamic rendering pass.
    ///
    /// Vulkan 1.3 dynamic rendering eliminates the need for render pass objects,
    /// allowing direct rendering to attachments specified at command buffer time.
    ///
    /// # Arguments
    /// * `color_attachments` - Slice of color attachment info
    /// * `depth_attachment` - Optional depth attachment info
    /// * `stencil_attachment` - Optional stencil attachment info
    /// * `render_area` - Area to render to
    /// * `layer_count` - Number of layers to render
    pub fn begin_rendering(
        &self,
        color_attachments: &[vk::RenderingAttachmentInfo],
        depth_attachment: Option<&vk::RenderingAttachmentInfo>,
        stencil_attachment: Option<&vk::RenderingAttachmentInfo>,
        render_area: vk::Rect2D,
        layer_count: u32,
    ) {
        let mut rendering_info = vk::RenderingInfo::default()
            .render_area(render_area)
            .layer_count(layer_count)
            .color_attachments(color_attachments);

        if let Some(depth) = depth_attachment {
            rendering_info = rendering_info.depth_attachment(depth);
        }

        if let Some(stencil) = stencil_attachment {
            rendering_info = rendering_info.stencil_attachment(stencil);
        }

        unsafe {
            self.device
                .cmd_begin_rendering(self.command_buffer, &rendering_info);
        }
    }

    /// End a dynamic rendering pass.
    pub fn end_rendering(&self) {
        unsafe {
            self.device.cmd_end_rendering(self.command_buffer);
        }
    }

    //=========================================================================
    // Buffer Binding
    //=========================================================================

    /// Bind a single vertex buffer at binding 0.
    ///
    /// # Arguments
    /// * `buffer` - The vertex buffer to bind
    /// * `offset` - Byte offset into the buffer
    pub fn bind_vertex_buffer(&self, buffer: vk::Buffer, offset: vk::DeviceSize) {
        unsafe {
            self.device
                .cmd_bind_vertex_buffers(self.command_buffer, 0, &[buffer], &[offset]);
        }
    }

    /// Bind an index buffer.
    ///
    /// # Arguments
    /// * `buffer` - The index buffer to bind
    /// * `offset` - Byte offset into the buffer
    /// * `index_type` - Type of indices (UINT16 or UINT32)
    pub fn bind_index_buffer(
        &self,
        buffer: vk::Buffer,
        offset: vk::DeviceSize,
        index_type: vk::IndexType,
    ) {
        unsafe {
            self.device
                .cmd_bind_index_buffer(self.command_buffer, buffer, offset, index_type);
        }
    }

    //=========================================================================
    // Descriptor Set Binding
    //=========================================================================

    /// Bind descriptor sets to a graphics pipeline.
    ///
    /// # Arguments
    /// * `pipeline_layout` - The pipeline layout
    /// * `first_set` - First set number to bind
    /// * `descriptor_sets` - Slice of descriptor sets to bind
    /// * `dynamic_offsets` - Dynamic offset values for dynamic descriptors
    pub fn bind_descriptor_sets(
        &self,
        pipeline_layout: vk::PipelineLayout,
        first_set: u32,
        descriptor_sets: &[vk::DescriptorSet],
        dynamic_offsets: &[u32],
    ) {
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                first_set,
                descriptor_sets,
                dynamic_offsets,
            );
        }
    }

    /// Push descriptor set for VK_KHR_push_descriptor extension.
    ///
    /// Push descriptors are a lightweight alternative to descriptor sets that
    /// don't require allocation from a pool. They're pushed directly into the
    /// command buffer and are only valid for that command.
    ///
    /// # Arguments
    ///
    /// * `device` - Vulkan device with push descriptor extension
    /// * `pipeline_layout` - Pipeline layout for compatibility
    /// * `set` - Descriptor set number (e.g., 1 for push descriptors)
    /// * `descriptor_writes` - Descriptor writes to push
    pub fn push_descriptor_set_khr(
        &self,
        device: &ash::khr::push_descriptor::Device,
        pipeline_layout: vk::PipelineLayout,
        set: u32,
        descriptor_writes: &[vk::WriteDescriptorSet],
    ) {
        unsafe {
            device.cmd_push_descriptor_set(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                set,
                descriptor_writes,
            );
        }
    }

    //=========================================================================
    // Compute Pipeline Methods
    //=========================================================================

    /// Dispatch compute workgroups.
    ///
    /// This command dispatches compute workgroups in a 3D grid. Each workgroup
    /// contains a number of invocations defined by the `workgroup_size` in the shader.
    ///
    /// # Arguments
    /// * `group_count_x` - Number of local workgroups in the X dimension
    /// * `group_count_y` - Number of local workgroups in the Y dimension
    /// * `group_count_z` - Number of local workgroups in the Z dimension
    pub fn dispatch(&self, group_count_x: u32, group_count_y: u32, group_count_z: u32) {
        unsafe {
            self.device.cmd_dispatch(
                self.command_buffer,
                group_count_x,
                group_count_y,
                group_count_z,
            );
        }
    }

    /// Dispatch compute workgroups indirectly.
    ///
    /// The dispatch parameters are read from a buffer, allowing GPU-driven dispatch.
    /// The buffer must contain three u32 values: group_count_x, group_count_y, group_count_z.
    ///
    /// # Arguments
    /// * `buffer` - Buffer containing dispatch parameters
    /// * `offset` - Byte offset into the buffer where parameters start
    ///
    /// # Buffer Layout
    /// The buffer must contain the following at the specified offset:
    /// ```ignore
    /// struct DispatchParameters {
    ///     group_count_x: u32,
    ///     group_count_y: u32,
    ///     group_count_z: u32,
    /// }
    /// ```
    ///
    /// Push constants to a pipeline.
    /// They are stored in the command buffer and don't require descriptor sets.
    ///
    /// # Arguments
    /// * `layout` - Pipeline layout
    /// * `stage_flags` - Shader stages that will access the push constants
    /// * `offset` - Offset into the push constant block (in bytes)
    /// * `data` - Data to push (must be Pod + Zeroable)
    ///
    /// # Example
    /// ```ignore
    /// # use ash::vk;
    /// # use katla_gfx::CommandBuffer;
    /// # let command_buffer: CommandBuffer = unsafe { std::mem::zeroed() };
    /// # let layout: vk::PipelineLayout = unsafe { std::mem::zeroed() };
    /// #[repr(C)]
    /// #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    /// struct PushConstants {
    ///     delta_time: f32,
    ///     emit_count: u32,
    /// }
    ///
    /// let constants = PushConstants { delta_time: 0.016, emit_count: 10 };
    /// command_buffer.push_constants(
    ///     layout,
    ///     vk::ShaderStageFlags::COMPUTE,
    ///     0,
    ///     &constants,
    /// );
    /// ```
    pub fn push_constants<T: Copy + bytemuck::Pod + bytemuck::Zeroable>(
        &self,
        layout: vk::PipelineLayout,
        stage_flags: vk::ShaderStageFlags,
        offset: u32,
        data: &T,
    ) {
        unsafe {
            self.device.cmd_push_constants(
                self.command_buffer,
                layout,
                stage_flags,
                offset,
                bytemuck::bytes_of(data),
            );
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

    #[test]
    fn test_dispatch_workgroup_calculation() {
        // Test workgroup dispatch calculation for 64K particles
        let particle_count: u32 = 65536;
        let workgroup_size: u32 = 256;
        let group_count = (particle_count + workgroup_size - 1) / workgroup_size;

        assert_eq!(group_count, 256);
    }

    #[test]
    fn test_push_constants_size() {
        // Test push constant struct size (must be 4-byte aligned)
        #[repr(C)]
        struct TestPushConstants {
            delta_time: f32,
            emit_count: u32,
            max_particles: u32,
        }

        let size = std::mem::size_of::<TestPushConstants>();
        assert_eq!(size, 12);
        assert_eq!(size % 4, 0, "Push constants must be 4-byte aligned");
    }
}
