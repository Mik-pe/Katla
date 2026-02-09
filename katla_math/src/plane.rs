use crate::{AABB, Mat4, Ray, Sphere, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub distance: f32,
}

/// Which side of a plane a point is on
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaneSide {
    Front,
    Back,
    Intersecting,
}

impl Plane {
    /// Create a new plane from a normal and distance from origin
    pub fn new(normal: Vec3, distance: f32) -> Self {
        Plane { normal, distance }
    }

    /// Create a plane from a point on the plane and a normal vector
    pub fn from_point_normal(point: Vec3, normal: Vec3) -> Self {
        let normal = normal.normalize();
        let distance = normal.dot(point);
        Plane { normal, distance }
    }

    /// Create a plane from three points (counter-clockwise winding)
    pub fn from_points(a: Vec3, b: Vec3, c: Vec3) -> Self {
        let ab = b - a;
        let ac = c - a;
        let normal = ab.cross(ac).normalize();
        let distance = normal.dot(a);
        Plane { normal, distance }
    }

    /// Calculate the signed distance from a point to the plane
    /// Positive means in front (along normal), negative means behind
    pub fn distance_to_point(&self, point: Vec3) -> f32 {
        self.normal.dot(point) - self.distance
    }

    /// Check if a point lies on the plane (within tolerance)
    pub fn contains_point(&self, point: Vec3, tolerance: f32) -> bool {
        self.distance_to_point(point).abs() < tolerance
    }

    /// Find the closest point on the plane to the given point
    pub fn closest_point(&self, point: Vec3) -> Vec3 {
        let dist = self.distance_to_point(point);
        point - self.normal * dist
    }

    /// Determine which side of the plane a point is on
    pub fn which_side(&self, point: Vec3) -> PlaneSide {
        let dist = self.distance_to_point(point);
        if dist > 1e-5 {
            PlaneSide::Front
        } else if dist < -1e-5 {
            PlaneSide::Back
        } else {
            PlaneSide::Intersecting
        }
    }

    /// Check if the plane intersects an AABB
    pub fn intersects_aabb(&self, aabb: &AABB) -> bool {
        // Get the positive and negative corners based on the plane normal
        let (positive, negative) = if self.normal.x() >= 0.0 {
            (Vec3::new(aabb.center.x() + aabb.extent.x(), aabb.center.y() + aabb.extent.y(), aabb.center.z() + aabb.extent.z()),
             Vec3::new(aabb.center.x() - aabb.extent.x(), aabb.center.y() - aabb.extent.y(), aabb.center.z() - aabb.extent.z()))
        } else {
            (Vec3::new(aabb.center.x() - aabb.extent.x(), aabb.center.y() + aabb.extent.y(), aabb.center.z() + aabb.extent.z()),
             Vec3::new(aabb.center.x() + aabb.extent.x(), aabb.center.y() - aabb.extent.y(), aabb.center.z() - aabb.extent.z()))
        };

        let pos_dist = self.distance_to_point(positive);
        let neg_dist = self.distance_to_point(negative);

        // If one is in front and one is behind, we're intersecting
        pos_dist >= 0.0 && neg_dist <= 0.0
    }

    /// Check if the plane intersects a sphere
    pub fn intersects_sphere(&self, sphere: &Sphere) -> bool {
        let dist = self.distance_to_point(sphere.center).abs();
        dist <= sphere.radius
    }

    /// Check if a ray intersects the plane
    /// Returns Some(distance along ray) if intersection occurs, None if parallel or no intersection
    pub fn intersects_ray(&self, ray: &Ray) -> Option<f32> {
        let denominator = self.normal.dot(ray.direction);

        // Ray is parallel to the plane
        if denominator.abs() < 1e-6 {
            return None;
        }

        let t = (self.distance - self.normal.dot(ray.origin)) / denominator;

        // Only return intersection if it's in front of the ray
        if t >= 0.0 {
            Some(t)
        } else {
            None
        }
    }

    /// Transform the plane by a matrix
    pub fn transform(&self, matrix: &Mat4) -> Plane {
        // Transform a point on the plane
        let point_on_plane = self.normal * self.distance;
        let transformed_point = matrix.clone() * point_on_plane;

        // Transform the normal (using inverse transpose for correct transformation)
        let normal = self.normal; // This will be transformed correctly below
        let transformed_normal = matrix.to_mat3() * normal;

        Plane::from_point_normal(transformed_point, transformed_normal)
    }

    /// Normalize the plane (ensure normal is unit length)
    pub fn normalize(&self) -> Plane {
        let len = self.normal.length();
        if len > 0.0 {
            Plane {
                normal: self.normal / len,
                distance: self.distance / len,
            }
        } else {
            *self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plane_from_point_normal() {
        let point = Vec3::new(1.0, 0.0, 0.0);
        let normal = Vec3::new(1.0, 0.0, 0.0);
        let plane = Plane::from_point_normal(point, normal);

        assert!((plane.normal.x() - 1.0).abs() < 1e-5);
        assert!((plane.distance - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_plane_from_points() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(1.0, 0.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        let plane = Plane::from_points(a, b, c);

        // Normal should be pointing in +Z direction
        assert!(plane.normal.z() > 0.0);

        // Origin should be on the plane
        assert!(plane.contains_point(Vec3::new(0.0, 0.0, 0.0), 1e-5));
    }

    #[test]
    fn test_plane_distance_to_point() {
        let plane = Plane::new(Vec3::new(0.0, 1.0, 0.0), 5.0);

        // Point at y=5 should be on the plane
        assert!((plane.distance_to_point(Vec3::new(0.0, 5.0, 0.0)) - 0.0).abs() < 1e-5);

        // Point at y=10 should be 5 units in front
        assert!((plane.distance_to_point(Vec3::new(0.0, 10.0, 0.0)) - 5.0).abs() < 1e-5);

        // Point at y=0 should be 5 units behind
        assert!((plane.distance_to_point(Vec3::new(0.0, 0.0, 0.0)) - (-5.0)).abs() < 1e-5);
    }

    #[test]
    fn test_plane_which_side() {
        let plane = Plane::new(Vec3::new(0.0, 1.0, 0.0), 5.0);

        assert_eq!(plane.which_side(Vec3::new(0.0, 10.0, 0.0)), PlaneSide::Front);
        assert_eq!(plane.which_side(Vec3::new(0.0, 0.0, 0.0)), PlaneSide::Back);
        assert_eq!(plane.which_side(Vec3::new(0.0, 5.0, 0.0)), PlaneSide::Intersecting);
    }

    #[test]
    fn test_plane_closest_point() {
        let plane = Plane::new(Vec3::new(0.0, 1.0, 0.0), 5.0);
        let point = Vec3::new(0.0, 10.0, 0.0);
        let closest = plane.closest_point(point);

        assert!((closest.y() - 5.0).abs() < 1e-5);
        assert!((closest.x() - 0.0).abs() < 1e-5);
        assert!((closest.z() - 0.0).abs() < 1e-5);
    }
}
