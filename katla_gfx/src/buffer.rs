//! GPU storage primitives.

pub use crate::vulkan::bda::DeviceAddressBuffer;
pub use crate::vulkan::particle_buffer::{
    calculate_workgroup_count, EmitterConfig, EmitterConfigBuffer, ParticleBuffer, ParticleData,
    MAX_PARTICLES,
};
pub use crate::vulkan::skeleton_buffer::{JointMatrix, SkeletonBuffer, MAX_JOINTS};
pub use crate::vulkan::vertexbuffer::{IndexBuffer, IndexType, VertexBuffer};
