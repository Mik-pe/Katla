//! Primitive shape generators for mesh creation.
//!
//! This module provides pure functions that generate raw vertex and index data
//! for common primitive shapes. No GPU context is required - these functions
//! simply return `Vec<VertexPBR>` and `Vec<u32>` that can be passed to
//! [`crate::VulkanRenderer::create_mesh`].
//!
//! # Example
//!
//! ```
//! use katla_gfx::primitives;
//! use katla_gfx::VertexPBR;
//!
//! // Generate a 2x2x2 cube
//! let (vertices, indices): (Vec<VertexPBR>, Vec<u32>) = primitives::generate_cube([2.0, 2.0, 2.0]);
//!
//! // Generate a sphere with radius 1.0, 32 segments, 16 rings
//! let (vertices, indices) = primitives::generate_sphere(1.0, 32, 16);
//!
//! // Generate a 10x10 plane
//! let (vertices, indices) = primitives::generate_plane(10.0, 10.0);
//! ```

mod cube;
mod plane;
mod sphere;

pub use cube::generate_cube;
pub use plane::generate_plane;
pub use sphere::generate_sphere;
