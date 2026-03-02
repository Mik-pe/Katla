use crate::{AABB, Mat4, Plane, Sphere, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayIntersection {
    pub point: Vec3,
    pub distance: f32,
    pub normal: Vec3,
}

impl Ray {
    /// Create a new ray from an origin point and direction
    /// Note: direction should be normalized for correct intersection calculations
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Ray { origin, direction }
    }

    /// Create a ray from two points (start and end)
    pub fn from_points(start: Vec3, end: Vec3) -> Self {
        let direction = (end - start).normalize();
        Ray {
            origin: start,
            direction,
        }
    }

    /// Get a point at a specific distance along the ray
    pub fn at(&self, distance: f32) -> Vec3 {
        self.origin + self.direction * distance
    }

    /// Calculate the shortest distance from the ray to a point
    pub fn distance_to_point(&self, point: Vec3) -> f32 {
        let to_point = point - self.origin;
        let projection = to_point.dot(self.direction);
        let closest = self.origin + self.direction * projection.max(0.0);
        (point - closest).length()
    }

    /// Check if the ray intersects a plane
    pub fn intersects_plane(&self, plane: &Plane) -> Option<Vec3> {
        plane.intersects_ray(self).map(|t| self.at(t))
    }

    /// Check if the ray intersects an AABB
    pub fn intersects_aabb(&self, aabb: &AABB) -> Option<RayIntersection> {
        // Slab method for ray-AABB intersection
        let mut t_min = f32::NEG_INFINITY;
        let mut t_max = f32::INFINITY;

        let inv_dir = Vec3::new(
            if self.direction.x() != 0.0 {
                1.0 / self.direction.x()
            } else {
                f32::INFINITY
            },
            if self.direction.y() != 0.0 {
                1.0 / self.direction.y()
            } else {
                f32::INFINITY
            },
            if self.direction.z() != 0.0 {
                1.0 / self.direction.z()
            } else {
                f32::INFINITY
            },
        );

        for i in 0..3 {
            let center = if i == 0 {
                aabb.center.x()
            } else if i == 1 {
                aabb.center.y()
            } else {
                aabb.center.z()
            };
            let extent = if i == 0 {
                aabb.extent.x()
            } else if i == 1 {
                aabb.extent.y()
            } else {
                aabb.extent.z()
            };
            let min = center - extent;
            let max = center + extent;

            let origin = if i == 0 {
                self.origin.x()
            } else if i == 1 {
                self.origin.y()
            } else {
                self.origin.z()
            };
            let inv_d = if i == 0 {
                inv_dir.x()
            } else if i == 1 {
                inv_dir.y()
            } else {
                inv_dir.z()
            };

            let t1 = (min - origin) * inv_d;
            let t2 = (max - origin) * inv_d;

            let (t_near, t_far) = if t1 < t2 { (t1, t2) } else { (t2, t1) };

            t_min = t_min.max(t_near);
            t_max = t_max.min(t_far);

            if t_min > t_max {
                return None;
            }
        }

        if t_min < 0.0 {
            if t_max < 0.0 {
                return None;
            }
            // Inside the box
            Some(RayIntersection {
                point: self.at(t_max),
                distance: t_max,
                normal: Vec3::new(0.0, 1.0, 0.0), // Simplified
            })
        } else {
            Some(RayIntersection {
                point: self.at(t_min),
                distance: t_min,
                normal: Vec3::new(0.0, 1.0, 0.0), // Simplified
            })
        }
    }

    /// Check if the ray intersects a sphere
    pub fn intersects_sphere(&self, sphere: &Sphere) -> Option<RayIntersection> {
        let oc = self.origin - sphere.center;
        let a = self.direction.dot(self.direction);
        let b = 2.0 * oc.dot(self.direction);
        let c = oc.dot(oc) - sphere.radius * sphere.radius;

        let discriminant: f32 = b * b - 4.0 * a * c;

        if discriminant < 0.0 {
            return None;
        }

        let sqrt_disc = f32::sqrt(discriminant);
        let t1 = (-b - sqrt_disc) / (2.0 * a);
        let t2 = (-b + sqrt_disc) / (2.0 * a);

        let t = if t1 > 0.0 {
            t1
        } else if t2 > 0.0 {
            t2
        } else {
            return None;
        };

        let point = self.at(t);
        let normal = (point - sphere.center).normalize();

        Some(RayIntersection {
            point,
            distance: t,
            normal,
        })
    }

    /// Check if the ray intersects a triangle
    pub fn intersects_triangle(&self, a: Vec3, b: Vec3, c: Vec3) -> Option<Vec3> {
        // Möller–Trumbore intersection algorithm
        let edge1 = b - a;
        let edge2 = c - a;
        let h = self.direction.cross(edge2);
        let det = edge1.dot(h);

        if det.abs() < 1e-6 {
            return None; // Ray is parallel to triangle
        }

        let inv_det = 1.0 / det;
        let s = self.origin - a;
        let u = inv_det * s.dot(h);

        if !(0.0..=1.0).contains(&u) {
            return None;
        }

        let q = s.cross(edge1);
        let v = inv_det * self.direction.dot(q);

        if v < 0.0 || u + v > 1.0 {
            return None;
        }

        let t = inv_det * edge2.dot(q);

        if t > 1e-6 { Some(self.at(t)) } else { None }
    }

    /// Transform the ray by a matrix
    pub fn transform(&self, matrix: &Mat4) -> Ray {
        let origin = matrix.clone() * self.origin;
        let direction: Vec3 = matrix.to_mat3() * self.direction;
        Ray {
            origin,
            direction: direction.normalize(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_at() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let point = ray.at(5.0);

        assert!((point.x() - 5.0).abs() < 1e-5);
        assert!((point.y() - 0.0).abs() < 1e-5);
        assert!((point.z() - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_ray_from_points() {
        let start = Vec3::new(0.0, 0.0, 0.0);
        let end = Vec3::new(5.0, 0.0, 0.0);
        let ray = Ray::from_points(start, end);

        assert!((ray.origin.x() - 0.0).abs() < 1e-5);
        assert!((ray.direction.x() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_ray_distance_to_point() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let point = Vec3::new(5.0, 3.0, 0.0);

        let dist = ray.distance_to_point(point);
        assert!((dist - 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_ray_intersects_sphere_center() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let sphere = Sphere {
            center: Vec3::new(5.0, 0.0, 0.0),
            radius: 1.0,
        };

        let intersection = ray.intersects_sphere(&sphere);
        assert!(intersection.is_some());

        let hit = intersection.unwrap();
        assert!((hit.distance - 4.0).abs() < 1e-5);
    }

    #[test]
    fn test_ray_intersects_sphere_miss() {
        let ray = Ray::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let sphere = Sphere {
            center: Vec3::new(5.0, 5.0, 0.0),
            radius: 1.0,
        };

        assert!(ray.intersects_sphere(&sphere).is_none());
    }

    #[test]
    fn test_ray_intersects_triangle() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.5, 1.0, 0.0);

        let ray = Ray::new(Vec3::new(0.5, 0.5, 1.0), Vec3::new(0.0, 0.0, -1.0));

        let hit = ray.intersects_triangle(a, b, c);
        assert!(hit.is_some());

        let point = hit.unwrap();
        assert!((point.z() - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_ray_intersects_triangle_miss() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.5, 1.0, 0.0);

        let ray = Ray::new(Vec3::new(2.0, 2.0, 1.0), Vec3::new(0.0, 0.0, -1.0));

        assert!(ray.intersects_triangle(a, b, c).is_none());
    }
}
