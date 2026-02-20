pub mod asset;
pub mod buffer_descriptor;
pub mod builder;
pub mod compute_pipeline;
pub mod descriptor;
pub mod file_watcher;
pub mod hot_reload;
pub mod materialbuilder;
pub mod parameters;
pub mod reflection;
pub mod registry;
pub mod shadermodule;
pub mod skeleton_descriptor;
pub mod storage_uniform;
pub mod template;
pub mod uniform_layout;

pub use asset::*;
pub use buffer_descriptor::{
    BufferBinding, BufferDescriptorSet, BufferDescriptorSetBuilder, BufferDescriptorSource,
};
pub use builder::*;
pub use compute_pipeline::{ComputePipeline, ComputePipelineBuilder, ComputePipelineError};
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
pub use skeleton_descriptor::SkeletonDescriptorSet;
pub use storage_uniform::{
    FrameUniforms, ObjectUniforms, StorageDescriptorSet, StorageUniformLayout,
    StorageUniformManager,
};
pub use template::*;
pub use uniform_layout::*;

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::rc::Rc;

use super::context::VulkanContext;
use crate::sync::{VkImageView, VkSampler};

/// PBR texture set containing all texture maps for a PBR material.
///
/// Contains albedo, normal, metallic/roughness, and occlusion textures.
/// Used to gather texture info before registering with bindless manager.
pub struct PbrTextureSet {
    pub albedo: ImageInfo,
    pub normal: ImageInfo,
    pub metallic_roughness: ImageInfo,
    pub occlusion: ImageInfo,
    pub emission: ImageInfo,
}

impl PbrTextureSet {
    /// Create a new PBR texture set from ImageInfo structs.
    pub fn new(
        albedo: ImageInfo,
        normal: ImageInfo,
        metallic_roughness: ImageInfo,
        occlusion: ImageInfo,
        emission: ImageInfo,
    ) -> Self {
        Self {
            albedo,
            normal,
            metallic_roughness,
            occlusion,
            emission,
        }
    }

    /// Create from raw Vulkan handles using a shared sampler.
    pub fn from_handles_shared_sampler(
        albedo_view: vk::ImageView,
        normal_view: vk::ImageView,
        mr_view: vk::ImageView,
        occlusion_view: vk::ImageView,
        emission_view: vk::ImageView,
        sampler: vk::Sampler,
    ) -> Self {
        Self {
            albedo: ImageInfo::from_raw(albedo_view, sampler),
            normal: ImageInfo::from_raw(normal_view, sampler),
            metallic_roughness: ImageInfo::from_raw(mr_view, sampler),
            occlusion: ImageInfo::from_raw(occlusion_view, sampler),
            emission: ImageInfo::from_raw(emission_view, sampler),
        }
    }

    /// Create from wrapper types using a shared sampler.
    pub fn from_wrapped_shared_sampler(
        albedo_view: crate::sync::VkImageView,
        normal_view: crate::sync::VkImageView,
        mr_view: crate::sync::VkImageView,
        occlusion_view: crate::sync::VkImageView,
        emission_view: crate::sync::VkImageView,
        sampler: crate::sync::VkSampler,
    ) -> Self {
        Self::from_handles_shared_sampler(
            albedo_view.into(),
            normal_view.into(),
            mr_view.into(),
            occlusion_view.into(),
            emission_view.into(),
            sampler.into(),
        )
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
    desc_pool: Option<vk::DescriptorPool>, // Option to prevent double-free
    pub uniform_buffer: Option<UniformBuffer>,
    pub image_info: Option<ImageInfo>,
    pub separate_bindings: bool,
}

impl ImageInfo {
    /// Create a new ImageInfo from wrapper types.
    ///
    /// This is the preferred constructor that accepts wrapper types
    /// instead of raw Vulkan types.
    pub fn new(image_view: VkImageView, sampler: VkSampler) -> Self {
        Self::from_raw(image_view.vk(), sampler.vk())
    }

    /// Create from raw Vulkan types (internal use).
    fn from_raw(image_view: vk::ImageView, sampler: vk::Sampler) -> Self {
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

    /// Create a minimal UniformHandle for storage buffer-based rendering.
    ///
    /// This creates a handle without allocating descriptor sets or uniform buffers.
    /// It's used for storage buffer mode where:
    /// - Uniform data comes from StorageUniformManager (shared storage buffer)
    /// - This handle stores texture info for bindless texture registration
    ///
    /// # Arguments
    /// * `context` - Vulkan context (unused but kept for API consistency)
    /// * `texture_layout` - Texture descriptor set layout (unused, kept for reference)
    pub fn new_storage(
        _context: &VulkanContext,
        _texture_layout: &vk::DescriptorSetLayout,
    ) -> Self {
        // Create a single empty descriptor - no allocation happens
        // The image_info will be set via add_image_info() later
        let empty_descriptor = UniformDescriptor {
            desc_set: vk::DescriptorSet::null(),
            desc_pool: None,      // No pool to destroy
            uniform_buffer: None, // No buffer - storage mode uses shared storage buffer
            image_info: None,
            separate_bindings: true, // WGSL always uses separate bindings
        };

        Self {
            next_bind_index: 0,
            next_update_index: 0,
            descriptors: vec![empty_descriptor],
            layout: UniformLayout::empty(), // Minimal layout for storage mode
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
                .descriptor_count(4)
                .ty(vk::DescriptorType::UNIFORM_BUFFER),
            vk::DescriptorPoolSize::default()
                .descriptor_count(4)
                .ty(vk::DescriptorType::STORAGE_BUFFER),
            vk::DescriptorPoolSize::default()
                .descriptor_count(4)
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
            vk::DescriptorPoolSize::default()
                .descriptor_count(4)
                .ty(vk::DescriptorType::SAMPLED_IMAGE),
            vk::DescriptorPoolSize::default()
                .descriptor_count(4)
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
    /// Additional descriptor set layouts (e.g., push descriptor layouts for UI).
    /// These need to be cleaned up on drop.
    additional_layouts: Vec<vk::DescriptorSetLayout>,
    /// Texture descriptor set layout (set 1) for legacy texture binding.
    /// Not used in bindless mode (textures accessed via ObjectUniforms.texture_indices).
    pub texture_set_layout: Option<vk::DescriptorSetLayout>,
    /// Skeleton descriptor set layout (set 2) for skeletal animation.
    /// Only present on skinned pipelines.
    pub skeleton_set_layout: Option<vk::DescriptorSetLayout>,
    /// Push descriptor set index (if this pipeline uses push descriptors).
    /// Used by UI textures for dynamic texture switching.
    pub push_descriptor_set: Option<u32>,
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

    /// Create a MaterialPipeline using wrapper types.
    pub fn new_wrapped(
        pipeline: Pipeline,
        desc_layout: crate::sync::VkDescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        Self::new(pipeline, desc_layout.into(), context)
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
            additional_layouts: Vec::new(),
            texture_set_layout: None,
            skeleton_set_layout: None,
            push_descriptor_set: None,
            context,
        }
    }
    ///
    /// This constructor is for use with storage buffer-based uniforms.
    /// The pipeline uses two descriptor sets:
    /// - Set 0 (uniform_set_layout): Storage buffers for frame_data and objects
    /// - Set 1 (texture_set_layout): Bindless texture array + shared sampler
    ///
    /// Object indexing is done via `@builtin(instance_index)` in the shader,
    /// which is set by the `first_instance` parameter in draw calls.
    ///
    /// The UniformHandle is created minimally - it stores texture info
    /// for bindless texture registration. Actual uniform data
    /// comes from the shared StorageUniformManager.
    pub fn new_storage(
        pipeline: Pipeline,
        uniform_set_layout: vk::DescriptorSetLayout,
        texture_set_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        // Create a minimal UniformHandle that just stores texture info
        // The actual uniform data comes from StorageUniformManager, not this handle
        let uniform = UniformHandle::new_storage(&context, &texture_set_layout);

        Self {
            pipeline: Some(pipeline),
            uniform,
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: Some(texture_set_layout),
            skeleton_set_layout: None,
            push_descriptor_set: None,
            context,
        }
    }

    /// Create a MaterialPipeline for storage buffer rendering with skeletal animation.
    ///
    /// This is like `new_storage` but with a third descriptor set for skeleton joint matrices:
    /// - Set 0 (uniform_set_layout): Storage buffers for frame_data and objects
    /// - Set 1 (texture_set_layout): Textures (separate image + sampler)
    /// - Set 2 (skeleton_set_layout): Storage buffer for joint matrices
    ///
    /// The skeleton descriptor set must be created and bound per animated mesh.
    pub fn new_storage_skinned(
        pipeline: Pipeline,
        uniform_set_layout: vk::DescriptorSetLayout,
        texture_set_layout: vk::DescriptorSetLayout,
        skeleton_set_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        let uniform = UniformHandle::new_storage(&context, &texture_set_layout);

        Self {
            pipeline: Some(pipeline),
            uniform,
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: Some(texture_set_layout),
            skeleton_set_layout: Some(skeleton_set_layout),
            push_descriptor_set: None,
            context,
        }
    }

    /// Create a MaterialPipeline for bindless texture rendering.
    ///
    /// This is like `new_storage` but for bindless textures:
    /// - Set 0 (uniform_set_layout): Storage buffers for frame_data and objects
    /// - Set 1: Bindless texture array + shared sampler (owned by BindlessTextureManager)
    ///
    /// Textures are NOT managed per-material. Instead:
    /// - Register textures with BindlessTextureManager to get indices
    /// - Pass texture indices via ObjectUniforms.texture_indices
    /// - Bind the BindlessTextureManager's descriptor set once per frame
    pub fn new_bindless(
        pipeline: Pipeline,
        uniform_set_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        // Create a minimal UniformHandle - no texture descriptors needed
        let uniform = UniformHandle::new_storage(&context, &vk::DescriptorSetLayout::null());

        Self {
            pipeline: Some(pipeline),
            uniform,
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: None, // No per-material texture layout for bindless
            skeleton_set_layout: None,
            push_descriptor_set: None,
            context,
        }
    }

    /// Create a MaterialPipeline for bindless texture rendering with skeletal animation.
    ///
    /// This is like `new_bindless` but with skeleton support:
    /// - Set 0 (uniform_set_layout): Storage buffers for frame_data and objects
    /// - Set 1: Bindless texture array + shared sampler (owned by BindlessTextureManager)
    /// - Set 2 (skeleton_set_layout): Storage buffer for joint matrices
    pub fn new_bindless_skinned(
        pipeline: Pipeline,
        uniform_set_layout: vk::DescriptorSetLayout,
        skeleton_set_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        // Create a minimal UniformHandle - no texture descriptors needed
        let uniform = UniformHandle::new_storage(&context, &vk::DescriptorSetLayout::null());

        Self {
            pipeline: Some(pipeline),
            uniform,
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: None, // No per-material texture layout for bindless
            skeleton_set_layout: Some(skeleton_set_layout),
            push_descriptor_set: None,
            context,
        }
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
            .vk_layout()
    }

    /// Get the uniform layout for this pipeline.
    pub fn layout(&self) -> &UniformLayout {
        self.uniform.layout()
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
                pipeline.vk_pipeline(),
            );

            self.context.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.vk_layout(),
                0,
                &[descriptor_set],
                &[],
            );
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
        if let Some(mut pipeline) = self.pipeline.take() {
            pipeline.destroy();
        }
        // Remove descriptor set layouts - they're owned by MaterialTemplate and will be destroyed there
        let _ = self.desc_layout.take();
        let _ = self.texture_set_layout.take();
    }

    pub fn destroy(&mut self) {
        self.uniform.destroy(&self.context);
        if let Some(mut pipeline) = self.pipeline.take() {
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
        if let Some(mut pipeline) = self.pipeline.take() {
            pipeline.destroy();
        }
        if let Some(desc_layout) = self.desc_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(desc_layout, None);
            }
        }
        // Clean up additional descriptor layouts (e.g., push descriptor layouts)
        for layout in self.additional_layouts.drain(..) {
            unsafe {
                self.context.device.destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(texture_layout) = self.texture_set_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(texture_layout, None);
            }
        }
        if let Some(skeleton_layout) = self.skeleton_set_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(skeleton_layout, None);
            }
        }
    }
}
