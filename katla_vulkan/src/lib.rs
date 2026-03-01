pub mod error;
pub mod handle;
pub mod material;
pub mod renderer;
pub mod sync;
pub mod texture;
pub mod viewport;
pub mod vulkan;

// Internal re-exports for crate-wide access
pub(crate) use vulkan::bindless_texture::MAX_BINDLESS_TEXTURES;
pub(crate) use vulkan::context::VulkanFrameCtx;
pub(crate) use vulkan::material::DescriptorLayoutBuilder;
pub(crate) use vulkan::BindlessTextureManager;
pub(crate) use vulkan::SwapData;
// Now public - needed by katla_app
pub use material::MaterialRegistry;
pub use vulkan::material::template::Material;
pub use vulkan::material::SkeletonDescriptorSet;
pub use vulkan::skeleton_buffer::SkeletonBuffer;
pub use vulkan::texture::Texture;

// Public API exports
pub use error::RendererError;
pub use handle::{
    BufferHandle, DescriptorSetHandle, Handle, ImageHandle, MaterialHandle, MeshHandle,
    PipelineHandle, PipelineLayoutHandle, ResourceStorage, SkeletonHandle, TextureHandle,
};
pub use material::{
    BindlessPbrMaterialConfig, BindlessSkinnedPbrMaterialConfig, DynamicMaterialConfig,
    FullPbrMaterialConfig, MaterialCacheError, MaterialCacheStats, MaterialDefinition,
    MaterialDomain, MaterialKey, MaterialPipelineCache, PbrMaterialConfig,
    SkinnedPbrMaterialConfig,
};
pub use renderer::{
    AssetRegistry, DrawCall, DrawList, FrameData, FrameUniforms, InstanceData, ParticleDispatch,
    ParticleRender, VulkanRenderer, FRAMES_IN_FLIGHT,
};
pub use texture::{ImageFormat, TextureDescriptor, TextureManager, TextureUsage};

pub use vulkan::context::{
    ValidationMessage, ValidationMessageType, ValidationSeverity, VulkanContext,
};
pub use vulkan::material::storage_uniform::*;
// Bindless texture constants
pub use vulkan::bindless_texture::{
    DEFAULT_ALBEDO_SLOT, DEFAULT_AO_SLOT, DEFAULT_EMISSION_SLOT, DEFAULT_MR_SLOT,
    DEFAULT_NORMAL_SLOT, DEFAULT_TEXTURE_COUNT,
};
// Explicit exports from vulkan module (not wildcard)
pub use vulkan::vertexbinding::{VertexBinding, VertexFormat};
pub use vulkan::vertexbuffer::{IndexBuffer, IndexType, VertexBuffer};
// Particle utilities
pub use vulkan::particle_buffer::calculate_workgroup_count;
// Descriptor builders needed for custom descriptor sets
pub use vulkan::descriptor::DescriptorSetLayoutBuilder;
pub use vulkan::descriptor_set::{DescriptorSet, DescriptorSetBuilder};
// Pipeline state types
pub use vulkan::pipeline_state::{CompareOp, CullMode, DescriptorType, FrontFace, ShaderStages};
// Re-export vk::Extent2D for convenience
pub use ash::vk::Extent2D;
// Material pipeline types
pub use vulkan::material::{
    ComputePipelineBuilder, MaterialPipeline, MaterialTemplate, PipelineBuilder, ShaderModule,
};
// Buffer and memory types
pub use vulkan::bda::DeviceAddressBuffer;
pub use vulkan::material::buffer_descriptor::UniformBuffer;
pub use vulkan::material::compute_pipeline::ComputePipeline;
pub use vulkan::particle_buffer::{EmitterConfig, ParticleBuffer, MAX_PARTICLES};
// Framebuffer (legacy render pass)
pub use vulkan::frame_buffer::FrameBuffer;
// Command buffer (needed for render graph execution)
pub use vulkan::commandbuffer::CommandBuffer;
