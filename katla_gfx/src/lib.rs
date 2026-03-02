//! Katla Graphics Library

// Public modules
pub mod error;
pub mod handle;
pub mod material;
pub mod pipeline;
pub mod renderer;
pub mod texture;

// Internal implementation (not public)
pub(crate) mod vulkan;

// Internal modules (implementation details)
pub(crate) mod buffer;
pub(crate) mod mesh;
pub(crate) mod sync;
pub(crate) mod viewport;

// Internal re-exports for crate-wide access
pub(crate) use vulkan::bindless_texture::BindlessTextureManager;
pub(crate) use vulkan::bindless_texture::MAX_BINDLESS_TEXTURES;
pub(crate) use vulkan::context::VulkanFrameCtx;
pub(crate) use vulkan::material::SkeletonDescriptorSet;
pub(crate) use vulkan::material::storage_uniform::{StorageDescriptorSet, StorageUniformManager};
pub(crate) use vulkan::swapdata::SwapData;
pub(crate) use vulkan::texture::Texture;
pub(crate) use vulkan::vertexbinding::VertexBinding;
pub(crate) use vulkan::vertexbuffer::{IndexBuffer, VertexBuffer};

// Graphics primitives (from ash::vk)
pub use ash::vk::Extent2D;

// Error handling
pub use error::RendererError;

// Handles
pub use handle::{
    DescriptorSetHandle, Handle, MaterialHandle, MeshHandle, PipelineHandle, PipelineLayoutHandle,
    SkeletonHandle, TextureHandle,
};

// Material system
pub use material::{
    BindlessPbrMaterialConfig, BindlessSkinnedPbrMaterialConfig, DynamicMaterialConfig,
    FullPbrMaterialConfig, MaterialCacheError, MaterialCacheStats, MaterialDefinition,
    MaterialDomain, MaterialKey, MaterialPipelineCache, PbrMaterialConfig, RenderState,
    ShaderSource, SkinnedPbrMaterialConfig,
};

// Pipeline building
pub use pipeline::{
    BlendFactor, BlendOp, ColorComponentFlags, CompareOp, ComputePipeline, ComputePipelineBuilder,
    ComputePipelineError, CullMode, DescriptorSet, DescriptorSetBuilder, DescriptorSetFlags,
    DescriptorSetLayoutBuilder, DescriptorType, DynamicState, FrontFace, InstanceError,
    LayoutBinding, Material, MaterialPipeline, MaterialTemplate, Pipeline, PipelineBuilder,
    PipelineError, PolygonMode, PrimitiveTopology, ShaderCache, ShaderError, ShaderModule,
    ShaderStages, VertexInputRate,
};

// Rendering
pub use renderer::{
    AssetRegistry, DrawCall, DrawList, FRAMES_IN_FLIGHT, FrameData, FrameUniforms, InstanceData,
    ParticleDispatch, ParticleRender, VulkanRenderer,
};

// Texture management
pub use texture::{ImageFormat, TextureDescriptor, TextureManager, TextureUsage};

// Bindless texture constants
pub use vulkan::bindless_texture::{
    DEFAULT_ALBEDO_SLOT, DEFAULT_AO_SLOT, DEFAULT_EMISSION_SLOT, DEFAULT_MR_SLOT,
    DEFAULT_NORMAL_SLOT, DEFAULT_TEXTURE_COUNT,
};

// Context (for advanced use)
pub use vulkan::commandbuffer::CommandBuffer;
pub use vulkan::context::{
    ValidationMessage, ValidationMessageType, ValidationSeverity, VulkanContext,
};
pub use vulkan::material::MaterialRegistry;
