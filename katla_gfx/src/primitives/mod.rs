//! Primitive shape generators (internal).
//!
//! Used internally by [`VulkanRenderer::create_*_mesh`] convenience methods.
//! External users should use those methods instead.

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
