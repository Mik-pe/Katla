use crate::{AABB, Mat4, Plane, Sphere, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    pub left: Plane,
    pub right: Plane,
    pub top: Plane,
    pub bottom: Plane,
    pub near: Plane,
    pub far: Plane,
}

impl Frustum {
    /// Create a new frustum from 6 planes
    pub fn new(
        left: Plane,
        right: Plane,
        top: Plane,
        bottom: Plane,
        near: Plane,
        far: Plane,
    ) -> Self {
        Frustum {
            left,
            right,
            top,
            bottom,
            near,
            far,
        }
    }

    /// Create a frustum from a projection and view matrix
    pub fn from_projection_view_matrix(proj: &Mat4, view: &Mat4) -> Self {
        let combined = *proj * *view;

        // Extract planes from the combined matrix
        // When normalizing the plane normal, we must also scale the distance
        // Left plane: row3 + row0
        let left_normal = Vec3::new(
            combined[3][0] + combined[0][0],
            combined[3][1] + combined[0][1],
            combined[3][2] + combined[0][2],
        );
        let left_len = left_normal.length();
        let left = Plane::new(
            left_normal.normalize(),
            (combined[3][3] + combined[0][3]) / left_len,
        );

        // Right plane: row3 - row0
        let right_normal = Vec3::new(
            combined[3][0] - combined[0][0],
            combined[3][1] - combined[0][1],
            combined[3][2] - combined[0][2],
        );
        let right_len = right_normal.length();
        let right = Plane::new(
            right_normal.normalize(),
            (combined[3][3] - combined[0][3]) / right_len,
        );

        // Bottom plane: row3 + row1
        let bottom_normal = Vec3::new(
            combined[3][0] + combined[1][0],
            combined[3][1] + combined[1][1],
            combined[3][2] + combined[1][2],
        );
        let bottom_len = bottom_normal.length();
        let bottom = Plane::new(
            bottom_normal.normalize(),
            (combined[3][3] + combined[1][3]) / bottom_len,
        );

        // Top plane: row3 - row1
        let top_normal = Vec3::new(
            combined[3][0] - combined[1][0],
            combined[3][1] - combined[1][1],
            combined[3][2] - combined[1][2],
        );
        let top_len = top_normal.length();
        let top = Plane::new(
            top_normal.normalize(),
            (combined[3][3] - combined[1][3]) / top_len,
        );

        // Near plane: row3 + row2
        let near_normal = Vec3::new(
            combined[3][0] + combined[2][0],
            combined[3][1] + combined[2][1],
            combined[3][2] + combined[2][2],
        );
        let near_len = near_normal.length();
        let near = Plane::new(
            near_normal.normalize(),
            (combined[3][3] + combined[2][3]) / near_len,
        );

        // Far plane: row3 - row2
        let far_normal = Vec3::new(
            combined[3][0] - combined[2][0],
            combined[3][1] - combined[2][1],
            combined[3][2] - combined[2][2],
        );
        let far_len = far_normal.length();
        let far = Plane::new(
            far_normal.normalize(),
            (combined[3][3] - combined[2][3]) / far_len,
        );

        Frustum {
            left,
            right,
            top,
            bottom,
            near,
            far,
        }
    }

    /// Create a frustum from camera parameters
    pub fn from_camera(
        position: Vec3,
        target: Vec3,
        up: Vec3,
        fov: f32,
        aspect: f32,
        near: f32,
    ) -> Self {
        let view = Mat4::create_lookat(position, target, up);
        let proj = Mat4::create_proj(fov, aspect, near);
        Self::from_projection_view_matrix(&proj, &view)
    }

    /// Check if a point is inside the frustum
    pub fn contains_point(&self, point: Vec3) -> bool {
        // Point is inside if it's in front of all planes
        self.left.distance_to_point(point) >= 0.0
            && self.right.distance_to_point(point) >= 0.0
            && self.top.distance_to_point(point) >= 0.0
            && self.bottom.distance_to_point(point) >= 0.0
            && self.near.distance_to_point(point) >= 0.0
            && self.far.distance_to_point(point) >= 0.0
    }

    /// Check if an AABB intersects the frustum
    pub fn intersects_aabb(&self, aabb: &AABB) -> bool {
        // AABB is potentially visible if it's in front of at least one plane
        // or intersects with at least one plane
        let planes = [
            &self.left,
            &self.right,
            &self.top,
            &self.bottom,
            &self.near,
            &self.far,
        ];

        // If the AABB is completely behind any plane, it's outside
        for plane in &planes {
            // Get the positive corner (most in the direction of the plane normal)
            let positive = Vec3::new(
                if plane.normal.x() >= 0.0 {
                    aabb.center.x() + aabb.extent.x()
                } else {
                    aabb.center.x() - aabb.extent.x()
                },
                if plane.normal.y() >= 0.0 {
                    aabb.center.y() + aabb.extent.y()
                } else {
                    aabb.center.y() - aabb.extent.y()
                },
                if plane.normal.z() >= 0.0 {
                    aabb.center.z() + aabb.extent.z()
                } else {
                    aabb.center.z() - aabb.extent.z()
                },
            );

            if plane.distance_to_point(positive) < 0.0 {
                return false;
            }
        }

        true
    }

    /// Check if an AABB is fully contained within the frustum
    pub fn contains_aabb(&self, aabb: &AABB) -> bool {
        // AABB is fully inside if all corners are inside
        // Check the most extreme corner for each plane
        let planes = [
            &self.left,
            &self.right,
            &self.top,
            &self.bottom,
            &self.near,
            &self.far,
        ];

        for plane in &planes {
            // Get the negative corner (most opposite to the plane normal)
            let negative = Vec3::new(
                if plane.normal.x() >= 0.0 {
                    aabb.center.x() - aabb.extent.x()
                } else {
                    aabb.center.x() + aabb.extent.x()
                },
                if plane.normal.y() >= 0.0 {
                    aabb.center.y() - aabb.extent.y()
                } else {
                    aabb.center.y() + aabb.extent.y()
                },
                if plane.normal.z() >= 0.0 {
                    aabb.center.z() - aabb.extent.z()
                } else {
                    aabb.center.z() + aabb.extent.z()
                },
            );

            if plane.distance_to_point(negative) < 0.0 {
                return false;
            }
        }

        true
    }

    /// Check if a sphere intersects the frustum
    pub fn intersects_sphere(&self, sphere: &Sphere) -> bool {
        let planes = [
            &self.left,
            &self.right,
            &self.top,
            &self.bottom,
            &self.near,
            &self.far,
        ];

        for plane in &planes {
            // Distance from center to plane
            let dist = plane.distance_to_point(sphere.center);

            // If center is behind the plane by more than radius, outside
            if dist < -sphere.radius {
                return false;
            }
        }

        true
    }

    /// Get the 8 corner points of the frustum
    pub fn corners(&self) -> [Vec3; 8] {
        // Find intersections of the 3 plane triples
        // Near plane corners
        let ntl = Self::intersect_three_planes(&self.left, &self.top, &self.near);
        let ntr = Self::intersect_three_planes(&self.right, &self.top, &self.near);
        let nbl = Self::intersect_three_planes(&self.left, &self.bottom, &self.near);
        let nbr = Self::intersect_three_planes(&self.right, &self.bottom, &self.near);

        // Far plane corners
        let ftl = Self::intersect_three_planes(&self.left, &self.top, &self.far);
        let ftr = Self::intersect_three_planes(&self.right, &self.top, &self.far);
        let fbl = Self::intersect_three_planes(&self.left, &self.bottom, &self.far);
        let fbr = Self::intersect_three_planes(&self.right, &self.bottom, &self.far);

        [ntl, ntr, nbl, nbr, ftl, ftr, fbl, fbr]
    }

    /// Find the intersection point of three planes
    fn intersect_three_planes(p1: &Plane, p2: &Plane, p3: &Plane) -> Vec3 {
        // Using Cramer's rule to solve the system of 3 plane equations
        let n1 = p1.normal;
        let n2 = p2.normal;
        let n3 = p3.normal;

        let d1 = p1.distance;
        let d2 = p2.distance;
        let d3 = p3.distance;

        let denom = n1.dot(n2.cross(n3));

        if denom.abs() < 1e-6 {
            // Planes are parallel or intersect in a line
            return Vec3::new(0.0, 0.0, 0.0);
        }

        let num = d1 * n2.cross(n3) + n1 * d2 * n3.cross(n1) + n1.cross(n2) * d3;
        num / denom
    }

    /// Calculate the center point of the frustum
    pub fn center(&self) -> Vec3 {
        let corners = self.corners();
        let mut sum = Vec3::new(0.0, 0.0, 0.0);
        for corner in &corners {
            sum += *corner;
        }
        sum / 8.0
    }

    /// Calculate a bounding sphere that encloses the frustum
    pub fn bounding_sphere(&self) -> Sphere {
        let center = self.center();
        let corners = self.corners();

        // Find the farthest corner
        let mut max_dist = 0.0;
        for corner in &corners {
            let dist = (*corner - center).length();
            if dist > max_dist {
                max_dist = dist;
            }
        }

        Sphere {
            center,
            radius: max_dist,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frustum_from_camera() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0), // position
            Vec3::new(0.0, 0.0, 0.0), // target
            Vec3::new(0.0, 1.0, 0.0), // up
            90.0,                     // fov
            16.0 / 9.0,               // aspect
            0.1,                      // near
        );

        // Origin should be in front of near plane
        assert!(frustum.near.distance_to_point(Vec3::new(0.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn test_frustum_contains_point() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            0.1,
        );

        // Origin should be inside
        assert!(frustum.contains_point(Vec3::new(0.0, 0.0, 0.0)));

        // Point far outside should not be inside
        assert!(!frustum.contains_point(Vec3::new(100.0, 100.0, 100.0)));
    }

    #[test]
    fn test_frustum_intersects_sphere() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            0.1,
        );

        // Sphere at origin should intersect
        let sphere = Sphere {
            center: Vec3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert!(frustum.intersects_sphere(&sphere));

        // Sphere way off to the side should not intersect
        let far_sphere = Sphere {
            center: Vec3::new(100.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert!(!frustum.intersects_sphere(&far_sphere));
    }
}
