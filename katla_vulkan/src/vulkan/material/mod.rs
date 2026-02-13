pub mod asset;
pub mod builder;
pub mod descriptor;
pub mod file_watcher;
pub mod hot_reload;
pub mod materialbuilder;
pub mod parameters;
pub mod reflection;
pub mod registry;
pub mod shadermodule;
pub mod storage_uniform;
pub mod template;
pub mod uniform_layout;

pub use asset::*;
pub use builder::*;
pub use descriptor::{
    DescriptorBinding, MaterialDescriptor, MaterialError, MaterialValue, RenderState, ShaderSource,
    ShaderStage, UniformType,
};
pub use file_watcher::*;
pub use hot_reload::*;
pub use materialbuilder::*;
pub use parameters::*;
pub use reflection::*;
pub use registry::*;
pub use shadermodule::*;
pub use storage_uniform::{
    FrameUniforms, ObjectUniforms, StorageDescriptorSet, StorageUniformLayout, StorageUniformManager,
    // Backward compatibility
    BdaDescriptorSet, BdaUniformLayout, BdaUniformManager,
};
pub use template::*;
pub use uniform_layout::*;

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::rc::Rc;

use super::context::VulkanContext;

/// Texture descriptor set for material textures (set 1).
///
/// Contains the albedo texture and sampler bindings.
pub struct TextureDescriptorSet {
    /// Descriptor set containing texture bindings.
    pub descriptor_set: vk::DescriptorSet,
    /// Descriptor pool (owned, for cleanup).
    descriptor_pool: vk::DescriptorPool,
    /// Device for cleanup.
    device: ash::Device,
}

impl TextureDescriptorSet {
    /// Create a new texture descriptor set.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `desc_layout` - Descriptor set layout for texture set (set 1)
    /// * `image_info` - Image info for the texture
    ///
    /// # Returns
    /// A new TextureDescriptorSet with texture bindings
    pub fn new(
        context: &Rc<VulkanContext>,
        desc_layout: vk::DescriptorSetLayout,
        image_info: &ImageInfo,
    ) -> Result<Self, vk::Result> {
        // Create descriptor pool for textures
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(1),
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe {
            context
                .device
                .create_descriptor_pool(&pool_info, None)?
        };

        // Allocate descriptor set
        let layouts = [desc_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { context.device.allocate_descriptor_sets(&alloc_info)? };
        let descriptor_set = descriptor_sets[0];

        // Write texture descriptors using separate bindings
        let (image_write, sampler_write) =
            image_info.update_once_separate(descriptor_set, 0, 1);

        unsafe {
            context
                .device
                .update_descriptor_sets(&[image_write, sampler_write], &[]);
        }

        Ok(Self {
            descriptor_set,
            descriptor_pool,
            device: context.device.clone(),
        })
    }

    /// Get the descriptor set for binding.
    pub fn set(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }
}

impl Drop for TextureDescriptorSet {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

pub struct UniformBuffer {
    allocation: Allocation,
    buffer: vk::Buffer,
    buf_size: vk::DeviceSize,
}

#[derive(Clone)]
pub struct ImageInfo {
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub is_updated: bool,
    // For COMBINED_IMAGE_SAMPLER binding
    combined_info: vk::DescriptorImageInfo,
    // For SAMPLED_IMAGE binding
    sampled_image_info: vk::DescriptorImageInfo,
    // For SAMPLER binding
    sampler_only_info: vk::DescriptorImageInfo,
}

pub struct UniformHandle {
    next_bind_index: usize,
    next_update_index: usize,
    descriptors: Vec<UniformDescriptor>,
    layout: UniformLayout,
}

pub struct UniformDescriptor {
    pub desc_set: vk::DescriptorSet,
    desc_pool: Option<vk::DescriptorPool>,  // Option to prevent double-free
    pub uniform_buffer: Option<UniformBuffer>,
    pub image_info: Option<ImageInfo>,
    pub separate_bindings: bool,
}

impl ImageInfo {
    pub fn new(image_view: vk::ImageView, sampler: vk::Sampler) -> Self {
        // Create combined image-sampler info (for COMBINED_IMAGE_SAMPLER binding)
        let combined_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(image_view)
            .sampler(sampler);

        // Create sampled image info (for SAMPLED_IMAGE binding - null sampler)
        let sampled_image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(image_view)
            .sampler(vk::Sampler::null());

        // Create sampler-only info (for SAMPLER binding)
        // Note: For SAMPLER descriptors, the imageView field is ignored but shouldn't be null
        let sampler_only_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(image_view) // Use valid image view even though it's ignored
            .sampler(sampler);

        Self {
            image_view,
            sampler,
            is_updated: false,
            combined_info,
            sampled_image_info,
            sampler_only_info,
        }
    }

    fn update_once(&self, set: vk::DescriptorSet, binding: u32) -> vk::WriteDescriptorSet<'_> {
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&self.combined_info))
    }

    fn update_once_separate(
        &self,
        set: vk::DescriptorSet,
        image_binding: u32,
        sampler_binding: u32,
    ) -> (vk::WriteDescriptorSet<'_>, vk::WriteDescriptorSet<'_>) {
        let image_write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(image_binding)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(std::slice::from_ref(&self.sampled_image_info));

        let sampler_write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(sampler_binding)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .image_info(std::slice::from_ref(&self.sampler_only_info));

        (image_write, sampler_write)
    }
}

impl UniformHandle {
    pub fn new(context: &VulkanContext, desc_layout: &vk::DescriptorSetLayout) -> Self {
        Self::with_layout(context, desc_layout, UniformLayout::matrices_only())
    }

    pub fn new_with_bindings(
        context: &VulkanContext,
        desc_layout: &vk::DescriptorSetLayout,
        separate_bindings: bool,
    ) -> Self {
        Self::with_layout_and_bindings(
            context,
            desc_layout,
            UniformLayout::matrices_only(),
            separate_bindings,
        )
    }

    pub fn new_with_options(
        context: &VulkanContext,
        desc_layout: &vk::DescriptorSetLayout,
        separate_bindings: bool,
        has_color: bool,
    ) -> Self {
        let layout = if has_color {
            UniformLayout::pbr_with_color()
        } else {
            UniformLayout::matrices_only()
        };
        Self::with_layout_and_bindings(context, desc_layout, layout, separate_bindings)
    }

    /// Create a minimal UniformHandle for BDA-style rendering.
    ///
    /// This creates a handle without allocating descriptor sets or uniform buffers.
    /// It's used for BDA mode where:
    /// - Uniform data comes from BdaUniformManager (storage buffer)
    /// - This handle only stores texture info for create_texture_descriptor()
    ///
    /// # Arguments
    /// * `context` - Vulkan context (unused but kept for API consistency)
    /// * `texture_layout` - Texture descriptor set layout (unused, kept for reference)
    pub fn new_bda(_context: &VulkanContext, _texture_layout: &vk::DescriptorSetLayout) -> Self {
        // Create a single empty descriptor - no allocation happens
        // The image_info will be set via add_image_info() later
        let empty_descriptor = UniformDescriptor {
            desc_set: vk::DescriptorSet::null(),
            desc_pool: None, // No pool to destroy
            uniform_buffer: None, // No buffer - BDA uses shared storage buffer
            image_info: None,
            separate_bindings: true, // WGSL always uses separate bindings
        };

        Self {
            next_bind_index: 0,
            next_update_index: 0,
            descriptors: vec![empty_descriptor],
            layout: UniformLayout::empty(), // Minimal layout for BDA
        }
    }

    pub fn with_layout(
        context: &VulkanContext,
        desc_layout: &vk::DescriptorSetLayout,
        layout: UniformLayout,
    ) -> Self {
        Self::with_layout_and_bindings(context, desc_layout, layout, false)
    }

    pub fn with_layout_and_bindings(
        context: &VulkanContext,
        desc_layout: &vk::DescriptorSetLayout,
        layout: UniformLayout,
        separate_bindings: bool,
    ) -> Self {
        let mut uniform_descs = vec![];
        for _ in 0..2 {
            let uniform_desc =
                Self::create_descriptor_sets(context, desc_layout, &layout, separate_bindings);
            uniform_descs.push(uniform_desc);
        }

        Self {
            next_bind_index: 0,
            next_update_index: 0,
            descriptors: uniform_descs,
            layout,
        }
    }

    /// Get the uniform layout for this handle.
    pub fn layout(&self) -> &UniformLayout {
        &self.layout
    }

    pub fn add_image_info(&mut self, image_info: ImageInfo) {
        for descr in &mut self.descriptors {
            descr.image_info = Some(image_info.clone());
        }
    }

    pub fn update_buffer(&mut self, context: &VulkanContext, data: &[u8]) {
        self.descriptors[self.next_update_index].update_buffer(context, data);

        self.next_bind_index = self.next_update_index;
        self.next_update_index = (self.next_update_index + 1) % self.descriptors.len();
    }

    pub fn next_descriptor(&self) -> &UniformDescriptor {
        &self.descriptors[self.next_bind_index]
    }

    pub fn destroy(&mut self, context: &VulkanContext) {
        // Destroy all descriptors and clear the vector to make this idempotent
        for desc in &mut self.descriptors {
            desc.destroy(context);
        }
        self.descriptors.clear();
    }

    fn create_descriptor_sets(
        context: &VulkanContext,
        desc_layout: &vk::DescriptorSetLayout,
        layout: &UniformLayout,
        separate_bindings: bool,
    ) -> UniformDescriptor {
        // Calculate buffer size from layout
        let data_size = layout.total_size() as vk::DeviceSize;

        let create_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .size(data_size);

        let (buffer, allocation) =
            context.allocate_buffer(&create_info, gpu_allocator::MemoryLocation::CpuToGpu);
        let uniform_buffer = Some(UniformBuffer {
            allocation,
            buffer,
            buf_size: data_size as vk::DeviceSize,
        });

        let desc_pool_sizes = &[
            vk::DescriptorPoolSize::default()
                .descriptor_count(1)
                .ty(vk::DescriptorType::UNIFORM_BUFFER),
            vk::DescriptorPoolSize::default()
                .descriptor_count(1)
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
            vk::DescriptorPoolSize::default()
                .descriptor_count(1)
                .ty(vk::DescriptorType::SAMPLED_IMAGE),
            vk::DescriptorPoolSize::default()
                .descriptor_count(1)
                .ty(vk::DescriptorType::SAMPLER),
        ];
        let desc_pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(desc_pool_sizes)
            .max_sets(1);
        let desc_pool =
            unsafe { context.device.create_descriptor_pool(&desc_pool_info, None) }.unwrap();

        let desc_layouts = &[*desc_layout];
        let desc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(desc_pool)
            .set_layouts(desc_layouts);
        let desc_set = unsafe { context.device.allocate_descriptor_sets(&desc_info) }.unwrap()[0];

        let image_info = None;

        UniformDescriptor {
            desc_set,
            desc_pool: Some(desc_pool),
            uniform_buffer,
            image_info,
            separate_bindings,
        }
    }
}

impl UniformDescriptor {
    pub fn update_buffer(&mut self, context: &VulkanContext, data: &[u8]) {
        if let Some(uniform_buffer) = &self.uniform_buffer {
            let data_size = std::mem::size_of_val(data) as vk::DeviceSize;
            if uniform_buffer.buf_size < data_size {
                panic!(
                    "Too little memory allocated for buffer of size {}",
                    data_size
                );
            }

            let mapped_data = context.map_buffer(&uniform_buffer.allocation);
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), mapped_data, data_size as usize);
            }

            let buf_info = [vk::DescriptorBufferInfo::default()
                .buffer(uniform_buffer.buffer)
                .offset(0)
                .range(data_size)];
            let mut desc_writes = vec![];
            desc_writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(self.desc_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buf_info),
            );
            if let Some(image_info) = &mut self.image_info {
                if !image_info.is_updated {
                    image_info.is_updated = true;
                    if self.separate_bindings {
                        let (image_write, sampler_write) =
                            image_info.update_once_separate(self.desc_set, 1, 2);
                        desc_writes.push(image_write);
                        desc_writes.push(sampler_write);
                    } else {
                        let write_set = image_info.update_once(self.desc_set, 1);
                        desc_writes.push(write_set);
                    }
                }
            }

            unsafe {
                context
                    .device
                    .update_descriptor_sets(desc_writes.as_slice(), &[])
            };
        }
    }

    pub fn destroy(&mut self, context: &VulkanContext) {
        if self.uniform_buffer.is_some() {
            let buffer = self.uniform_buffer.take().unwrap();
            context.free_buffer(buffer.buffer, buffer.allocation);
        }
        if let Some(desc_pool) = self.desc_pool.take() {
            unsafe {
                context.device.destroy_descriptor_pool(desc_pool, None);
            }
        }
    }
}

pub struct DescriptorLayoutBuilder {
    bindings: Vec<vk::DescriptorSetLayoutBinding<'static>>,
}

impl DescriptorLayoutBuilder {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn add_binding(
        mut self,
        binding: u32,
        descriptor_type: vk::DescriptorType,
        stage_flags: vk::ShaderStageFlags,
        count: u32,
    ) -> Self {
        self.bindings.push(vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type,
            descriptor_count: count,
            stage_flags,
            p_immutable_samplers: std::ptr::null(),
            _marker: std::marker::PhantomData,
        });
        self
    }

    pub fn build(&self, device: &ash::Device) -> Result<vk::DescriptorSetLayout, vk::Result> {
        let create_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&self.bindings);
        unsafe { device.create_descriptor_set_layout(&create_info, None) }
    }
}

impl Default for DescriptorLayoutBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct MaterialPipeline {
    pipeline: Option<Pipeline>,
    pub uniform: UniformHandle,
    desc_layout: Option<vk::DescriptorSetLayout>,
    /// Texture descriptor set layout (set 1) for BDA-style rendering.
    /// Separated from uniform set (set 0) for bindless-style texture updates.
    pub texture_set_layout: Option<vk::DescriptorSetLayout>,
    /// Texture descriptor set (set 1) containing material textures.
    pub texture_descriptor: Option<TextureDescriptorSet>,
    context: Rc<VulkanContext>,
}

impl MaterialPipeline {
    pub fn new(
        pipeline: Pipeline,
        desc_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        Self::with_layout(
            pipeline,
            desc_layout,
            context,
            UniformLayout::matrices_only(),
        )
    }

    pub fn new_with_bindings(
        pipeline: Pipeline,
        desc_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
        separate_bindings: bool,
    ) -> Self {
        Self::with_layout_and_bindings(
            pipeline,
            desc_layout,
            context,
            UniformLayout::matrices_only(),
            separate_bindings,
        )
    }

    pub fn new_with_options(
        pipeline: Pipeline,
        desc_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
        separate_bindings: bool,
        has_color: bool,
    ) -> Self {
        let layout = if has_color {
            UniformLayout::pbr_with_color()
        } else {
            UniformLayout::matrices_only()
        };
        Self::with_layout_and_bindings(pipeline, desc_layout, context, layout, separate_bindings)
    }

    pub fn with_layout(
        pipeline: Pipeline,
        desc_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
        layout: UniformLayout,
    ) -> Self {
        Self::with_layout_and_bindings(pipeline, desc_layout, context, layout, false)
    }

    pub fn with_layout_and_bindings(
        pipeline: Pipeline,
        desc_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
        layout: UniformLayout,
        separate_bindings: bool,
    ) -> Self {
        let uniform = UniformHandle::with_layout_and_bindings(
            &context,
            &desc_layout,
            layout,
            separate_bindings,
        );
        Self {
            pipeline: Some(pipeline),
            uniform,
            desc_layout: Some(desc_layout),
            texture_set_layout: None,
            texture_descriptor: None,
            context,
        }
    }

    /// Create a MaterialPipeline for storage buffer-based rendering with instance indexing.
    ///
    /// This constructor is for use with storage buffer-based uniforms.
    /// The pipeline uses two descriptor sets:
    /// - Set 0 (uniform_set_layout): Storage buffers for frame_data and objects
    /// - Set 1 (texture_set_layout): Textures (separate image + sampler)
    ///
    /// Object indexing is done via `@builtin(instance_index)` in the shader,
    /// which is set by the `first_instance` parameter in draw calls.
    ///
    /// The UniformHandle is created minimally - it only stores texture info
    /// for later use by `create_texture_descriptor()`. Actual uniform data
    /// comes from the shared BdaUniformManager.
    pub fn new_storage(
        pipeline: Pipeline,
        uniform_set_layout: vk::DescriptorSetLayout,
        texture_set_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        // Create a minimal UniformHandle that just stores texture info
        // The actual uniform data comes from BdaUniformManager, not this handle
        let uniform = UniformHandle::new_bda(&context, &texture_set_layout);

        Self {
            pipeline: Some(pipeline),
            uniform,
            desc_layout: Some(uniform_set_layout),
            texture_set_layout: Some(texture_set_layout),
            texture_descriptor: None,
            context,
        }
    }

    /// Alias for backward compatibility.
    #[deprecated(since = "0.1.0", note = "Use new_storage() instead")]
    pub fn new_bda(
        pipeline: Pipeline,
        uniform_set_layout: vk::DescriptorSetLayout,
        texture_set_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        Self::new_storage(pipeline, uniform_set_layout, texture_set_layout, context)
    }

    /// Get the Vulkan context for this pipeline.
    pub fn context(&self) -> &Rc<VulkanContext> {
        &self.context
    }

    /// Get the pipeline handle.
    pub fn get_pipeline(&self) -> Option<&Pipeline> {
        self.pipeline.as_ref()
    }

    /// Get the pipeline handle (panics if pipeline was destroyed)
    pub fn vk_pipeline(&self) -> &Pipeline {
        self.pipeline
            .as_ref()
            .expect("Pipeline accessed after destruction")
    }

    /// Get the pipeline layout (panics if pipeline was destroyed)
    pub fn vk_layout(&self) -> vk::PipelineLayout {
        self.pipeline
            .as_ref()
            .expect("Pipeline accessed after destruction")
            .layout
    }

    /// Get the uniform layout for this pipeline.
    pub fn layout(&self) -> &UniformLayout {
        self.uniform.layout()
    }

    /// Bind pipeline with a custom descriptor set (for materials with per-material uniforms).
    ///
    /// This method is kept for backward compatibility but should be replaced
    /// with BDA-based uniform management for better performance.
    ///
    /// # Deprecated
    /// This method uses traditional descriptor-based uniform updates.
    /// Prefer `bind_with_bda()` for new code which uses
    /// Buffer Device Address (BDA) for uniform data.
    ///
    /// NOTE: BDA implementation is pending Application integration.
    /// Shaders must be updated before using bind_with_bda().
    #[deprecated(since = "0.1.0", note = "Use bind_with_bda() for BDA uniform management (pending Application integration)")]
    pub fn bind(&self, command_buffer: vk::CommandBuffer) {
        let pipeline = self
            .pipeline
            .as_ref()
            .expect("Pipeline accessed after destruction");
        unsafe {
            self.context.device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.handle,
            );

            self.context.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.layout,
                0,
                &[self.uniform.next_descriptor().desc_set],
                &[],
            );
        }
    }

    /// Bind the pipeline with a custom descriptor set (for materials with per-material uniforms)
    ///
    /// # Arguments
    /// * `command_buffer` - The command buffer to record into
    /// * `descriptor_set` - The descriptor set to bind (from material's own uniform buffer)
    pub fn bind_with_descriptor(
        &self,
        command_buffer: vk::CommandBuffer,
        descriptor_set: vk::DescriptorSet,
    ) {
        let pipeline = self
            .pipeline
            .as_ref()
            .expect("Pipeline accessed after destruction");
        unsafe {
            self.context.device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.handle,
            );

            self.context.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.layout,
                0,
                &[descriptor_set],
                &[],
            );
        }
    }

    /// Create texture descriptor set from the material's image info.
    ///
    /// This creates set 1 for texture bindings. Must have texture_set_layout set.
    ///
    /// # Arguments
    /// * `image_info` - Optional image info to use. If None, uses the pipeline's uniform image_info.
    ///
    /// # Returns
    /// Ok(()) on success, or an error if creation fails
    pub fn create_texture_descriptor_with_info(
        &mut self,
        image_info: &ImageInfo,
    ) -> Result<(), vk::Result> {
        let texture_layout = self
            .texture_set_layout
            .ok_or(vk::Result::ERROR_INITIALIZATION_FAILED)?;

        let texture_descriptor =
            TextureDescriptorSet::new(&self.context, texture_layout, image_info)?;

        self.texture_descriptor = Some(texture_descriptor);
        Ok(())
    }

    /// Create texture descriptor set from the pipeline's image info.
    ///
    /// This creates set 1 for texture bindings. Must have texture_set_layout set
    /// and image_info must be set in the pipeline's uniform.
    ///
    /// # Returns
    /// Ok(()) on success, or an error if creation fails
    pub fn create_texture_descriptor(&mut self) -> Result<(), vk::Result> {
        // Clone the image_info to avoid borrow issues
        let image_info = self
            .uniform
            .next_descriptor()
            .image_info
            .clone()
            .ok_or(vk::Result::ERROR_INITIALIZATION_FAILED)?;

        self.create_texture_descriptor_with_info(&image_info)
    }

    /// Bind pipeline with storage buffer-based uniforms (instance index style).
    ///
    /// This method binds:
    /// - The pipeline
    /// - Set 0: Storage buffer uniforms (frame_data + objects)
    /// - Set 1: Texture descriptor
    ///
    /// Object indexing is done via `@builtin(instance_index)` in the shader,
    /// which is set by the `first_instance` parameter in draw calls.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record into
    /// * `storage_descriptor_set` - Storage buffer descriptor set (set 0)
    ///
    /// # Panics
    /// Panics if texture_descriptor is not set (call create_texture_descriptor first)
    pub fn bind_with_storage(
        &self,
        command_buffer: vk::CommandBuffer,
        storage_descriptor_set: vk::DescriptorSet,
    ) {
        let pipeline = self
            .pipeline
            .as_ref()
            .expect("Pipeline accessed after destruction");

        let texture_set = self
            .texture_descriptor
            .as_ref()
            .expect("Texture descriptor not created - call create_texture_descriptor first");

        unsafe {
            // Bind pipeline
            self.context.device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.handle,
            );

            // Bind set 0: Storage uniforms (frame_data + objects)
            self.context.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.layout,
                0,
                &[storage_descriptor_set],
                &[],
            );

            // Bind set 1: Textures
            self.context.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.layout,
                1,
                &[texture_set.set()],
                &[],
            );

            // Note: Object index is passed via `first_instance` in draw call,
            // and accessed in shader via `@builtin(instance_index)`
        }
    }

    pub fn update_buffer(&mut self, data: &[u8]) {
        self.uniform.update_buffer(&self.context, data);
    }

    /// Destroy the pipeline resources (but NOT the descriptor set layout).
    ///
    /// This is used during hot reload when the descriptor set layout is
    /// preserved and owned by the MaterialTemplate.
    pub fn destroy_preserving_layout(&mut self) {
        self.uniform.destroy(&self.context);
        self.texture_descriptor = None; // Drop cleans up descriptor pool
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.destroy();
        }
        // Remove descriptor set layouts - they're owned by MaterialTemplate and will be destroyed there
        let _ = self.desc_layout.take();
        let _ = self.texture_set_layout.take();
    }

    pub fn destroy(&mut self) {
        self.uniform.destroy(&self.context);
        self.texture_descriptor = None; // Drop cleans up descriptor pool
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.destroy();
        }
        if let Some(desc_layout) = self.desc_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(desc_layout, None);
            }
        }
        if let Some(texture_layout) = self.texture_set_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(texture_layout, None);
            }
        }
    }
}

impl Drop for MaterialPipeline {
    fn drop(&mut self) {
        // Clean up any remaining resources
        // Note: If destroy_preserving_layout() was called, these will already be None
        self.uniform.destroy(&self.context);
        self.texture_descriptor = None; // Drop cleans up descriptor pool
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.destroy();
        }
        if let Some(desc_layout) = self.desc_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(desc_layout, None);
            }
        }
        if let Some(texture_layout) = self.texture_set_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(texture_layout, None);
            }
        }
    }
}
