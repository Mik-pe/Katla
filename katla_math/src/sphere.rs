use crate::Vec3;

const KINDA_SMALL_NUMBER: f32 = 0.00001f32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}

impl Sphere {
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    pub fn maybe_expand(&mut self, point: Vec3) {
        if !self.point_inside(point) {
            self.radius = (point - self.center).length();
        }
    }

    pub fn point_inside(&self, point: Vec3) -> bool {
        let relative_point = point - self.center;

        (self.radius + KINDA_SMALL_NUMBER) * (self.radius + KINDA_SMALL_NUMBER)
            >= relative_point.length_squared()
    }

    pub fn intersects(&self, other: &Self) -> bool {
        let dist_sq = (self.center - other.center).length_squared();
        let radius_sum = self.radius + other.radius;
        dist_sq <= radius_sum * radius_sum
    }

    pub fn create_from_verts<'a, I>(verts: I) -> Self
    where
        I: IntoIterator<Item = &'a [f32; 3]>,
    {
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

        for v in verts {
            let p = Vec3::new(v[0], v[1], v[2]);
            if p[0] < min[0] {
                min[0] = p[0];
            }
            if p[1] < min[1] {
                min[1] = p[1];
            }
            if p[2] < min[2] {
                min[2] = p[2];
            }
            if p[0] > max[0] {
                max[0] = p[0];
            }
            if p[1] > max[1] {
                max[1] = p[1];
            }
            if p[2] > max[2] {
                max[2] = p[2];
            }
        }

        let extent = (max - min).mul(0.5);
        let center = min + extent;
        let radius = extent.length();

        Self { center, radius }
    }
}
