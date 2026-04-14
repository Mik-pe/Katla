use crate::{Mat4, Sphere, Vec3, compute_bounds};

#[derive(Clone, Copy, Debug)]
pub struct AABB {
    pub center: Vec3,
    pub extent: Vec3,
}

impl AABB {
    #[inline]
    pub fn min(&self) -> Vec3 {
        self.center - self.extent
    }

    #[inline]
    pub fn max(&self) -> Vec3 {
        self.center + self.extent
    }

    #[inline]
    pub fn from_min_max(min: Vec3, max: Vec3) -> Self {
        let extent = (max - min) * 0.5;
        Self {
            center: min + extent,
            extent,
        }
    }

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

    #[inline]
    pub fn intersects_sphere(&self, sphere: &Sphere) -> bool {
        let closest = self.closest_point(sphere.center);
        let d = sphere.center - closest;
        d.dot(d) <= sphere.radius * sphere.radius
    }

    pub fn create_from_verts(verts: &[Vec3]) -> Self {
        let (min, max) = compute_bounds(verts);
        Self::from_min_max(min, max)
    }

    #[inline]
    pub fn merge(&self, other: &AABB) -> AABB {
        let min = Vec3::new(
            self.min()[0].min(other.min()[0]),
            self.min()[1].min(other.min()[1]),
            self.min()[2].min(other.min()[2]),
        );
        let max = Vec3::new(
            self.max()[0].max(other.max()[0]),
            self.max()[1].max(other.max()[1]),
            self.max()[2].max(other.max()[2]),
        );
        AABB::from_min_max(min, max)
    }

    pub fn transform(&self, mat: &Mat4) -> AABB {
        let mn = self.min();
        let mx = self.max();
        let corners = [
            Vec3::new(mn[0], mn[1], mn[2]),
            Vec3::new(mx[0], mn[1], mn[2]),
            Vec3::new(mn[0], mx[1], mn[2]),
            Vec3::new(mx[0], mx[1], mn[2]),
            Vec3::new(mn[0], mn[1], mx[2]),
            Vec3::new(mx[0], mn[1], mx[2]),
            Vec3::new(mn[0], mx[1], mx[2]),
            Vec3::new(mx[0], mx[1], mx[2]),
        ];

        let mut out_min = *mat * corners[0];
        let mut out_max = out_min;

        for corner in &corners[1..] {
            let transformed = *mat * *corner;
            for i in 0..3 {
                if transformed[i] < out_min[i] {
                    out_min[i] = transformed[i];
                }
                if transformed[i] > out_max[i] {
                    out_max[i] = transformed[i];
                }
            }
        }

        AABB::from_min_max(out_min, out_max)
    }

    #[inline]
    pub fn closest_point(&self, point: Vec3) -> Vec3 {
        let mn = self.min();
        let mx = self.max();
        Vec3::new(
            point[0].clamp(mn[0], mx[0]),
            point[1].clamp(mn[1], mx[1]),
            point[2].clamp(mn[2], mx[2]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aabb_merge() {
        let a = AABB::from_min_max(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0));
        let b = AABB::from_min_max(Vec3::new(2.0, 2.0, 2.0), Vec3::new(3.0, 3.0, 3.0));
        let merged = a.merge(&b);

        assert_eq!(merged.min(), Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(merged.max(), Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(merged.center, Vec3::new(1.5, 1.5, 1.5));
        assert_eq!(merged.extent, Vec3::new(1.5, 1.5, 1.5));
    }

    #[test]
    fn test_aabb_transform_translation() {
        let aabb = AABB::from_min_max(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0));
        let translation = Mat4::from_translation([5.0, 10.0, 15.0]);
        let transformed = aabb.transform(&translation);

        assert_eq!(transformed.min(), Vec3::new(5.0, 10.0, 15.0));
        assert_eq!(transformed.max(), Vec3::new(7.0, 12.0, 17.0));
        assert_eq!(transformed.extent, aabb.extent);
    }

    #[test]
    fn test_aabb_transform_rotation() {
        let aabb = AABB::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let rotation = Mat4::from_rotaxis(&std::f32::consts::FRAC_PI_4, [0.0, 0.0, 1.0]);
        let transformed = aabb.transform(&rotation);

        let expected_extent = std::f32::consts::SQRT_2;
        assert!(
            (transformed.extent[0] - expected_extent).abs() < 0.001,
            "expected x extent ~{}, got {}",
            expected_extent,
            transformed.extent[0]
        );
        assert!(
            (transformed.extent[1] - expected_extent).abs() < 0.001,
            "expected y extent ~{}, got {}",
            expected_extent,
            transformed.extent[1]
        );
        assert!(
            (transformed.extent[2] - 1.0).abs() < 0.001,
            "expected z extent 1.0, got {}",
            transformed.extent[2]
        );
    }

    #[test]
    fn test_aabb_closest_point_inside() {
        let aabb = AABB::from_min_max(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0));
        let point = Vec3::new(1.0, 1.0, 1.0);
        let closest = aabb.closest_point(point);

        assert_eq!(closest, point);
    }

    #[test]
    fn test_aabb_closest_point_outside() {
        let aabb = AABB::from_min_max(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0));
        let point = Vec3::new(3.0, -1.0, 1.0);
        let closest = aabb.closest_point(point);

        assert_eq!(closest, Vec3::new(2.0, 0.0, 1.0));
    }

    #[test]
    fn test_aabb_closest_point_on_face() {
        let aabb = AABB::from_min_max(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0));
        let point = Vec3::new(1.0, 2.0, 1.0);
        let closest = aabb.closest_point(point);

        assert_eq!(closest, point);
    }

    #[test]
    fn test_aabb_from_min_max() {
        let aabb = AABB::from_min_max(Vec3::new(1.0, 2.0, 3.0), Vec3::new(5.0, 6.0, 7.0));
        assert_eq!(aabb.center, Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(aabb.extent, Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(aabb.min(), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(aabb.max(), Vec3::new(5.0, 6.0, 7.0));
    }

    #[test]
    fn test_aabb_transform_identity() {
        let aabb = AABB::from_min_max(Vec3::new(-1.0, 2.0, -3.0), Vec3::new(4.0, 5.0, 6.0));
        let identity = Mat4::identity();
        let transformed = aabb.transform(&identity);

        assert!((transformed.min()[0] - aabb.min()[0]).abs() < 0.001,);
        assert!((transformed.min()[1] - aabb.min()[1]).abs() < 0.001,);
        assert!((transformed.min()[2] - aabb.min()[2]).abs() < 0.001,);
        assert!((transformed.max()[0] - aabb.max()[0]).abs() < 0.001,);
        assert!((transformed.max()[1] - aabb.max()[1]).abs() < 0.001,);
        assert!((transformed.max()[2] - aabb.max()[2]).abs() < 0.001,);
        assert!((transformed.extent[0] - aabb.extent[0]).abs() < 0.001);
        assert!((transformed.extent[1] - aabb.extent[1]).abs() < 0.001);
        assert!((transformed.extent[2] - aabb.extent[2]).abs() < 0.001);
    }

    #[test]
    fn test_aabb_transform_scale() {
        let aabb = AABB::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let scale = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
        let transformed = aabb.transform(&scale);

        // Original extent is (1,1,1), half-extent is (1,1,1). Scale by (2,3,4).
        // Corners at (-1,-1,-1) and (1,1,1) -> (-2,-3,-4) and (2,3,4).
        // New extent = (2,3,4).
        assert!(
            (transformed.extent[0] - 2.0).abs() < 0.001,
            "expected x extent 2.0, got {}",
            transformed.extent[0]
        );
        assert!(
            (transformed.extent[1] - 3.0).abs() < 0.001,
            "expected y extent 3.0, got {}",
            transformed.extent[1]
        );
        assert!(
            (transformed.extent[2] - 4.0).abs() < 0.001,
            "expected z extent 4.0, got {}",
            transformed.extent[2]
        );
    }

    #[test]
    fn test_aabb_transform_uniform_scale() {
        let aabb = AABB::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let scale = Mat4::from_scale(Vec3::new(2.0, 2.0, 2.0));
        let transformed = aabb.transform(&scale);

        // Original extent is (1,1,1), uniform scale by 2 -> extent doubles to (2,2,2).
        assert!(
            (transformed.extent[0] - 2.0).abs() < 0.001,
            "expected x extent 2.0, got {}",
            transformed.extent[0]
        );
        assert!(
            (transformed.extent[1] - 2.0).abs() < 0.001,
            "expected y extent 2.0, got {}",
            transformed.extent[1]
        );
        assert!(
            (transformed.extent[2] - 2.0).abs() < 0.001,
            "expected z extent 2.0, got {}",
            transformed.extent[2]
        );
        // Center stays at origin for centered AABB with pure scale.
        assert!((transformed.center[0]).abs() < 0.001);
        assert!((transformed.center[1]).abs() < 0.001);
        assert!((transformed.center[2]).abs() < 0.001);
    }

    #[test]
    fn test_aabb_transform_90_degree_rotation() {
        // AABB from (0,0,0) to (2,1,1)
        let aabb = AABB::from_min_max(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.0));
        let angle = std::f32::consts::FRAC_PI_2;
        let rotation = Mat4::from_rotaxis(&angle, [0.0, 0.0, 1.0]);
        let transformed = aabb.transform(&rotation);

        // 90-degree rotation around Z: (x,y) -> (-y, x)
        // Corner (0,0,0) -> (0,0,0)
        // Corner (2,0,0) -> (0,2,0)
        // Corner (0,1,0) -> (-1,0,0)
        // Corner (2,1,0) -> (-1,2,0)
        // Z stays at 0..1
        // Min = (-1, 0, 0), Max = (0, 2, 1)
        assert!(
            (transformed.min()[0] - (-1.0)).abs() < 0.001,
            "expected min.x = -1.0, got {}",
            transformed.min()[0]
        );
        assert!(
            (transformed.min()[1] - 0.0).abs() < 0.001,
            "expected min.y = 0.0, got {}",
            transformed.min()[1]
        );
        assert!(
            (transformed.min()[2] - 0.0).abs() < 0.001,
            "expected min.z = 0.0, got {}",
            transformed.min()[2]
        );
        assert!(
            (transformed.max()[0] - 0.0).abs() < 0.001,
            "expected max.x = 0.0, got {}",
            transformed.max()[0]
        );
        assert!(
            (transformed.max()[1] - 2.0).abs() < 0.001,
            "expected max.y = 2.0, got {}",
            transformed.max()[1]
        );
        assert!(
            (transformed.max()[2] - 1.0).abs() < 0.001,
            "expected max.z = 1.0, got {}",
            transformed.max()[2]
        );
    }

    #[test]
    fn test_aabb_transform_translation_preserves_extent() {
        let aabb = AABB::from_min_max(Vec3::new(-3.0, 1.0, -5.0), Vec3::new(7.0, 4.0, 2.0));
        let translation = Mat4::from_translation([100.0, -200.0, 300.0]);
        let transformed = aabb.transform(&translation);

        assert!((transformed.extent[0] - aabb.extent[0]).abs() < 0.001,);
        assert!((transformed.extent[1] - aabb.extent[1]).abs() < 0.001,);
        assert!((transformed.extent[2] - aabb.extent[2]).abs() < 0.001,);
    }

    #[test]
    fn test_aabb_transform_combined_trs() {
        // Unit cube at origin: (-0.5,-0.5,-0.5) to (0.5,0.5,0.5)
        let aabb = AABB::from_min_max(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5));

        // Scale by 2
        let scale = Mat4::from_scale(Vec3::new(2.0, 2.0, 2.0));
        // Rotate 45 degrees around Y
        let angle = std::f32::consts::FRAC_PI_4;
        let rotation = Mat4::from_rotaxis(&angle, [0.0, 1.0, 0.0]);
        // Translate by (10,0,0)
        let translation = Mat4::from_translation([10.0, 0.0, 0.0]);

        // Combined: T * R * S
        let combined = translation * rotation * scale;
        let transformed = aabb.transform(&combined);

        // After scale by 2, cube is (-1,-1,-1) to (1,1,1).
        // After 45-degree Y rotation, the xz-extent grows by sqrt(2).
        // After translation, center moves to (10,0,0).
        assert!(
            (transformed.center[0] - 10.0).abs() < 0.01,
            "expected center.x ~10.0, got {}",
            transformed.center[0]
        );
        assert!(
            (transformed.center[1] - 0.0).abs() < 0.01,
            "expected center.y ~0.0, got {}",
            transformed.center[1]
        );
        assert!(
            (transformed.center[2] - 0.0).abs() < 0.01,
            "expected center.z ~0.0, got {}",
            transformed.center[2]
        );

        // Original extent was (0.5, 0.5, 0.5). After scale + rotation, extent should be larger.
        assert!(
            transformed.extent[0] > aabb.extent[0],
            "x extent should be larger after scale+rotation"
        );
        assert!(
            transformed.extent[2] > aabb.extent[2],
            "z extent should be larger after scale+rotation"
        );
        // Y extent should be exactly 1.0 (scale 2 * 0.5, no Y rotation effect).
        assert!(
            (transformed.extent[1] - 1.0).abs() < 0.01,
            "expected y extent ~1.0, got {}",
            transformed.extent[1]
        );
    }

    #[test]
    fn test_aabb_transform_negative_scale() {
        let aabb = AABB::from_min_max(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0));
        let scale = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        let transformed = aabb.transform(&scale);

        // Negative X scale flips the box: corners (0,0,0)->(0,0,0) and (1,1,1)->(-1,1,1).
        // Result: min = (-1, 0, 0), max = (0, 1, 1). Valid AABB: min < max.
        assert!(
            transformed.min()[0] < transformed.max()[0],
            "min.x ({}) should be < max.x ({})",
            transformed.min()[0],
            transformed.max()[0]
        );
        assert!(
            transformed.min()[1] < transformed.max()[1],
            "min.y ({}) should be < max.y ({})",
            transformed.min()[1],
            transformed.max()[1]
        );
        assert!(
            transformed.min()[2] < transformed.max()[2],
            "min.z ({}) should be < max.z ({})",
            transformed.min()[2],
            transformed.max()[2]
        );
    }

    #[test]
    fn test_aabb_transform_zero_size_aabb() {
        // Point AABB at (3, 4, 5)
        let aabb = AABB::from_min_max(Vec3::new(3.0, 4.0, 5.0), Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(aabb.extent, Vec3::new(0.0, 0.0, 0.0));

        let translation = Mat4::from_translation([10.0, 20.0, 30.0]);
        let transformed = aabb.transform(&translation);

        // Point should be moved, extent stays zero.
        assert!(
            (transformed.min()[0] - 13.0).abs() < 0.001,
            "expected min.x = 13.0, got {}",
            transformed.min()[0]
        );
        assert!(
            (transformed.min()[1] - 24.0).abs() < 0.001,
            "expected min.y = 24.0, got {}",
            transformed.min()[1]
        );
        assert!(
            (transformed.min()[2] - 35.0).abs() < 0.001,
            "expected min.z = 35.0, got {}",
            transformed.min()[2]
        );
        assert!(
            (transformed.extent[0]).abs() < 0.001,
            "expected zero x extent, got {}",
            transformed.extent[0]
        );
        assert!(
            (transformed.extent[1]).abs() < 0.001,
            "expected zero y extent, got {}",
            transformed.extent[1]
        );
        assert!(
            (transformed.extent[2]).abs() < 0.001,
            "expected zero z extent, got {}",
            transformed.extent[2]
        );
    }
}
