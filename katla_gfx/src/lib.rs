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

// Size type (Katla-native)
mod size;

pub use crate::size::Size2D;

// Error handling
pub use error::RendererError;

// Handles
pub use handle::{
    DescriptorSetHandle, Handle, MaterialHandle, MeshHandle, PipelineHandle, PipelineLayoutHandle,
    SkeletonHandle, TextureHandle,
};

// Material system
pub use material::{
    MaterialDefinition, MaterialDomain, MaterialKey, PbrMaterialConfig, PbrMaterialFlags,
    RenderState, ShaderSource,
};

// Pipeline building - Katla-native types only
pub use pipeline::{
    BlendFactor, BlendOp, CompareOp, ComputePipeline, ComputePipelineBuilder, ComputePipelineError,
    CullMode, FrontFace, InstanceError, Material, MaterialPipeline, MaterialTemplate, Pipeline,
    PipelineBuilder, PipelineError, PolygonMode, ShaderCache, ShaderError, ShaderModule,
    ShaderStageFlags, VertexAttributeFormat, VertexLayout,
};

// Rendering
pub use renderer::{
    AssetRegistry, BindlessDefaults, DrawCall, DrawList, FrameUniforms, InstanceData,
    ParticleDispatch, ParticleRender, VulkanRenderer,
};

// Texture management
pub use texture::{ImageFormat, TextureDescriptor, TextureManager, TextureUsage};

// Context - advanced escape hatch for low-level Vulkan access.
// Use this only when you need direct access to Vulkan device, allocator,
// or other low-level resources that aren't exposed through the high-level API.
pub use vulkan::context::{ValidationLevel, VulkanContext};
