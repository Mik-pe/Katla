//! Katla Graphics Library

// Public modules
pub mod error;
pub mod handle;
pub mod material;
pub mod pipeline;
pub mod render_pass;
pub mod renderer;
pub mod texture;
pub mod vertex;

// Internal implementation (primitive mesh generators - use VulkanRenderer::create_*_mesh instead)
pub(crate) mod primitives;

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

// Public re-exports of wrapper types for cross-crate use
pub use sync::{VkImage, VkImageView};

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
pub use material::{MaterialDomain, MaterialInstance, MaterialKey, RenderState, ShaderSource};

// Pipeline building - Katla-native types only
pub use pipeline::{
    BlendFactor, BlendOp, CompareOp, ComputePipeline, ComputePipelineBuilder, ComputePipelineError,
    CullMode, FrontFace, MaterialPipeline, MaterialTemplate, Pipeline, PipelineBuilder,
    PipelineError, PolygonMode, ShaderCache, ShaderError, ShaderModule, ShaderStageFlags,
    VertexAttributeFormat, VertexLayout,
};

// Texture management
pub use texture::{ImageFormat, TextureDescriptor, TextureManager, TextureUsage};

// Vertex types
pub use vertex::{
    Vertex, VertexPBR, VertexPBRSkinned, VertexPosition, VertexPositionColor, VertexPositionNormal,
    VertexPositionNormalUV,
};

// Render pass system
pub use render_pass::{
    AttachmentInfo, AttachmentResources, BarrierKind, ClearValue, LoadOp, StoreOp,
};

// Vulkan command buffer (needed for render pass execution)
pub use vulkan::commandbuffer::CommandBuffer;

/// Low-level Vulkan context - an escape hatch for advanced use cases.
///
/// `VulkanContext` provides direct access to Vulkan device, allocator, and other
/// low-level resources. Use this only when the high-level API is insufficient.
///
/// # When to use the high-level API instead
///
/// Most operations should use [`VulkanRenderer`] methods:
/// - [`VulkanRenderer::create_mesh()`] for mesh creation
/// - [`VulkanRenderer::register_material()`] for material registration
/// - [`VulkanRenderer::texture_manager()`] for texture operations
/// - [`VulkanRenderer::create_viewport()`] for render targets
///
/// # When to use VulkanContext (escape hatch)
///
/// - Implementing custom render passes not covered by the high-level API
/// - Direct GPU memory allocation for specialized buffers
/// - Accessing Vulkan physical device properties and limits
/// - Integrating with external Vulkan libraries
///
/// [`VulkanRenderer`]: renderer::VulkanRenderer
/// [`VulkanRenderer::create_mesh()`]: renderer::VulkanRenderer::create_mesh
/// [`VulkanRenderer::register_material()`]: renderer::VulkanRenderer::register_material
/// [`VulkanRenderer::texture_manager()`]: renderer::VulkanRenderer::texture_manager
/// [`VulkanRenderer::create_viewport()`]: renderer::VulkanRenderer::create_viewport
pub use vulkan::context::{ValidationLevel, VulkanContext};
