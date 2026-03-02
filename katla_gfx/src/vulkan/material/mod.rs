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

// Explicit exports from asset module
pub use asset::{AssetError, load_material_from_file};

// Explicit exports from buffer_descriptor module
pub use buffer_descriptor::UniformBuffer;

// Explicit exports from builder module
pub use builder::{Pipeline, PipelineBuilder, PipelineError};

// Explicit exports from compute_pipeline module
pub use compute_pipeline::{ComputePipeline, ComputePipelineBuilder, ComputePipelineError};

// Explicit exports from descriptor module
pub use descriptor::{
    MaterialDescriptor, MaterialError, MaterialValue, RenderState, ShaderBinding, ShaderSource,
    ShaderStage, UniformType,
};

// Explicit exports from file_watcher module
pub use file_watcher::{FileWatcher, WatcherError};

// Explicit exports from materialbuilder module
pub use materialbuilder::{MaterialBuildError, MaterialBuilder};

// Explicit exports from parameters module
pub use parameters::{MaterialParameters, ParameterError};

// Explicit exports from reflection module
pub use reflection::{MemberType, ReflectionError, ShaderReflection, StructLayout, StructMember};

// Explicit exports from registry module
pub(crate) use registry::MaterialRegistry;

// Explicit exports from shadermodule module
pub use shadermodule::{ShaderCache, ShaderError, ShaderModule};

// Explicit exports from skeleton_descriptor module
pub use skeleton_descriptor::SkeletonDescriptorSet;

// Explicit exports from storage_uniform module
pub use storage_uniform::{
    FrameUniforms, ObjectUniforms, StorageDescriptorSet, StorageUniformLayout,
    StorageUniformManager,
};

// Explicit exports from template module
pub use template::{InstanceError, Material, MaterialTemplate};

// Explicit exports from uniform_layout module
pub use uniform_layout::UniformLayout;

use ash::vk;
use std::rc::Rc;

use super::context::VulkanContext;
use crate::handle::TextureHandle;

/// PBR texture set containing all texture maps for a PBR material.
///
/// Contains albedo, normal, metallic/roughness, occlusion, and emission textures.
/// Uses TextureHandle for opaque texture references.
#[derive(Clone, Debug)]
pub struct PbrTextureSet {
    pub albedo: TextureHandle,
    pub normal: TextureHandle,
    pub metallic_roughness: TextureHandle,
    pub occlusion: TextureHandle,
    pub emission: TextureHandle,
}

impl PbrTextureSet {
    /// Create a new PBR texture set from texture handles.
    pub fn new(
        albedo: TextureHandle,
        normal: TextureHandle,
        metallic_roughness: TextureHandle,
        occlusion: TextureHandle,
        emission: TextureHandle,
    ) -> Self {
        Self {
            albedo,
            normal,
            metallic_roughness,
            occlusion,
            emission,
        }
    }

    /// Create a PBR texture set with default textures.
    ///
    /// Uses the TextureManager's default textures for all slots.
    pub fn with_defaults(tm: &crate::texture::TextureManager) -> Self {
        Self {
            albedo: tm.default_white(),
            normal: tm.default_normal(),
            metallic_roughness: tm.default_metallic_roughness(),
            occlusion: tm.default_occlusion(),
            emission: tm.default_emission(),
        }
    }

    /// Create a PBR texture set with placeholder handles.
    ///
    /// This is for backward compatibility when TextureManager is not available.
    /// The handles are NONE and should not be used for actual texture lookups.
    /// Bindless indices are used for actual texture access.
    pub fn with_placeholder_handles() -> Self {
        Self {
            albedo: TextureHandle::NONE,
            normal: TextureHandle::NONE,
            metallic_roughness: TextureHandle::NONE,
            occlusion: TextureHandle::NONE,
            emission: TextureHandle::NONE,
        }
    }

    /// Get all texture handles as an array.
    pub fn handles(&self) -> [TextureHandle; 5] {
        [
            self.albedo,
            self.normal,
            self.metallic_roughness,
            self.occlusion,
            self.emission,
        ]
    }

    /// Register textures with bindless manager and return indices.
    ///
    /// Returns [albedo, normal, mr, occlusion, emission] bindless slot indices.
    pub fn register_bindless(
        &self,
        tm: &crate::texture::TextureManager,
        bindless: &mut crate::vulkan::bindless_texture::BindlessTextureManager,
    ) -> Option<[u32; 5]> {
        let albedo_slot = bindless.register_texture(tm.get_view(self.albedo)?)?;
        let normal_slot = bindless.register_texture(tm.get_view(self.normal)?)?;
        let mr_slot = bindless.register_texture(tm.get_view(self.metallic_roughness)?)?;
        let occlusion_slot = bindless.register_texture(tm.get_view(self.occlusion)?)?;
        let emission_slot = bindless.register_texture(tm.get_view(self.emission)?)?;

        Some([
            albedo_slot,
            normal_slot,
            mr_slot,
            occlusion_slot,
            emission_slot,
        ])
    }
}

pub struct MaterialPipeline {
    pipeline: Option<Pipeline>,
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
    /// Whether this pipeline uses bindless textures.
    /// Bindless materials bind the BindlessTextureManager's descriptor set at set 1.
    pub is_bindless: bool,
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
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: None,
            skeleton_set_layout: None,
            push_descriptor_set: None,
            is_bindless: false,
            context,
        }
    }

    /// Create a MaterialPipeline for UI rendering with push descriptors.
    ///
    /// UI materials use two descriptor sets:
    /// - Set 0: Uniform/sampler/font atlas (static)
    /// - Set 1: Dynamic texture via push descriptors
    pub fn new_ui(
        pipeline: Pipeline,
        uniform_set_layout: vk::DescriptorSetLayout,
        push_descriptor_layout: vk::DescriptorSetLayout,
        context: Rc<VulkanContext>,
    ) -> Self {
        Self {
            pipeline: Some(pipeline),
            desc_layout: Some(uniform_set_layout),
            additional_layouts: vec![push_descriptor_layout],
            texture_set_layout: None,
            skeleton_set_layout: None,
            push_descriptor_set: Some(1), // Set 1 is the push descriptor set
            is_bindless: false,
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
        Self {
            pipeline: Some(pipeline),
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: Some(texture_set_layout),
            skeleton_set_layout: None,
            push_descriptor_set: None,
            is_bindless: false,
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
        Self {
            pipeline: Some(pipeline),
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: Some(texture_set_layout),
            skeleton_set_layout: Some(skeleton_set_layout),
            push_descriptor_set: None,
            is_bindless: false,
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
        Self {
            pipeline: Some(pipeline),
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: None, // No per-material texture layout for bindless
            skeleton_set_layout: None,
            push_descriptor_set: None,
            is_bindless: true,
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
        Self {
            pipeline: Some(pipeline),
            desc_layout: Some(uniform_set_layout),
            additional_layouts: Vec::new(),
            texture_set_layout: None, // No per-material texture layout for bindless
            skeleton_set_layout: Some(skeleton_set_layout),
            push_descriptor_set: None,
            is_bindless: true,
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

    /// Destroy the pipeline resources (but NOT the descriptor set layout).
    ///
    /// This is used during hot reload when the descriptor set layout is
    /// preserved and owned by the MaterialTemplate.
    pub fn destroy_preserving_layout(&mut self) {
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.destroy();
        }
        // Remove descriptor set layouts - they're owned by MaterialTemplate and will be destroyed there
        let _ = self.desc_layout.take();
        let _ = self.texture_set_layout.take();
    }

    pub fn destroy(&mut self) {
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
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
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
