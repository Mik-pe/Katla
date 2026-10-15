use katla_math::{AABB, Sphere, Vec3};

#[test]
fn test_transformed_bounds_match_corners_with_shear_and_reflection() {
    use katla_math::{Mat4, Quat, Vec4};
    for extent in [Vec3::new(1.0, 2.0, 0.5), Vec3::new(0.0, 0.0, 0.0)] {
        let bounds = AABB {
            center: Vec3::new(-1.0, 3.0, 0.5),
            extent,
        };
        let shear = Mat4([
            Vec4::new(1.0, 0.3, -0.2, 0.0),
            Vec4::new(-0.4, 1.0, 0.7, 0.0),
            Vec4::new(0.5, -0.2, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ]);
        for matrix in [
            Mat4::identity(),
            shear,
            Mat4::from_trs(
                Vec3::new(2.0, -5.0, 4.0),
                Quat::from_euler(0.7, -0.4, 1.2),
                Vec3::new(-2.0, 0.5, 0.0),
            )
            .mul(&shear),
        ] {
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for corner in 0..8 {
                let point: [f32; 3] = std::array::from_fn(|axis| {
                    bounds.center[axis]
                        + if corner & (1 << axis) == 0 {
                            -extent[axis]
                        } else {
                            extent[axis]
                        }
                });
                for row in 0..3 {
                    let value = matrix[0][row] * point[0]
                        + matrix[1][row] * point[1]
                        + matrix[2][row] * point[2]
                        + matrix[3][row];
                    min[row] = min[row].min(value);
                    max[row] = max[row].max(value);
                }
            }
            let actual = bounds.transform(&matrix);
            for axis in 0..3 {
                approx::assert_abs_diff_eq!(actual.min()[axis], min[axis], epsilon = 1e-5);
                approx::assert_abs_diff_eq!(actual.max()[axis], max[axis], epsilon = 1e-5);
            }
        }
    }
}

#[test]
fn test_expand() {
    let verts = vec![
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, -10.0, 0.0),
        Vec3::new(0.0, 10.0, -10.0),
    ];
    let aabb = AABB::create_from_verts(&verts);

    assert_eq!(aabb.extent[0], 0.5);
    assert_eq!(aabb.extent[1], 10.0);
    assert_eq!(aabb.extent[2], 5.0);

    assert_eq!(aabb.center[0], 0.5);
    assert_eq!(aabb.center[1], 0.0);
    assert_eq!(aabb.center[2], -5.0);
}

#[test]
fn test_aabb_intersects() {
    let aabb1 = AABB {
        center: Vec3::new(0.0, 0.0, 0.0),
        extent: Vec3::new(1.0, 1.0, 1.0),
    };

    let aabb2 = AABB {
        center: Vec3::new(0.5, 0.5, 0.5),
        extent: Vec3::new(1.0, 1.0, 1.0),
    };

    assert!(aabb1.intersects(&aabb2));
}

#[test]
fn test_aabb_does_not_intersect() {
    let aabb1 = AABB {
        center: Vec3::new(0.0, 0.0, 0.0),
        extent: Vec3::new(1.0, 1.0, 1.0),
    };

    let aabb2 = AABB {
        center: Vec3::new(-2.0, -2.0, -2.0),
        extent: Vec3::new(0.9, 0.9, 0.9),
    };

    assert!(!aabb1.intersects(&aabb2));
}

#[test]
fn test_aabb_create_from_verts() {
    let verts = vec![
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
    ];

    let aabb = AABB::create_from_verts(&verts);

    assert_eq!(aabb.center, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(aabb.extent, Vec3::new(1.0, 1.0, 1.0));
}

#[test]
fn test_aabb_intersects_sphere_inside() {
    let aabb = AABB {
        center: Vec3::new(0.0, 0.0, 0.0),
        extent: Vec3::new(1.0, 1.0, 1.0),
    };
    let sphere = Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.5);

    assert!(aabb.intersects_sphere(&sphere));
}

#[test]
fn test_aabb_intersects_sphere_overlapping() {
    let aabb = AABB {
        center: Vec3::new(0.0, 0.0, 0.0),
        extent: Vec3::new(1.0, 1.0, 1.0),
    };
    let sphere = Sphere::new(Vec3::new(2.0, 0.0, 0.0), 1.5);

    assert!(aabb.intersects_sphere(&sphere));
}

#[test]
fn test_aabb_does_not_intersect_sphere() {
    let aabb = AABB {
        center: Vec3::new(0.0, 0.0, 0.0),
        extent: Vec3::new(1.0, 1.0, 1.0),
    };
    let sphere = Sphere::new(Vec3::new(5.0, 5.0, 5.0), 1.0);

    assert!(!aabb.intersects_sphere(&sphere));
}

#[test]
fn test_aabb_intersects_sphere_touching() {
    let aabb = AABB {
        center: Vec3::new(0.0, 0.0, 0.0),
        extent: Vec3::new(1.0, 1.0, 1.0),
    };
    let sphere = Sphere::new(Vec3::new(3.0, 0.0, 0.0), 2.0);

    assert!(aabb.intersects_sphere(&sphere));
}
