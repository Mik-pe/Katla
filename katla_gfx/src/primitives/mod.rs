//! Primitive shape generators.
//!
//! Pure CPU functions that produce vertex/index data for common shapes.
//! Use the free functions [`create_cube`], [`create_sphere`], etc. to create
//! mesh handles via any `GpuRenderer` backend.

mod cone;
mod cube;
mod cylinder;
mod plane;
mod sphere;
mod torus;

pub use cone::generate_cone;
pub use cube::generate_cube;
pub use cylinder::generate_cylinder;
pub use plane::generate_plane;
pub use plane::generate_plane_xy;
pub use sphere::generate_sphere;
pub use torus::generate_torus;

use crate::handle::MeshHandle;
use crate::renderer::gpu_renderer::GpuRenderer;

/// Create a cube mesh centered at the origin.
pub fn create_cube(renderer: &mut impl GpuRenderer, size: [f32; 3]) -> MeshHandle {
    let (vertices, indices) = generate_cube(size);
    renderer.create_mesh(&vertices, &indices)
}

/// Create a UV sphere mesh centered at the origin.
pub fn create_sphere(
    renderer: &mut impl GpuRenderer,
    radius: f32,
    segments: u32,
    rings: u32,
) -> MeshHandle {
    let (vertices, indices) = generate_sphere(radius, segments, rings);
    renderer.create_mesh(&vertices, &indices)
}

/// Create a plane mesh on the XZ plane.
pub fn create_plane(renderer: &mut impl GpuRenderer, width: f32, height: f32) -> MeshHandle {
    let (vertices, indices) = generate_plane(width, height);
    renderer.create_mesh(&vertices, &indices)
}

/// Create a cone mesh with base at y=0 and apex at y=height.
pub fn create_cone(
    renderer: &mut impl GpuRenderer,
    height: f32,
    base_radius: f32,
    segments: u32,
) -> MeshHandle {
    let (vertices, indices) = generate_cone(height, base_radius, segments);
    renderer.create_mesh(&vertices, &indices)
}

/// Create a cylinder mesh standing on Y axis.
pub fn create_cylinder(
    renderer: &mut impl GpuRenderer,
    height: f32,
    radius: f32,
    segments: u32,
) -> MeshHandle {
    let (vertices, indices) = generate_cylinder(height, radius, segments);
    renderer.create_mesh(&vertices, &indices)
}

/// Create a torus (donut) mesh on the XZ plane.
pub fn create_torus(
    renderer: &mut impl GpuRenderer,
    major_radius: f32,
    minor_radius: f32,
    segments: u32,
    rings: u32,
) -> MeshHandle {
    let (vertices, indices) = generate_torus(major_radius, minor_radius, segments, rings);
    renderer.create_mesh(&vertices, &indices)
}

/// Create a plane on the XY axis (vertical, facing +Z).
pub fn create_plane_xy(
    renderer: &mut impl GpuRenderer,
    width: f32,
    height: f32,
    segments: u32,
) -> MeshHandle {
    let (vertices, indices) = generate_plane_xy(width, height, segments);
    renderer.create_mesh(&vertices, &indices)
}
