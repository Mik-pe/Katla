use crate::{Vec3, compute_bounds};

#[derive(Clone, Copy, Debug)]
pub struct AABB {
    pub center: Vec3,
    pub extent: Vec3,
}

impl AABB {
    // An intersection test between two AABBs.
    pub fn intersects(&self, other: &AABB) -> bool {
        if (self.center[0] - other.center[0]).abs() > (self.extent[0] + other.extent[0]) {
            return false;
        }
        if (self.center[1] - other.center[1]).abs() > (self.extent[1] + other.extent[1]) {
            return false;
        }
        if (self.center[2] - other.center[2]).abs() > (self.extent[2] + other.extent[2]) {
            return false;
        }

        true
    }

    // This is a helper function to create an AABB from a list of vertices.
    pub fn create_from_verts(verts: &[Vec3]) -> Self {
        let (min, max) = compute_bounds(verts);
        let extent = (max - min).mul(0.5);
        Self {
            center: min + extent,
            extent,
        }
    }
}
