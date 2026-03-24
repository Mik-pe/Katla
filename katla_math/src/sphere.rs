use crate::{Vec3, compute_bounds};

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
        let vec3s: Vec<Vec3> = verts
            .into_iter()
            .map(|v| Vec3::new(v[0], v[1], v[2]))
            .collect();
        let (min, max) = compute_bounds(&vec3s);

        let extent = (max - min).mul(0.5);
        let center = min + extent;
        let radius = extent.length();

        Self { center, radius }
    }
}
