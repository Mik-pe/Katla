//! 3-dimensional vector (scalar implementation)
//!
//! Vec3 uses a scalar implementation for better cache efficiency.
//! SSE is not beneficial for 3-component vectors due to register space waste.

pub use crate::scalar::vec3::Vec3;
