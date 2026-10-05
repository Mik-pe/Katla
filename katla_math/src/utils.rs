//! Mathematical utility functions

use crate::Vec3;

/// Compute axis-aligned bounding min/max from a set of 3D vertices.
/// Returns (min, max) where min/max are Vec3 of the component-wise extremes.
pub fn compute_bounds(verts: &[Vec3]) -> (Vec3, Vec3) {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

    for vert in verts {
        if vert[0] > max[0] {
            max[0] = vert[0];
        }
        if vert[1] > max[1] {
            max[1] = vert[1];
        }
        if vert[2] > max[2] {
            max[2] = vert[2];
        }
        if vert[0] < min[0] {
            min[0] = vert[0];
        }
        if vert[1] < min[1] {
            min[1] = vert[1];
        }
        if vert[2] < min[2] {
            min[2] = vert[2];
        }
    }

    (min, max)
}
