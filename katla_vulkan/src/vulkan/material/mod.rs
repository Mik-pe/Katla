pub mod asset;
pub mod buffer_descriptor;
pub mod builder;
pub mod compute_pipeline;
pub mod descriptor;
pub mod file_watcher;
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
pub use buffer_descriptor::{BufferBinding, BufferDescriptorSource, UniformBuffer};
pub use builder::*;
pub use compute_pipeline::{ComputePipeline, ComputePipelineBuilder, ComputePipelineError};
pub use descriptor::{
    DescriptorBinding, MaterialDescriptor, MaterialError, MaterialValue, RenderState, ShaderSource,
    ShaderStage, UniformType,
};
pub use file_watcher::*;
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

/// Texture info for bindless texture registration.
#[derive(Clone)]
pub struct ImageInfo {
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
}

impl ImageInfo {
    pub fn new(image_view: VkImageView, sampler: VkSampler) -> Self {
        Self {
            image_view: image_view.vk(),
            sampler: sampler.vk(),
        }
    }

    pub fn from_raw(image_view: vk::ImageView, sampler: vk::Sampler) -> Self {
        Self { image_view, sampler }
    }
}

/// Minimal handle for storage buffer-based materials.
/// Only stores texture info for bindless registration - no buffers or descriptors.
pub struct UniformHandle {
    image_info: Option<ImageInfo>,
    layout: UniformLayout,
}

impl UniformHandle {
    /// Create a minimal handle for storage buffer mode.
    pub fn new_storage(
        _context: &VulkanContext,
        _texture_layout: &vk::DescriptorSetLayout,
    ) -> Self {
        Self {
            image_info: None,
            layout: UniformLayout::empty(),
        }
    }

    pub fn layout(&self) -> &UniformLayout {
        &self.layout
    }

    pub fn add_image_info(&mut self, image_info: ImageInfo) {
        self.image_info = Some(image_info);
    }

    pub fn image_info(&self) -> Option<&ImageInfo> {
        self.image_info.as_ref()
    }

    pub fn destroy(&mut self, _context: &VulkanContext) {
        self.image_info = None;
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
    /// Create a MaterialPipeline with a custom descriptor layout.
    ///
    /// Use this for pipelines that don't fit the standard bindless or storage patterns,
    /// such as particle systems with custom descriptor layouts.
    ///
    /// For standard PBR materials, prefer `new_bindless()` or `new_storage()`.
    pub fn new_custom(
        pipeline: Pipeline,
        uniform_set_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        Self {
            pipeline: Some(pipeline),
            uniform: UniformHandle::new_storage(&context, &vk::DescriptorSetLayout::null()),
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: None,
            skeleton_set_layout: None,
            push_descriptor_set: None,
            context,
        }
    }

    /// Create a MaterialPipeline for storage buffer-based rendering.
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

    /// Get the pipeline handle as a wrapper type.
    ///
    /// Panics if the pipeline was destroyed.
    pub fn pipeline(&self) -> crate::sync::VkPipeline {
        self.vk_pipeline().pipeline()
    }

    /// Get the pipeline layout as a wrapper type.
    ///
    /// Panics if the pipeline was destroyed.
    pub fn pipeline_layout(&self) -> crate::sync::VkPipelineLayout {
        self.vk_pipeline().pipeline_layout()
    }

    /// Get the pipeline handle (panics if pipeline was destroyed)
    pub(crate) fn vk_pipeline(&self) -> &Pipeline {
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

    /// Destroy the pipeline resources (but NOT the descriptor set layout).
    ///
    /// This is used during hot reload when the descriptor set layout is
    /// preserved and owned by the MaterialTemplate.
    pub fn destroy_preserving_layout(&mut self) {
        self.uniform.destroy(&self.context);
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.destroy();
        }
        // Remove descriptor set layouts - they're owned by MaterialTemplate and will be destroyed there
        let _ = self.desc_layout.take();
        let _ = self.texture_set_layout.take();
    }

    pub fn destroy(&mut self) {
        self.uniform.destroy(&self.context);
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
