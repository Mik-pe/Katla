//! GPU storage primitives.

pub use crate::vulkan::bda::DeviceAddressBuffer;
pub use crate::vulkan::particle_buffer::{
    EmitterConfig, EmitterConfigBuffer, MAX_PARTICLES, ParticleBuffer, ParticleData,
    calculate_workgroup_count,
};
pub use crate::vulkan::skeleton_buffer::{JointMatrix, MAX_JOINTS, SkeletonBuffer};
pub use crate::vulkan::vertexbuffer::{IndexBuffer, IndexType, VertexBuffer};
