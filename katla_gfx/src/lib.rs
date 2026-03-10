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
pub mod material;
pub mod render_pass;
pub mod renderer;
pub mod texture;
pub mod vertex;

// Internal modules (pipeline state is implementation detail)
pub(crate) mod pipeline;

// Internal implementation (primitive mesh generators - use VulkanRenderer::create_*_mesh instead)
pub(crate) mod primitives;

// Render graph system
pub mod render_graph;

// Internal implementation (not public)
pub(crate) mod vulkan;

// Internal modules (implementation details)
pub(crate) mod barrier;
pub(crate) mod buffer;
pub(crate) mod mesh;
pub(crate) mod sync;
pub(crate) mod viewport;

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
pub use handle::{Handle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};

// Material system
pub use material::{Material, MaterialDomain, RenderState, ShaderSource};

// Material creation API (used by application layer)
pub use vulkan::material::compiler::{MaterialBuilder, MaterialOptions, VertexType};

// Texture management
pub use texture::{ImageFormat, TextureDescriptor, TextureUsage};

// Vertex types (public module for discoverability and extensibility)
pub use vertex::{VertexPBR, VertexPBRSkinned, VertexUI};

// Render pass system
pub use render_pass::{
    AttachmentInfo, AttachmentResources, BarrierKind, ClearValue, LoadOp, StoreOp,
};

// UI rendering types
pub use renderer::{UIDrawList, UiDrawCommand};

// Renderer
pub use renderer::VulkanRenderer;

// Render graph system - minimal public API
pub use render_graph::{
    Frame, FrameGraph, FrameGraphBuilder, FullscreenPass, GeometryPass, GraphResourceDesc,
    GraphResourceType, LightType, RenderGraphError, ShadowPass, TonemapOperator, TonemapParams,
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
/// use katla_gfx::{VulkanContext, PipelineHandle};
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
/// [`VulkanRenderer::create_mesh()`]: renderer::VulkanRenderer::create_mesh
/// [`VulkanRenderer::register_material()`]: renderer::VulkanRenderer::register_material
/// [`VulkanRenderer::create_texture()`]: renderer::VulkanRenderer::create_texture
/// [`VulkanRenderer::create_viewport()`]: renderer::VulkanRenderer::create_viewport
pub use vulkan::context::{ValidationLevel, VulkanContext};
