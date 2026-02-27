pub mod error;
pub mod handle;
pub mod material;
pub mod render_graph;
pub mod renderer;
pub mod sync;
pub mod texture;
pub mod viewport;
pub mod vulkan;

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
pub use render_graph::errors::RenderGraphError;
pub use render_graph::pass::{PassBuilder, PassExecutionContext};
pub use render_graph::resource::{
    CompiledResource, ResourceAccessType, ResourceId, ResourceKind, ResourceLifetime, ResourceUsage,
};
pub use render_graph::*;
pub use renderer::{
    AssetRegistry, DefaultRenderTargets, DrawCall, DrawList, FrameData, FrameUniforms,
    InstanceData, ParticleDispatch, ParticleRender, VulkanRenderer, FRAMES_IN_FLIGHT,
};
pub use texture::{TextureDescriptor, TextureManager, TextureUsage};
pub use viewport::{DepthFormat, OutputMode, ViewportBuilder, ViewportHandle};
pub use vulkan::context::{ValidationMessage, ValidationMessageType, ValidationSeverity};
pub use vulkan::material::storage_uniform::*;
pub use vulkan::*;
