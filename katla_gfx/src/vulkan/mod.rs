pub mod bda;
pub mod bindless_texture;
pub mod commandbuffer;
pub mod commandpool;
pub mod context;
pub mod descriptor;
pub mod descriptor_set;
pub mod material;
pub mod particle_buffer;
pub mod pipeline_state;
pub mod queue;
pub mod skeleton_buffer;
pub mod swapchain;
pub mod swapdata;
pub mod texture;
pub mod vertex_attr_set;
pub mod vertex_attribute;
pub mod vertexbinding;
pub mod vertexbuffer;

// Re-export commonly used types from submodules for internal crate access
pub use bda::DeviceAddressBuffer;
pub use bindless_texture::BindlessTextureManager;
pub(crate) use bindless_texture::{
    DEFAULT_ALBEDO_SLOT, DEFAULT_AO_SLOT, DEFAULT_EMISSION_SLOT, DEFAULT_MR_SLOT,
    DEFAULT_NORMAL_SLOT, DEFAULT_TEXTURE_COUNT,
};
pub(crate) use commandbuffer::CommandBuffer;
pub use commandpool::CommandPool;
pub use context::ValidationLevel;
pub(crate) use context::{ValidationMessage, ValidationMessageType, ValidationSeverity};
pub(crate) use descriptor::{DescriptorSetLayoutBuilder, LayoutBinding};
pub(crate) use descriptor_set::{DescriptorSet, DescriptorSetBuilder};
pub use material::storage_uniform::{
    FrameUniforms, ObjectUniforms, StorageDescriptorSet, StorageUniformLayout,
    StorageUniformManager,
};
pub use material::{
    MaterialBuilder, MaterialDescriptor, MaterialError, MaterialValue, RenderState, ShaderBinding,
    ShaderSource,
};
pub use particle_buffer::{
    calculate_workgroup_count, EmitterConfig, EmitterConfigBuffer, ParticleBuffer, ParticleData,
    MAX_PARTICLES,
};
// Internal pipeline state types - not exposed publicly
pub(crate) use pipeline_state::{
    ColorComponentFlags, DescriptorType, DynamicState, PrimitiveTopology, ShaderStages,
    VertexInputRate,
};
// Re-export Katla-native types from pipeline module for internal use
pub(crate) use crate::pipeline::{
    BlendFactor, BlendOp, CompareOp, CullMode, FrontFace, PolygonMode,
};
pub use queue::Queue;
pub use skeleton_buffer::{JointMatrix, SkeletonBuffer, MAX_JOINTS};
pub use swapchain::{Swapchain, SwapchainInfo};
pub use swapdata::SwapData;
pub use texture::Texture;
pub use vertex_attr_set::VertexAttributeSet;
pub use vertex_attribute::{AttributeBinding, AttributeType};
pub use vertexbinding::{
    get_pbr_vertex_binding, get_skinned_vertex_binding, VertexBinding, VertexFormat,
};
pub use vertexbuffer::{IndexBuffer, IndexType, VertexBuffer};
