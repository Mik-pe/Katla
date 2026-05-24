//! Katla Graphics Library
//!
//! This is the graphics API layer for the Katla engine. It provides a
//! cross-backend rendering system supporting Vulkan and Metal, with a focus
//! on ergonomics and performance.
//!
//! # Backend Selection
//!
//! The crate supports two rendering backends:
//! - **Vulkan** — via `ash`, available on all platforms
//! - **Metal** — via `objc2-metal`, native on macOS (`cfg(target_os = "macos")`)
//!
//! Use [`AnyRenderer`] for runtime backend selection, or use `VulkanRenderer` /
//! `MetalRenderer` directly for compile-time backend commitment.
//!
//! # Getting Started
//!
//! ## Creating the Renderer
//!
//! ```ignore
//! // Vulkan backend
//! let renderer = AnyRenderer::new_vulkan(
//!     &display, &window,
//!     ValidationMode::Full,
//!     CString::new("My App").unwrap(),
//!     CString::new("Katla Engine").unwrap(),
//! )?;
//!
//! // Metal backend (macOS only)
//! let renderer = AnyRenderer::new_metal(
//!     &display, &window,
//!     ValidationMode::Full,
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
//! // Via GpuRenderer trait (backend-agnostic)
//! let material = renderer.compile_material("shaders/pbr.wgsl", "pbr")?;
//!
//! // Vulkan-specific options (bypasses trait)
//! let material = vulkan_renderer.compile_material(
//!     "shaders/pbr.wgsl",
//!     MaterialOptions { vertex_type: VertexType::Pbr, ..Default::default() },
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
//!     frame.submit(geometry_pass_id, &draw_list);
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
//! For low-level rendering code, bindless texture queries are available through
//! the `GpuRenderer` trait (backend-agnostic) or directly on backend renderers:
//!
//! - [`GpuRenderer::get_bindless_slot()`] - Get slot index for a texture
//! - [`GpuRenderer::get_texture_at_slot()`] - Reverse lookup: slot → texture
//! - [`GpuRenderer::get_texture_bindless_index()`] - Get slot index (returns 0 when unregistered)
//!
//! # API Organization
//!
//! ## Core Types
//!
//! - [`AnyRenderer`] - Runtime backend dispatch (Vulkan | Metal)
//! - [`GpuRenderer`] - Backend-agnostic renderer trait
//! - [`VulkanRenderer`] / [`MetalRenderer`] - Backend-specific renderers
//! - [`MaterialHandle`] / [`MeshHandle`] / [`TextureHandle`] - Opaque resource handles
//! - [`RendererError`] - Error type for renderer operations
//!
//! ## Material System
//!
//! - [`GpuRenderer::compile_material()`] - Create materials from shaders (backend-agnostic)
//! - [`MaterialOptions`] - Configure material properties (Vulkan-specific)
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
//! - **Backends** - `vulkan` (Vulkan via ash), `metal` (Metal via objc2-metal, macOS only)
//! - **Internal** - `pipeline`, `sync`, `animation`, `shadow`, `lighting` (implementation details)
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
pub mod particles;
pub mod render_pass;
pub mod renderer;
pub mod texture;
pub mod vertex;

// Internal modules (pipeline state is implementation detail)
#[allow(dead_code)]
pub(crate) mod backend;
pub(crate) mod pipeline;

// Re-export pipeline state types for validation examples
#[cfg(feature = "validation")]
pub use pipeline::{CompareOp, CullMode, FrontFace};

// Primitive mesh generators — use primitives::create_cube() etc. for backend-agnostic mesh creation
pub mod primitives;

// Render graph system
pub mod compute;
pub mod render_graph;

// Animation module — always available. Internal Vulkan implementations are self-gated.
pub mod animation;

// Lighting — Vulkan-specific internals
#[cfg(feature = "validation")]
pub mod lighting;
#[cfg(not(feature = "validation"))]
pub(crate) mod lighting;

// Shadow module — always available. Internal Vulkan implementations are self-gated.
pub mod shadow;

// Sync — Vulkan-specific
#[cfg(feature = "validation")]
pub mod sync;
#[cfg(not(feature = "validation"))]
pub(crate) mod sync;

pub(crate) mod vulkan;

#[cfg(target_os = "macos")]
pub(crate) mod metal;

// Re-export animation types — shared data types always available
pub use animation::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};
pub use animation::{PoseComputeBuffers, PoseComputePipeline};

// Re-export types used by katla_app
pub use renderer::PointLightGPU;
pub use shadow::cascade::CascadeParams;

// Internal modules (implementation details)
pub(crate) mod barrier;
pub(crate) mod gpu_buffer;
pub(crate) mod viewport;

// Re-export viewport types (backend-agnostic)
pub use viewport::{DepthFormat, OutputMode, Viewport, ViewportBuilder, ViewportHandle};

// Re-export ShaderCache for examples and tests
#[cfg(feature = "validation")]
pub use vulkan::material::shadermodule::ShaderCache;

// Re-export compute pipeline types for external compute dispatch
pub use vulkan::material::compute_pipeline::{
    ComputePipeline, ComputePipelineBuilder, ComputePipelineError,
};

// Re-export pipeline builder and types for validation examples
#[cfg(feature = "validation")]
pub use vulkan::material::builder::Pipeline;
#[cfg(feature = "validation")]
pub use vulkan::material::builder::PipelineBuilder;
#[cfg(feature = "validation")]
pub use vulkan::vertexbinding::VertexFormat;

// Re-export for validation examples and advanced compute usage
#[cfg(feature = "validation")]
pub use vulkan::commandbuffer::CommandBuffer;
#[cfg(feature = "validation")]
pub use vulkan::pipeline_state::ShaderStages;

// Size type (Katla-native)
mod size;

pub use crate::size::Size2D;

// Error handling
pub use error::RendererError;
pub use error::ValidationMode;

// Handles
pub use handle::{
    EmitterHandle, Handle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle,
};

// Material system
pub use material::MaterialDomain;

// Material creation API (used by application layer)
pub use vulkan::material::compiler::{MaterialOptions, VertexType};

// Texture management
pub use texture::{ImageFormat, TextureDescriptor, TextureUsage};

// Vertex types (public module for discoverability and extensibility)
pub use vertex::{VertexPBR, VertexPBRSkinned, VertexUI};

// SOA vertex attribute types (shared enum definition)
pub use vertex::AttributeType;

// Render pass system
pub use render_pass::{AttachmentInfo, BarrierKind, ClearValue, LoadOp, StoreOp};

// UI rendering types
pub use renderer::{UIDrawList, UiDrawCommand};

// Renderer types (backend-agnostic)
pub use renderer::{
    DrawCall, DrawList, FrameUniforms, GpuCapabilities, GpuTimestamp, GpuVendor, InstanceData,
};

// Renderer (Vulkan-specific)
pub use renderer::VulkanRenderer;

// Renderer (Metal-specific)
#[cfg(target_os = "macos")]
pub use metal::metal_renderer::MetalRenderer;

// Backend-agnostic renderer trait
pub use renderer::gpu_renderer::GpuRenderer;

// Enum-based renderer dispatch (both backends)
pub use renderer::any_renderer::AnyRenderer;

// Enum-based frame graph dispatch
pub use render_graph::any_frame::AnyFrame;
pub use render_graph::any_frame_graph::AnyFrameGraph;

// Modern particle system — shared config types always available
pub use particles::EmitterConfig;
pub use particles::GlobalParticleSystem;

// Render graph system — pass types and descriptors are backend-agnostic
pub use render_graph::Frame;
pub use render_graph::descriptor_sets::CompositingDescriptorSet;
pub use render_graph::{
    FullscreenPass, GeometryPass, GraphResourceDesc, GraphResourceType, OutlinePass, OverlayParams,
    OverlayPass, ParticlePass, RenderGraphError, ShadowPass, StencilIndicatorPass, TonemapOperator,
    TonemapParams,
};
/// Vulkan-specific frame graph type.
pub type FrameGraph = render_graph::FrameGraph<renderer::VulkanRenderer>;
pub use render_graph::{FrameGraphBuilder, RenderGraphBackend};

/// Low-level Vulkan context - an escape hatch for advanced Vulkan-specific use cases.
///
/// For cross-backend code, prefer using [`GpuRenderer`] trait methods instead.
/// `VulkanContext` provides direct access to Vulkan device, allocator, and other
/// low-level resources. Use this only when the high-level API is insufficient
/// and you specifically need Vulkan internals.
///
/// # When to use the high-level API instead
///
/// Most operations should use [`GpuRenderer`] trait methods:
/// - [`GpuRenderer::create_mesh()`] for mesh creation
/// - [`GpuRenderer::compile_material()`] for material compilation
/// - [`GpuRenderer::create_texture()`] for texture operations
/// - [`GpuRenderer::create_viewport()`] for render targets
///
/// These work identically on both Vulkan and Metal backends.
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
pub use vulkan::context::{ValidationLevel, VulkanContext};
