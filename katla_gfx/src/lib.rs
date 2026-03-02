//! Katla Graphics Library

// Public modules
pub mod buffer;
pub mod error;
pub mod handle;
pub mod material;
pub mod mesh;
pub mod pipeline;
pub mod renderer;
pub mod sync;
pub mod texture;
pub mod viewport;

// Internal implementation (not public)
pub(crate) mod vulkan;

// Internal re-exports for crate-wide access
// These types are used by other modules via crate::TypeName
pub(crate) use vulkan::bindless_texture::BindlessTextureManager;
pub(crate) use vulkan::bindless_texture::MAX_BINDLESS_TEXTURES;
pub(crate) use vulkan::context::VulkanFrameCtx;
pub(crate) use vulkan::material::Material;
pub(crate) use vulkan::material::SkeletonDescriptorSet;
pub(crate) use vulkan::material::storage_uniform::{
    FrameUniforms, StorageDescriptorSet, StorageUniformManager,
};
pub(crate) use vulkan::pipeline_state::ShaderStages;
pub(crate) use vulkan::swapdata::SwapData;
pub(crate) use vulkan::texture::Texture;
pub(crate) use vulkan::vertexbinding::VertexBinding;
pub(crate) use vulkan::vertexbuffer::{IndexBuffer, VertexBuffer};

// Root convenience exports (public API)
pub use ash::vk::Extent2D;
pub use error::RendererError;
pub use handle::{
    DescriptorSetHandle, Handle, MaterialHandle, MeshHandle, PipelineHandle, PipelineLayoutHandle,
    SkeletonHandle, TextureHandle,
};
pub use material::{
    BindlessPbrMaterialConfig, BindlessSkinnedPbrMaterialConfig, DynamicMaterialConfig,
    FullPbrMaterialConfig, MaterialCacheError, MaterialCacheStats, MaterialDefinition,
    MaterialDomain, MaterialKey, MaterialPipelineCache, PbrMaterialConfig,
    SkinnedPbrMaterialConfig,
};
// MaterialRegistry is in vulkan::material, not the material module
pub use renderer::{
    AssetRegistry, DrawCall, DrawList, FRAMES_IN_FLIGHT, FrameData, InstanceData, ParticleDispatch,
    ParticleRender, VulkanRenderer,
};
pub use texture::{ImageFormat, TextureDescriptor, TextureManager, TextureUsage};
pub use vulkan::bindless_texture::{
    DEFAULT_ALBEDO_SLOT, DEFAULT_AO_SLOT, DEFAULT_EMISSION_SLOT, DEFAULT_MR_SLOT,
    DEFAULT_NORMAL_SLOT, DEFAULT_TEXTURE_COUNT,
};
pub use vulkan::commandbuffer::CommandBuffer;
pub use vulkan::context::{
    ValidationMessage, ValidationMessageType, ValidationSeverity, VulkanContext,
};
pub use vulkan::material::MaterialRegistry;
