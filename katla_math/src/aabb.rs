use crate::Vec3;

#[derive(Clone)]
pub struct AABB {
    pub center: Vec3,
    pub extent: Vec3,
}

impl AABB {
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

    pub fn create_from_verts(verts: &[Vec3]) -> Self {
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

        //Bruteforcing is ok sometimes!
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
        let extent = (max - min).mul(0.5);
        Self {
            center: min + extent,
            extent,
        }
    }
}
