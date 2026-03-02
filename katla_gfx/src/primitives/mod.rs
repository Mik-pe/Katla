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
//! // Generate a 10x10 plane (XZ, horizontal)
//! let (vertices, indices) = primitives::generate_plane(10.0, 10.0);
//!
//! // Generate a 2x2 XY plane with 4x4 tessellation (vertical, facing +Z)
//! let (vertices, indices) = primitives::generate_plane_xy(2.0, 2.0, 4);
//! ```

mod cube;
mod cylinder;
mod plane;
mod sphere;
mod torus;

pub use cube::generate_cube;
pub use cylinder::generate_cylinder;
pub use plane::generate_plane;
pub use plane::generate_plane_xy;
pub use sphere::generate_sphere;
pub use torus::generate_torus;
