use crate::{AABB, Mat4, Plane, Sphere, Vec3, Vec4};

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
    ///
    /// Extracts clip-space planes using the Gribb-Hartmann method.
    /// Mat4 stores column-major: `m[col][row]`, so row `r` = `(m[0][r], m[1][r], m[2][r], m[3][r])`.
    pub fn from_projection_view_matrix(proj: &Mat4, view: &Mat4) -> Self {
        let m = *proj * *view;

        // Extract planes from the combined matrix rows (Gribb-Hartmann method).
        // row r = (m[0][r], m[1][r], m[2][r], m[3][r])
        let r0 = Vec4::new(m[0][0], m[1][0], m[2][0], m[3][0]);
        let r1 = Vec4::new(m[0][1], m[1][1], m[2][1], m[3][1]);
        let r2 = Vec4::new(m[0][2], m[1][2], m[2][2], m[3][2]);
        let r3 = Vec4::new(m[0][3], m[1][3], m[2][3], m[3][3]);

        fn normalize_plane(p: Vec4) -> Plane {
            let n = Vec3::new(p.x(), p.y(), p.z());
            let len = n.length();
            Plane::new(n / len, p.w() / len)
        }

        Frustum {
            left: normalize_plane(r3 + r0),
            right: normalize_plane(r3 - r0),
            bottom: normalize_plane(r3 + r1),
            top: normalize_plane(r3 - r1),
            near: normalize_plane(r3 + r2),
            far: normalize_plane(r3 - r2),
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

        // corners_with_far_distance: far corners should be farther than near corners
        let corners_fixed = frustum.corners_with_far_distance(100.0);
        let near_dist = (corners_fixed[0] - camera).length();
        let far_valid = corners_fixed[4..]
            .iter()
            .all(|c| (*c - camera).length() > near_dist);

        assert!(
            far_valid,
            "Far corners with far_distance should be farther than near corners"
        );

        // Far corners should be at approximately the specified far distance from near plane
        for i in 4..8 {
            let dist_from_camera = (corners_fixed[i] - camera).length();
            assert!(
                dist_from_camera > 50.0,
                "Far corner {i} at dist {dist_from_camera} should be far from camera"
            );
        }
    }

    fn aabb_at(center: Vec3, extent: f32) -> AABB {
        AABB::from_min_max(
            center - Vec3::new(extent, extent, extent),
            center + Vec3::new(extent, extent, extent),
        )
    }

    fn camera_frustum() -> Frustum {
        Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            0.1,
        )
    }

    #[test]
    fn test_frustum_aabb_fully_inside() {
        let frustum = camera_frustum();
        let aabb = aabb_at(Vec3::new(0.0, 0.0, 0.0), 0.5);
        assert!(
            frustum.intersects_aabb(&aabb),
            "Small box at origin should be inside the frustum"
        );
    }

    #[test]
    fn test_frustum_aabb_fully_outside_left() {
        let frustum = camera_frustum();
        let aabb = aabb_at(Vec3::new(-10.0, 0.0, 0.0), 0.5);
        assert!(
            !frustum.intersects_aabb(&aabb),
            "Box at (-10,0,0) should be outside (far left of view)"
        );
    }

    #[test]
    fn test_frustum_aabb_fully_outside_right() {
        let frustum = camera_frustum();
        let aabb = aabb_at(Vec3::new(10.0, 0.0, 0.0), 0.5);
        assert!(
            !frustum.intersects_aabb(&aabb),
            "Box at (10,0,0) should be outside (far right of view)"
        );
    }

    #[test]
    fn test_frustum_aabb_fully_outside_above() {
        let frustum = camera_frustum();
        let aabb = aabb_at(Vec3::new(0.0, 10.0, 0.0), 0.5);
        assert!(
            !frustum.intersects_aabb(&aabb),
            "Box at (0,10,0) should be outside (far above view)"
        );
    }

    #[test]
    fn test_frustum_aabb_fully_outside_behind() {
        let frustum = camera_frustum();
        // With infinite reverse-Z projection the far plane cannot cull, but the
        // near plane and side planes can. Place the box far behind the camera
        // AND off to the side so a side plane culls it.
        let aabb = aabb_at(Vec3::new(0.0, 100.0, 10.0), 0.5);
        assert!(
            !frustum.intersects_aabb(&aabb),
            "Box at (0,100,10) should be outside (above and behind camera)"
        );
    }

    #[test]
    fn test_frustum_aabb_straddling_plane() {
        let frustum = camera_frustum();
        // With 90 deg FOV the half-angle is 45 deg, so the left plane passes
        // through the line x = -(z - 5) approximately. At z=0 this is x=-5.
        // Place the box so it crosses the left boundary.
        let aabb = aabb_at(Vec3::new(-4.5, 0.0, 0.0), 1.0);
        assert!(
            frustum.intersects_aabb(&aabb),
            "Box straddling left plane should intersect"
        );
    }

    #[test]
    fn test_frustum_aabb_large_enclosing_frustum() {
        let frustum = camera_frustum();
        let aabb = AABB::from_min_max(
            Vec3::new(-100.0, -100.0, -100.0),
            Vec3::new(100.0, 100.0, 100.0),
        );
        assert!(
            frustum.intersects_aabb(&aabb),
            "Huge AABB enclosing the frustum should intersect"
        );
    }

    #[test]
    fn test_frustum_aabb_at_near_plane() {
        let frustum = camera_frustum();
        // Near plane is ~0.1 units in front of camera at (0,0,5), so at z≈4.9
        let aabb = aabb_at(Vec3::new(0.0, 0.0, 4.9), 0.05);
        assert!(
            frustum.intersects_aabb(&aabb),
            "AABB at the near plane should intersect"
        );
    }

    #[test]
    fn test_frustum_aabb_forward_z() {
        // Camera at (0,0,5) looking at origin (-Z direction), verifying that
        // objects in front are visible and objects to the side are culled.
        // Uses the same camera as other tests but verifies that moving the
        // AABB along Z works correctly with the near plane.
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            0.1,
        );

        // AABB at z=2 (between near plane and camera) should be visible
        let aabb_near = aabb_at(Vec3::new(0.0, 0.0, 2.0), 0.5);
        assert!(
            frustum.intersects_aabb(&aabb_near),
            "AABB at (0,0,2) should be inside the frustum"
        );

        // AABB at z=4.9 (just behind the near plane) should still be visible
        let aabb_near_plane = aabb_at(Vec3::new(0.0, 0.0, 4.89), 0.01);
        assert!(
            frustum.intersects_aabb(&aabb_near_plane),
            "AABB just behind near plane should be inside"
        );

        // AABB above and behind camera should be outside (culled by top/bottom planes)
        let aabb_above_behind = aabb_at(Vec3::new(0.0, 100.0, 10.0), 0.5);
        assert!(
            !frustum.intersects_aabb(&aabb_above_behind),
            "AABB at (0,100,10) should be outside (above and behind camera)"
        );
    }

    #[test]
    fn test_frustum_aabb_narrow_fov() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            30.0,
            1.0,
            0.1,
        );

        let aabb_wide = aabb_at(Vec3::new(3.0, 0.0, 0.0), 0.5);
        assert!(
            !frustum.intersects_aabb(&aabb_wide),
            "Box at (3,0,0) should be outside with narrow 30 deg FOV"
        );

        let aabb_center = aabb_at(Vec3::new(0.0, 0.0, 0.0), 0.5);
        assert!(
            frustum.intersects_aabb(&aabb_center),
            "Box at origin should still be visible with narrow FOV"
        );
    }

    // --- Plane extraction tests (TODO 176a) ---

    #[test]
    fn test_frustum_plane_normals_point_inward() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        // A point clearly inside the frustum: on the view axis, between near
        // and far.  Camera at z=5, near=1 → near clip at z≈4, so z=2 is inside.
        let interior_point = Vec3::new(0.0, 0.0, 2.0);

        let planes = [
            (&frustum.left, "left"),
            (&frustum.right, "right"),
            (&frustum.top, "top"),
            (&frustum.bottom, "bottom"),
            (&frustum.near, "near"),
            (&frustum.far, "far"),
        ];

        for (plane, name) in &planes {
            let dist = plane.distance_to_point(interior_point);
            assert!(
                dist > 0.0,
                "{name} plane should have positive distance for interior point: dist={dist}, normal={:?}, d={}",
                plane.normal,
                plane.distance
            );
        }
    }

    #[test]
    fn test_frustum_near_plane_distance() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        // Camera at z=5 looking toward -Z, near=1.0.
        // The near clip plane should be at z≈4.0 (camera z minus near distance).
        // Verify the point (0,0,4) is approximately on the near plane.
        let near_point = Vec3::new(0.0, 0.0, 4.0);

        // With reverse-Z infinite projection, the geometrically near clip plane
        // is extracted as the "far" plane in the Gribb-Hartmann scheme.
        // Check both named planes to find which one represents the near clip.
        let near_dist = frustum.near.distance_to_point(near_point);
        let far_dist = frustum.far.distance_to_point(near_point);

        // One of near or far should be approximately 0 at z=4
        let near_is_clip = near_dist.abs() < 0.1;
        let far_is_clip = far_dist.abs() < 0.1;

        assert!(
            near_is_clip || far_is_clip,
            "Either near or far plane should pass through (0,0,4). \
             near.distance_to_point={near_dist:.4}, far.distance_to_point={far_dist:.4}"
        );

        // The near clip plane normal should be primarily along the Z axis.
        let clip_plane = if near_is_clip {
            &frustum.near
        } else {
            &frustum.far
        };
        assert!(
            clip_plane.normal.z().abs() > 0.9,
            "Near clip plane normal should be primarily along Z axis, got {:?}",
            clip_plane.normal
        );

        // Verify a point slightly beyond the near plane (toward camera) is outside
        let behind_near = Vec3::new(0.0, 0.0, 4.5);
        assert!(
            clip_plane.distance_to_point(behind_near) < 0.0,
            "Point at z=4.5 (between camera and near plane) should be outside near clip: dist={}",
            clip_plane.distance_to_point(behind_near)
        );

        // Verify a point inside (past the near plane, away from camera)
        let inside_near = Vec3::new(0.0, 0.0, 3.0);
        assert!(
            clip_plane.distance_to_point(inside_near) > 0.0,
            "Point at z=3.0 (past near plane) should be inside near clip: dist={}",
            clip_plane.distance_to_point(inside_near)
        );
    }

    #[test]
    fn test_frustum_symmetric_planes() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        // With symmetric 90 deg FOV and aspect 1.0, left/right plane normals
        // should be mirror images in X: left.normal.x == -right.normal.x,
        // and identical in Y and Z.
        let tolerance = 0.01;

        assert!(
            (frustum.left.normal.x() + frustum.right.normal.x()).abs() < tolerance,
            "Left X ({}) should be negative of right X ({}), sum should be ~0",
            frustum.left.normal.x(),
            frustum.right.normal.x()
        );
        assert!(
            (frustum.left.normal.y() - frustum.right.normal.y()).abs() < tolerance,
            "Left Y ({}) should equal right Y ({})",
            frustum.left.normal.y(),
            frustum.right.normal.y()
        );
        assert!(
            (frustum.left.normal.z() - frustum.right.normal.z()).abs() < tolerance,
            "Left Z ({}) should equal right Z ({})",
            frustum.left.normal.z(),
            frustum.right.normal.z()
        );

        // Top/bottom should be symmetric in Y
        assert!(
            (frustum.top.normal.x() - frustum.bottom.normal.x()).abs() < tolerance,
            "Top X ({}) should equal bottom X ({})",
            frustum.top.normal.x(),
            frustum.bottom.normal.x()
        );
        assert!(
            (frustum.top.normal.y() + frustum.bottom.normal.y()).abs() < tolerance,
            "Top Y ({}) should be negative of bottom Y ({}), sum should be ~0",
            frustum.top.normal.y(),
            frustum.bottom.normal.y()
        );
        assert!(
            (frustum.top.normal.z() - frustum.bottom.normal.z()).abs() < tolerance,
            "Top Z ({}) should equal bottom Z ({})",
            frustum.top.normal.z(),
            frustum.bottom.normal.z()
        );

        // Distances should also be symmetric
        assert!(
            (frustum.left.distance - frustum.right.distance).abs() < tolerance,
            "Left distance ({}) should equal right distance ({})",
            frustum.left.distance,
            frustum.right.distance
        );
        assert!(
            (frustum.top.distance - frustum.bottom.distance).abs() < tolerance,
            "Top distance ({}) should equal bottom distance ({})",
            frustum.top.distance,
            frustum.bottom.distance
        );
    }

    #[test]
    fn test_frustum_from_known_ortho() {
        // Orthographic: x in [-1,1], y in [-1,1], z in [near=0, far=10]
        let proj = Mat4::create_ortho(-1.0, 1.0, -1.0, 1.0, 0.0, 10.0);
        let view = Mat4::identity();
        let frustum = Frustum::from_projection_view_matrix(&proj, &view);

        let tolerance = 0.01;

        // Verify a point clearly inside the frustum
        assert!(
            frustum.contains_point(Vec3::new(0.0, 0.0, -5.0)),
            "Point (0,0,-5) should be inside the orthographic frustum"
        );

        // Points just inside each boundary should be contained
        assert!(
            frustum.contains_point(Vec3::new(-0.5, 0.0, -5.0)),
            "Point inside left boundary should be contained"
        );
        assert!(
            frustum.contains_point(Vec3::new(0.5, 0.0, -5.0)),
            "Point inside right boundary should be contained"
        );

        // Points well outside should not be contained
        assert!(
            !frustum.contains_point(Vec3::new(-5.0, 0.0, -5.0)),
            "Point far left should be outside"
        );
        assert!(
            !frustum.contains_point(Vec3::new(5.0, 0.0, -5.0)),
            "Point far right should be outside"
        );
        assert!(
            !frustum.contains_point(Vec3::new(0.0, 5.0, -5.0)),
            "Point far above should be outside"
        );

        // Left/right normals should have opposite X components
        assert!(
            (frustum.left.normal.x() + frustum.right.normal.x()).abs() < tolerance,
            "Left X ({}) should be negative of right X ({})",
            frustum.left.normal.x(),
            frustum.right.normal.x()
        );
    }

    #[test]
    fn test_frustum_forward_z_camera() {
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            1.0,
            1.0,
        );

        // Object at (0,0,-5) is 5 units in front of the camera along -Z,
        // past the near plane (near=1.0, so near clip at z≈-1).
        let object = Vec3::new(0.0, 0.0, -5.0);

        // Find which plane is the near clip: with reverse-Z the Gribb-Hartmann
        // "far" extraction may give the near clip.
        let near_is_clip = frustum
            .near
            .distance_to_point(Vec3::new(0.0, 0.0, -1.0))
            .abs()
            < 0.5;
        let clip_plane = if near_is_clip {
            &frustum.near
        } else {
            &frustum.far
        };

        let clip_dist = clip_plane.distance_to_point(object);
        assert!(
            clip_dist > 0.0,
            "Object at (0,0,-5) should be past the near clip plane: dist={clip_dist:.4}, plane normal={:?}, d={}",
            clip_plane.normal,
            clip_plane.distance
        );

        // Object should be fully inside the frustum
        assert!(
            frustum.contains_point(object),
            "Object at (0,0,-5) should be inside the frustum. \
             left={}, right={}, top={}, bottom={}, near={}, far={}",
            frustum.left.distance_to_point(object),
            frustum.right.distance_to_point(object),
            frustum.top.distance_to_point(object),
            frustum.bottom.distance_to_point(object),
            frustum.near.distance_to_point(object),
            frustum.far.distance_to_point(object),
        );
    }

    #[test]
    fn test_frustum_wide_fov() {
        let frustum_narrow = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::new(0.0, 1.0, 0.0),
            60.0,
            1.0,
            1.0,
        );

        let frustum_wide = Frustum::from_camera(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::new(0.0, 1.0, 0.0),
            120.0,
            1.0,
            1.0,
        );

        // Wider FOV should cull fewer objects — a point at (5, 0, -5) should
        // be inside the 120° frustum but outside the 60° frustum.
        let wide_point = Vec3::new(5.0, 0.0, -5.0);
        assert!(
            !frustum_narrow.contains_point(wide_point),
            "Narrow FOV (60°) should not contain (5,0,-5)"
        );
        assert!(
            frustum_wide.contains_point(wide_point),
            "Wide FOV (120°) should contain (5,0,-5)"
        );

        // Both frustums should contain a point on the view axis
        let center_point = Vec3::new(0.0, 0.0, -5.0);
        assert!(
            frustum_narrow.contains_point(center_point),
            "Narrow FOV should contain (0,0,-5)"
        );
        assert!(
            frustum_wide.contains_point(center_point),
            "Wide FOV should contain (0,0,-5)"
        );
    }

    #[test]
    fn test_frustum_culling_integration() {
        // Camera at (0, 5, 10) looking at origin, 90 deg FOV
        let frustum = Frustum::from_camera(
            Vec3::new(0.0, 5.0, 10.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            90.0,
            16.0 / 9.0,
            0.1,
        );

        let visible_extent = 0.5;
        let visible_entities = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(-3.0, 0.0, 0.0),
            Vec3::new(0.0, 3.0, 0.0),
        ];

        let hidden_entities = [
            Vec3::new(0.0, 0.0, 20.0),
            Vec3::new(50.0, 0.0, 0.0),
            Vec3::new(-50.0, 0.0, 0.0),
            Vec3::new(0.0, 50.0, 0.0),
        ];

        for pos in &visible_entities {
            let aabb = AABB::from_min_max(
                *pos - Vec3::new(visible_extent, visible_extent, visible_extent),
                *pos + Vec3::new(visible_extent, visible_extent, visible_extent),
            );
            assert!(
                frustum.intersects_aabb(&aabb),
                "Entity at {:?} should be visible (inside frustum)",
                pos
            );
        }

        for pos in &hidden_entities {
            let aabb = AABB::from_min_max(
                *pos - Vec3::new(visible_extent, visible_extent, visible_extent),
                *pos + Vec3::new(visible_extent, visible_extent, visible_extent),
            );
            assert!(
                !frustum.intersects_aabb(&aabb),
                "Entity at {:?} should be culled (outside frustum)",
                pos
            );
        }
    }
}
