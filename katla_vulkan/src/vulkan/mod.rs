pub mod bda;
pub mod bindless_texture;
pub mod commandbuffer;
pub mod commandpool;
pub mod context;
pub mod descriptor;
pub mod descriptor_set;
pub mod frame_buffer;
pub mod framebuffer;
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

pub use bda::*;
pub use bindless_texture::*;
pub use commandbuffer::*;
pub use commandpool::*;
// VulkanContext is now internal - only validation types are exported
pub use context::{ValidationMessage, ValidationMessageType, ValidationSeverity};
// Explicit re-exports to avoid DescriptorBinding ambiguity
pub use descriptor::DescriptorSetLayoutBuilder;
pub use descriptor_set::{DescriptorSet, DescriptorSetBuilder};
// Note: descriptor::DescriptorBinding is for layout creation
// descriptor_set::DescriptorBinding is for runtime binding - use full path when needed
pub use frame_buffer::*;
pub use framebuffer::*;
#[allow(deprecated)]
pub use material::storage_uniform::*;
// Exclude material::DescriptorBinding to avoid conflict with descriptor::DescriptorBinding
// MaterialAsset is in crate::renderer::registry, MaterialType does not exist
pub use material::{
    MaterialBuilder, MaterialDescriptor, MaterialError, MaterialValue, RenderState, ShaderSource,
};
pub use particle_buffer::*;
pub use pipeline_state::*;
pub use queue::*;
pub use skeleton_buffer::*;
pub use swapchain::*;
pub use swapdata::*;
pub use texture::*;
pub use vertex_attr_set::*;
pub use vertex_attribute::*;
pub use vertexbinding::*;
// VertexBuffer and IndexBuffer are now internal
pub use vertexbuffer::IndexType;

pub use ImageFormat;
