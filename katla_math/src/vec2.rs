//! 2-dimensional vector (scalar implementation)
//!
//! Vec2 uses a scalar implementation for better cache efficiency.
//! SSE is not beneficial for 2-component vectors due to register space waste.

pub use crate::scalar::vec2::Vec2;
