pub mod bda;
pub mod bindless_texture;
pub mod commandbuffer;
pub mod commandpool;
pub mod context;
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
pub(crate) use commandbuffer::CommandBuffer;
pub use commandpool::CommandPool;
pub(crate) use descriptor_set::DescriptorSet;
// Internal pipeline state types - not exposed publicly
// Re-export Katla-native types from pipeline module for internal use
pub use queue::Queue;
pub use swapchain::{Swapchain, SwapchainInfo};
pub use vertexbuffer::{IndexBuffer, IndexType, VertexBuffer};
