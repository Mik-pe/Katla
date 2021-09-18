use crate::Vec3;

const KINDA_SMALL_NUMBER: f32 = 0.00001f32;

#[derive(Clone)]
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
            self.radius = (point - self.center).distance();
        }
    }

    pub fn point_inside(&self, point: Vec3) -> bool {
        let relative_point = point - self.center;

        (self.radius + KINDA_SMALL_NUMBER) * (self.radius + KINDA_SMALL_NUMBER)
            >= relative_point.distance_squared()
    }

    pub fn intersects(&self, other: &Self) -> bool {
        let dist_sq = (self.center - other.center).distance_squared();
        let radius_sum = self.radius + other.radius;
        dist_sq <= radius_sum * radius_sum
    }

    //Create a bounding sphere from a slice that can be made into a vec3
    pub fn create_from_verts<'a, I, T: 'a>(verts: I) -> Self
    where
        I: IntoIterator<Item = &'a T>,
        &'a T: Into<&'a Vec3>,
    {
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

        //Bruteforcing is ok sometimes!
        for t_vert in verts {
            let vert: &'a Vec3 = t_vert.into();
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
        let center = min + extent;
        let radius = extent.distance();

        Self { center, radius }
    }
}
