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
        let proj = Mat4::create_proj_reverse_z(fov, aspect, near);
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
        let planes = [
            &self.left,
            &self.right,
            &self.top,
            &self.bottom,
            &self.near,
            &self.far,
        ];

        for plane in &planes {
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
        let planes = [
            &self.left,
            &self.right,
            &self.top,
            &self.bottom,
            &self.near,
            &self.far,
        ];

        for plane in &planes {
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
            let dist = plane.distance_to_point(sphere.center);
            if dist < -sphere.radius {
                return false;
            }
        }

        true
    }

    /// Get the 8 corner points of the frustum
    pub fn corners(&self) -> [Vec3; 8] {
        let origin = Vec3::new(0.0, 0.0, 0.0);
        // Near plane corners
        let ntl = Self::intersect_three_planes(&self.left, &self.top, &self.near).unwrap_or(origin);
        let ntr =
            Self::intersect_three_planes(&self.right, &self.top, &self.near).unwrap_or(origin);
        let nbl =
            Self::intersect_three_planes(&self.left, &self.bottom, &self.near).unwrap_or(origin);
        let nbr =
            Self::intersect_three_planes(&self.right, &self.bottom, &self.near).unwrap_or(origin);

        // Far plane corners
        let ftl = Self::intersect_three_planes(&self.left, &self.top, &self.far).unwrap_or(origin);
        let ftr = Self::intersect_three_planes(&self.right, &self.top, &self.far).unwrap_or(origin);
        let fbl =
            Self::intersect_three_planes(&self.left, &self.bottom, &self.far).unwrap_or(origin);
        let fbr =
            Self::intersect_three_planes(&self.right, &self.bottom, &self.far).unwrap_or(origin);

        [ntl, ntr, nbl, nbr, ftl, ftr, fbl, fbr]
    }

    /// Get the 8 corner points of the frustum using a finite far distance.
    ///
    /// With infinite reverse-Z projection, the far plane is at infinity, so
    /// `corners()` produces degenerate far corners. This method computes near
    /// corners normally and extends each edge direction to `far_distance` from
    /// the near plane along the near plane normal.
    #[inline]
    pub fn corners_with_far_distance(&self, far_distance: f32) -> [Vec3; 8] {
        let origin = Vec3::new(0.0, 0.0, 0.0);
        let ntl = Self::intersect_three_planes(&self.left, &self.top, &self.near).unwrap_or(origin);
        let ntr =
            Self::intersect_three_planes(&self.right, &self.top, &self.near).unwrap_or(origin);
        let nbl =
            Self::intersect_three_planes(&self.left, &self.bottom, &self.near).unwrap_or(origin);
        let nbr =
            Self::intersect_three_planes(&self.right, &self.bottom, &self.near).unwrap_or(origin);

        let near_corners = [ntl, ntr, nbl, nbr];

        // Get existing (degenerate) far corners from the matrix's far plane.
        // The direction from near corner to existing far corner gives us the
        // frustum edge direction. We extend this direction proportionally.
        let existing = self.corners();

        // Compute the distance along the near plane normal from near to existing-far.
        let near_to_far_depth = (self.near.distance - self.far.distance).abs();
        if near_to_far_depth < 1e-6 {
            return [ntl, ntr, nbl, nbr, ntl, ntr, nbl, nbr];
        }

        // Scale factor: how much to extend from the near-to-existing-far distance
        // to reach the desired far_distance from the near plane.
        let scale = far_distance / near_to_far_depth;

        let mut result = [Vec3::new(0.0, 0.0, 0.0); 8];
        result[0] = ntl;
        result[1] = ntr;
        result[2] = nbl;
        result[3] = nbr;

        for i in 0..4 {
            let edge_dir = existing[i + 4] - near_corners[i];
            result[i + 4] = near_corners[i] + edge_dir * scale;
        }

        result
    }

    /// Find the intersection point of three planes
    fn intersect_three_planes(p1: &Plane, p2: &Plane, p3: &Plane) -> Option<Vec3> {
        let n1 = p1.normal;
        let n2 = p2.normal;
        let n3 = p3.normal;

        let d1 = p1.distance;
        let d2 = p2.distance;
        let d3 = p3.distance;

        let denom = n1.dot(n2.cross(n3));

        if denom.abs() < 1e-6 {
            return None;
        }

        let num = d1 * n2.cross(n3) + n1 * d2 * n3.cross(n1) + n1.cross(n2) * d3;
        Some(num / denom)
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

    /// Calculate the center point of the frustum using a finite far distance.
    #[inline]
    pub fn center_with_far(&self, far_distance: f32) -> Vec3 {
        let corners = self.corners_with_far_distance(far_distance);
        let mut sum = Vec3::new(0.0, 0.0, 0.0);
        for corner in &corners {
            sum += *corner;
        }
        sum / 8.0
    }

    /// Calculate a bounding sphere that encloses the frustum using a finite far distance.
    #[inline]
    pub fn bounding_sphere_with_far(&self, far_distance: f32) -> Sphere {
        let center = self.center_with_far(far_distance);
        let corners = self.corners_with_far_distance(far_distance);

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
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            16.0 / 9.0,
            0.1,
        );

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

        assert!(frustum.contains_point(Vec3::new(0.0, 0.0, 0.0)));
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

        let sphere = Sphere {
            center: Vec3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert!(frustum.intersects_sphere(&sphere));

        let far_sphere = Sphere {
            center: Vec3::new(100.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert!(!frustum.intersects_sphere(&far_sphere));
    }

    #[test]
    fn test_corners_with_far_distance() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        let corners = frustum.corners_with_far_distance(100.0);
        let camera = Vec3::new(0.0, 0.0, 5.0);

        // Near corners should all be at the same distance from camera
        let near_dist = (corners[0] - camera).length();
        for i in 1..4 {
            let dist = (corners[i] - camera).length();
            assert!(
                (dist - near_dist).abs() < 0.1,
                "Near corner {i} at distance {dist}, expected ~{near_dist}"
            );
        }

        // Far corners should all be at the same distance from camera
        let far_dist = (corners[4] - camera).length();
        for i in 5..8 {
            let dist = (corners[i] - camera).length();
            assert!(
                (dist - far_dist).abs() < 1.0,
                "Far corner {i} at distance {dist}, expected ~{far_dist}"
            );
        }

        // Far depth along near plane normal should be approximately 100.0 from near plane
        let near_depth =
            frustum.near.distance_to_point(corners[0]) - frustum.near.distance_to_point(camera);
        let far_depth =
            frustum.near.distance_to_point(corners[4]) - frustum.near.distance_to_point(camera);
        let extension = (far_depth - near_depth).abs();
        assert!(
            (extension - 100.0).abs() < 10.0,
            "Far depth extension should be ~100.0 from near plane, got {extension}"
        );

        // All far corners should be farther from camera than near corners
        for i in 0..4 {
            let nd = (corners[i] - camera).length();
            let fd = (corners[i + 4] - camera).length();
            assert!(fd > nd, "Far corner {i} should be farther than near corner");
        }
    }

    #[test]
    fn test_corners_with_far_distance_symmetry() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        let corners = frustum.corners_with_far_distance(50.0);

        // NTL and NTR should have same Z, opposite X (symmetric about view axis)
        assert!(
            (corners[0].x() + corners[1].x()).abs() < 0.5,
            "Near TL.x + TR.x should be ~0"
        );
        assert!(
            (corners[0].z() - corners[1].z()).abs() < 0.1,
            "Near TL.z should equal TR.z"
        );

        // Same symmetry for far corners
        assert!(
            (corners[4].x() + corners[5].x()).abs() < 0.5,
            "Far TL.x + TR.x should be ~0"
        );
        assert!(
            (corners[4].z() - corners[5].z()).abs() < 0.1,
            "Far TL.z should equal TR.z"
        );

        // TL and BL should have same X, opposite Y
        assert!(
            (corners[0].x() - corners[2].x()).abs() < 0.1,
            "Near TL.x should equal BL.x"
        );
        assert!(
            (corners[0].y() + corners[2].y()).abs() < 0.5,
            "Near TL.y + BL.y should be ~0"
        );
    }

    #[test]
    fn test_center_with_far() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        let center = frustum.center_with_far(100.0);

        // Center should be on or near the view axis
        assert!(center.x().abs() < 1.0, "Center x should be ~0");
        assert!(center.y().abs() < 1.0, "Center y should be ~0");

        // Center z should be between near and far on the view axis
        let near_z = frustum.corners_with_far_distance(100.0)[0].z();
        let far_z = frustum.corners_with_far_distance(100.0)[4].z();
        let z_min = near_z.min(far_z);
        let z_max = near_z.max(far_z);
        assert!(
            center.z() >= z_min - 1.0 && center.z() <= z_max + 1.0,
            "Center z={} should be between near z={} and far z={}",
            center.z(),
            z_min,
            z_max
        );
    }

    #[test]
    fn test_bounding_sphere_with_far() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        let sphere = frustum.bounding_sphere_with_far(100.0);

        // The sphere should enclose all 8 corners
        let corners = frustum.corners_with_far_distance(100.0);
        for (i, corner) in corners.iter().enumerate() {
            let dist = (*corner - sphere.center).length();
            assert!(
                dist <= sphere.radius + 0.5,
                "Corner {i} at distance {dist} exceeds sphere radius {}",
                sphere.radius
            );
        }

        assert!(sphere.radius > 0.0);
    }

    #[test]
    fn test_corners_degenerate_without_far_override() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        let camera = Vec3::new(0.0, 0.0, 5.0);

        // corners() with infinite reverse-Z: far corners are closer to camera than near corners
        let corners_infinite = frustum.corners();
        let near_dist = (corners_infinite[0] - camera).length();
        let far_closer = corners_infinite[4..]
            .iter()
            .any(|c| (*c - camera).length() < near_dist);

        // corners_with_far_distance: far corners should be farther
        let corners_fixed = frustum.corners_with_far_distance(100.0);
        let far_valid = corners_fixed[4..]
            .iter()
            .all(|c| (*c - camera).length() > (corners_fixed[0] - camera).length());

        assert!(
            far_closer,
            "Far corners with infinite reverse-Z should be closer than near corners"
        );
        assert!(
            far_valid,
            "Far corners with far_distance should be farther than near corners"
        );
    }
}
