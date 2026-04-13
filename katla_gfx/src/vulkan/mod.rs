pub(crate) mod bda;
pub(crate) mod bindless_texture;
pub(crate) mod commandbuffer;
pub(crate) mod commandpool;
pub(crate) mod context;
pub(crate) mod descriptor_set;
pub(crate) mod material;
pub(crate) mod pipeline_state;
pub(crate) mod queue;
pub(crate) mod skeleton_buffer;
pub(crate) mod swapchain;
pub(crate) mod swapdata;
pub(crate) mod texture;
pub(crate) mod thread_pool_command_pool;
pub(crate) mod vertex_attr_set;
pub(crate) mod vertex_attribute;
pub(crate) mod vertexbinding;
pub(crate) mod vertexbuffer;

// Re-export commonly used types from submodules for internal crate access
pub(crate) use commandbuffer::CommandBuffer;
pub use commandpool::CommandPool;
pub(crate) use descriptor_set::DescriptorSet;
// Internal pipeline state types - not exposed publicly
// Re-export Katla-native types from pipeline module for internal use
pub use queue::Queue;
pub use swapchain::{Swapchain, SwapchainInfo};
pub use vertexbuffer::{IndexBuffer, IndexType, VertexBuffer};
