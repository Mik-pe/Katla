//! Katla Graphics Library
//!
//! This is the graphics API layer for the Katla engine. It provides a Vulkan-based
//! rendering system with a focus on ergonomics and performance.
//!
//! # Getting Started
//!
//! ## Creating the Renderer
//!
//! ```ignore
//! let renderer = VulkanRenderer::init(
//!     &event_loop,
//!     &window,
//!     true,  // validation layers
//!     CString::new("My App").unwrap(),
//!     CString::new("Katla Engine").unwrap(),
//! )?;
//! ```
//!
//! ## Creating Materials
//!
//! See [`material::API`](material/API.html) for complete material creation guide.
//!
//! ```ignore
//! use katla_gfx::{MaterialOptions, VertexType};
//!
//! let material = renderer.compile_material(
//!     "shaders/pbr.wgsl",
//!     MaterialOptions {
//!         vertex_type: VertexType::Pbr,
//!         ..Default::default()
//!     },
//! )?;
//! ```
//!
//! ## Building Frame Graphs
//!
//! See [`render_graph::API`](render_graph/API.html) for frame graph usage guide.
//!
//! ```ignore
//! let graph = renderer.create_frame_graph()
//!     .add_pass(GeometryPass::new("geometry")
//!         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
//!         .write_depth("depth", ImageFormat::D32Sfloat))
//!     .build()?;
//!
//! renderer.render(&mut graph, |frame| {
//!     frame.submit("geometry", &draw_list);
//! })?;
//! ```
//!
//! # Bindless Texture System
//!
//! The renderer uses a bindless texture system where all textures are stored in a single
//! descriptor array accessed by index. This eliminates per-material descriptor bindings
//! and enables efficient texture management.
//!
//! ## Querying Bindless Texture Information
//!
//! For advanced use cases such as debugging and texture inspection tools, the API provides
//! methods to query bindless texture slot information:
//!
//! ```ignore
//! use katla_gfx::{VulkanRenderer, TextureHandle};
//!
//! // Get the bindless slot index for a texture handle
//! if let Some(slot) = renderer.get_bindless_slot(texture_handle) {
//!     println!("Texture is at bindless slot {}", slot);
//! }
//!
//! // Get the texture handle at a specific slot
//! if let Some(handle) = renderer.get_texture_at_slot(10) {
//!     println!("Texture at slot 10: {:?}", handle);
//! }
//!
//! // Iterate over all registered textures with their slots
//! for (handle, slot) in renderer.iter_bindless_textures() {
//!     println!("Texture {:?} is at slot {}", handle, slot);
//! }
//!
//! // Get bindless slot utilization statistics
//! let (occupied, available, total) = renderer.get_bindless_stats();
//! println!("Bindless slots: {}/{} used", occupied, total);
//!
//! // Get the font atlas bindless slot
//! if let Some(slot) = renderer.get_font_atlas_bindless_slot() {
//!     println!("Font atlas is at slot {}", slot);
//! }
//! ```
//!
//! ## Advanced: Direct Bindless Access
//!
//! For low-level rendering code, you can access the bindless system directly:
//!
//! - [`VulkanRenderer::get_texture_bindless_index()`] - Get slot index for a texture
//! - [`VulkanRenderer::get_bindless_slot()`] - Get slot index (returns Option)
//! - [`VulkanRenderer::get_texture_at_slot()`] - Reverse lookup: slot → texture
//! - [`VulkanRenderer::iter_bindless_textures()`] - Iterate all registered textures
//! - [`VulkanRenderer::get_bindless_stats()`] - Get slot utilization stats
//! - [`VulkanRenderer::get_font_atlas_bindless_slot()`] - Get font atlas slot
//!
//! # API Organization
//!
//! ## Core Types
//!
//! - [`VulkanRenderer`] - Main renderer, create once at startup
//! - [`MaterialHandle`] / [`MeshHandle`] / [`TextureHandle`] - Opaque resource handles
//! - [`RendererError`] - Error type for renderer operations
//!
//! ## Material System
//!
//! - [`compile_material()`](VulkanRenderer::method.compile_material) - Create materials from shaders
//! - [`MaterialOptions`] - Configure material properties
//! - [`VertexType`] - Select vertex format (PBR, UI, Skinned, Simple)
//!
//! ## Frame Graph
//!
//! - [`FrameGraph`] - Compiled render pipeline
//! - [`FrameGraphBuilder`] - Builder for creating frame graphs
//! - [`GeometryPass`] - 3D geometry rendering
//! - [`FullscreenPass`] - Post-processing effects
//! - [`UIPass`] - 2D UI rendering
//! - [`ShadowPass`] - Shadow map generation
//!
//! # Documentation Guides
//!
//! - [Material API Guide](material/API.html) - Creating and using materials
//! - [Frame Graph API Guide](render_graph/API.html) - Building render pipelines
//!
//! # Module Organization
//!
//! The library is organized into:
//!
//! - **Public API** - [`renderer`], [`render_graph`], [`material`], [`texture`]
//! - **Internal** - `vulkan`, `pipeline`, `sync` (implementation details)
//!
//! # Resource Handles
//!
//! Most resources use opaque handles for type safety and flexibility:
//! - [`MeshHandle`] - Created via [`VulkanRenderer::create_mesh()`]
//! - [`MaterialHandle`] - Created via [`VulkanRenderer::compile_material()`]
//! - [`TextureHandle`] - Created via [`VulkanRenderer::create_texture()`]
//! - [`SkeletonHandle`] - Created via [`VulkanRenderer::create_skeleton()`]
//!

// Public modules
pub mod error;
pub mod handle;
pub mod lighting;
pub mod material;
pub mod particles;
pub mod render_pass;
pub mod renderer;
pub mod shadow;
pub mod texture;
pub mod vertex;

// Internal modules (pipeline state is implementation detail)
pub(crate) mod pipeline;

// Internal implementation (primitive mesh generators - use VulkanRenderer::create_*_mesh instead)
pub(crate) mod primitives;

// Render graph system
pub mod compute;
pub mod render_graph;

// Internal implementation (not public)
pub(crate) mod vulkan;

// Internal modules (implementation details)
pub(crate) mod barrier;
pub(crate) mod buffer;
pub(crate) mod mesh;
pub mod sync;
pub(crate) mod viewport;

// Re-export ShaderCache for examples and tests
pub use vulkan::material::shadermodule::ShaderCache;

// Re-export compute pipeline types for external compute dispatch
pub use vulkan::material::compute_pipeline::{
    ComputePipeline, ComputePipelineBuilder, ComputePipelineError,
};

// Internal re-exports for crate-wide access
pub(crate) use vulkan::bindless_texture::BindlessTextureManager;
pub(crate) use vulkan::bindless_texture::MAX_BINDLESS_TEXTURES;
pub(crate) use vulkan::context::VulkanFrameCtx;
pub(crate) use vulkan::material::SkeletonDescriptorSet;
pub(crate) use vulkan::material::storage_uniform::StorageDescriptorSet;
pub(crate) use vulkan::material::storage_uniform::StorageUniformManager;
pub(crate) use vulkan::skeleton_buffer::SkeletonBuffer;
pub(crate) use vulkan::swapdata::SwapData;
pub(crate) use vulkan::vertexbinding::VertexBinding;
pub(crate) use vulkan::vertexbuffer::{IndexBuffer, VertexBuffer};

// Size type (Katla-native)
mod size;

pub use crate::size::Size2D;

// Error handling
pub use error::RendererError;

// Handles
pub use handle::{
    EmitterHandle, Handle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle,
};

// Material system
pub use material::MaterialDomain;

// Material creation API (used by application layer)
pub use vulkan::material::compiler::{MaterialBuilder, MaterialOptions, VertexType};

// Texture management
pub use texture::{ImageFormat, TextureDescriptor, TextureUsage};

// Vertex types (public module for discoverability and extensibility)
pub use vertex::{VertexPBR, VertexPBRSkinned, VertexUI};

// SOA vertex attribute types
pub use vulkan::vertex_attribute::AttributeType;

// Render pass system
pub use render_pass::{
    AttachmentInfo, AttachmentResources, BarrierKind, ClearValue, LoadOp, StoreOp,
};

// UI rendering types
pub use renderer::{UIDrawList, UiDrawCommand};

// Renderer
pub use renderer::{DrawList, VulkanRenderer};

// Modern particle system
pub use particles::{EmitterConfig, GlobalParticleSystem};

// Forward+ lighting system
pub use lighting::{LightCullFrameData, LightCullingBuffers, PointLightGPU};

// Render graph system - minimal public API
pub use render_graph::{
    CompositingDescriptorSet, Frame, FrameGraph, FrameGraphBuilder, FullscreenPass, GeometryPass,
    GraphResourceDesc, GraphResourceType, RenderGraphError, ShadowPass, TonemapOperator,
    TonemapParams,
};

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
/// - [`VulkanRenderer::create_texture()`] for texture operations
/// - [`VulkanRenderer::create_viewport()`] for render targets
///
/// # When to use VulkanContext (escape hatch)
///
/// - Implementing custom render passes not covered by the high-level API
/// - Direct GPU memory allocation for specialized buffers
/// - Accessing Vulkan physical device properties and limits
/// - Integrating with external Vulkan libraries
///
/// # Example: Custom pipeline state
///
/// If you need pipeline state configuration beyond what `create_pbr_material()` provides,
/// you can use the low-level Vulkan context:
///
/// ```ignore
/// use katla_gfx::{VulkanContext, MaterialOptions, VertexType};
/// use ash::vk;
///
/// // Get the context (escape hatch)
/// let context = renderer.context();
/// let device = context.device();
///
/// // Create custom pipeline state with specific blend modes
/// let blend_state = vk::PipelineColorBlendAttachmentState::default()
///     .blend_enable(true)
///     .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
///     .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
///     .color_blend_op(vk::BlendOp::ADD);
///
/// // ... use with Vulkan API directly
/// ```
///
/// [`VulkanRenderer`]: renderer::VulkanRenderer
/// [`VulkanRenderer::compile_material()`]: renderer::VulkanRenderer::compile_material
/// [`VulkanRenderer::create_mesh()`]: renderer::VulkanRenderer::create_mesh
/// [`VulkanRenderer::create_texture()`]: renderer::VulkanRenderer::create_texture
/// [`VulkanRenderer::create_skeleton()`]: renderer::VulkanRenderer::create_skeleton
pub use vulkan::context::{ValidationLevel, ValidationMode, VulkanContext};
