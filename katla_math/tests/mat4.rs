use approx::assert_relative_eq;
use katla_math::{Mat4, PI, Quat, Transform, Vec3, Vec4};

#[test]
fn test_mul_associative() {
    let m1 = Mat4([
        Vec4::new(1.0, 2.0, 3.0, 4.0),
        Vec4::new(5.0, 6.0, 7.0, 8.0),
        Vec4::new(9.0, 10.0, 11.0, 12.0),
        Vec4::new(13.0, 14.0, 15.0, 16.0),
    ]);

    let m2 = Mat4([
        Vec4::new(17.0, 18.0, 19.0, 20.0),
        Vec4::new(21.0, 22.0, 23.0, 24.0),
        Vec4::new(25.0, 26.0, 27.0, 28.0),
        Vec4::new(29.0, 30.0, 31.0, 32.0),
    ]);

    let m3 = Mat4([
        Vec4::new(33.0, 34.0, 35.0, 36.0),
        Vec4::new(37.0, 38.0, 39.0, 40.0),
        Vec4::new(41.0, 42.0, 43.0, 44.0),
        Vec4::new(45.0, 46.0, 47.0, 48.0),
    ]);

    let left = m1.mul(&m2).mul(&m3);
    let right = m1.mul(&m2.mul(&m3));

    for i in 0..4 {
        for j in 0..4 {
            assert_relative_eq!(left[i][j], right[i][j], epsilon = 1e-4);
        }
    }
}

#[test]
fn test_inverse_identity() {
    let identity = Mat4::default();
    let inv = identity.inverse();
    assert_eq!(identity, inv);
}

#[test]
fn test_inverse_translation() {
    let m = Mat4::from_translation([5.0, 10.0, 15.0]);
    let inv = m.inverse();
    let result = m.mul(&inv);
    let identity = Mat4::default();

    assert_eq!(identity, result);
}

#[test]
fn test_inverse_rotation() {
    let angle = std::f32::consts::PI / 4.0;
    let m = Mat4::from_rotaxis(&angle, [0.0, 1.0, 0.0]);
    let inv = m.inverse();
    let result = m.mul(&inv);
    let identity = Mat4::default();
    assert_eq!(identity, result);
}

#[test]
fn test_from_rotaxis_x() {
    let angle = std::f32::consts::PI / 2.0;
    let m = Mat4::from_rotaxis(&angle, [1.0, 0.0, 0.0]);

    assert_relative_eq!(m[0][0], 1.0, epsilon = 1e-4);
    assert_relative_eq!(m[0][1], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[0][2], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][0], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][1], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][2], 1.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][0], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][1], -1.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][2], 0.0, epsilon = 1e-4);
}

#[test]
fn test_from_rotaxis_y() {
    let angle = std::f32::consts::PI / 2.0;
    let m = Mat4::from_rotaxis(&angle, [0.0, 1.0, 0.0]);

    assert_relative_eq!(m[0][0], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[0][1], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[0][2], -1.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][0], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][1], 1.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][2], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][0], 1.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][1], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][2], 0.0, epsilon = 1e-4);
}

#[test]
fn test_from_rotaxis_z() {
    let angle = std::f32::consts::PI / 2.0;
    let m = Mat4::from_rotaxis(&angle, [0.0, 0.0, 1.0]);

    assert_relative_eq!(m[0][0], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[0][1], 1.0, epsilon = 1e-4);
    assert_relative_eq!(m[0][2], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][0], -1.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][1], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][2], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][0], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][1], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[2][2], 1.0, epsilon = 1e-4);
}

#[test]
fn test_from_rotaxis_zero_angle() {
    let m = Mat4::from_rotaxis(&0.0, [1.0, 2.0, 3.0]);
    let identity = Mat4::default();
    assert_eq!(m, identity);
}

#[test]
fn test_create_lookat() {
    let eye = Vec3::new(0.0, 0.0, 5.0);
    let target = Vec3::new(0.0, 0.0, 0.0);
    let up = Vec3::new(0.0, 1.0, 0.0);

    let m = Mat4::create_lookat(eye, target, up);

    assert_relative_eq!(m[3][0], 0.0);
    assert_relative_eq!(m[3][1], 0.0);
    assert_relative_eq!(m[3][2], 5.0);
    assert_relative_eq!(m[3][3], 1.0);
}

#[test]
fn test_create_lookat_up_direction() {
    let eye = Vec3::new(0.0, 0.0, 5.0);
    let target = Vec3::new(0.0, 0.0, 0.0);
    let up = Vec3::new(0.0, 1.0, 0.0);

    let m = Mat4::create_lookat(eye, target, up);

    assert_relative_eq!(m[1][0], 0.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][1], 1.0, epsilon = 1e-4);
    assert_relative_eq!(m[1][2], 0.0, epsilon = 1e-4);
}

#[test]
fn test_decompose_identity() {
    let m = Mat4::identity();
    let transform = m.decompose();

    assert_relative_eq!(transform.position.x(), 0.0);
    assert_relative_eq!(transform.position.y(), 0.0);
    assert_relative_eq!(transform.position.z(), 0.0);

    assert_relative_eq!(transform.scale.x(), 1.0);
    assert_relative_eq!(transform.scale.y(), 1.0);
    assert_relative_eq!(transform.scale.z(), 1.0);

    // Identity quaternion
    let (x, y, z, w) = transform.rotation.xyzw();
    assert_relative_eq!(x, 0.0, epsilon = 1e-5);
    assert_relative_eq!(y, 0.0, epsilon = 1e-5);
    assert_relative_eq!(z, 0.0, epsilon = 1e-5);
    assert_relative_eq!(w, 1.0, epsilon = 1e-5);
}

#[test]
fn test_decompose_rotation() {
    // Test 90 degree rotation around Z axis
    let rotation = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), PI / 2.0);
    let m = Mat4::from_rotation(rotation);
    let transform = m.decompose();

    assert_relative_eq!(transform.position.x(), 0.0);
    assert_relative_eq!(transform.position.y(), 0.0);
    assert_relative_eq!(transform.position.z(), 0.0);

    assert_relative_eq!(transform.scale.x(), 1.0);
    assert_relative_eq!(transform.scale.y(), 1.0);
    assert_relative_eq!(transform.scale.z(), 1.0);

    // Check that we got a valid normalized quaternion
    assert!(transform.rotation.is_normalized());
}

#[test]
fn test_decompose_scale() {
    let m = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let transform = m.decompose();

    assert_relative_eq!(transform.scale.x(), 2.0);
    assert_relative_eq!(transform.scale.y(), 3.0);
    assert_relative_eq!(transform.scale.z(), 4.0);
}

#[test]
fn test_decompose_trs() {
    // Create a transform with translation, rotation, and scale
    let mut t = Transform::new();
    t.position = Vec3::new(5.0, 10.0, 15.0);
    t.rotation = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), PI / 4.0);
    t.scale = Vec3::new(2.0, 3.0, 4.0);

    // Convert to matrix
    let m = t.make_mat4();

    // Decompose back
    let decomposed = m.decompose();

    // Check translation
    assert_relative_eq!(decomposed.position.x(), 5.0, epsilon = 1e-4);
    assert_relative_eq!(decomposed.position.y(), 10.0, epsilon = 1e-4);
    assert_relative_eq!(decomposed.position.z(), 15.0, epsilon = 1e-4);

    // Check scale
    assert_relative_eq!(decomposed.scale.x(), 2.0, epsilon = 1e-4);
    assert_relative_eq!(decomposed.scale.y(), 3.0, epsilon = 1e-4);
    assert_relative_eq!(decomposed.scale.z(), 4.0, epsilon = 1e-4);

    // Note: rotation may not be perfectly normalized when decomposing from
    // matrices with non-uniform scale, which is expected behavior
}

#[test]
fn test_create_ortho() {
    let m = Mat4::create_ortho(-1.0, 1.0, -1.0, 1.0, 0.1, 100.0);

    assert_relative_eq!(m[0][0], 1.0);
    assert_relative_eq!(m[1][1], 1.0);
    assert_relative_eq!(m[2][2], -0.02002002, epsilon = 1e-6);
    assert_relative_eq!(m[3][3], 1.0);
}

#[test]
fn test_create_proj() {
    let m = Mat4::create_proj(90.0, 1.0, 0.1);

    assert_relative_eq!(m[2][3], -1.0);
    assert_relative_eq!(m[3][3], 0.0);
    assert!(m[2][2] == 0.0);
    assert!(m[3][2] > 0.0);
}

#[test]
#[should_panic(expected = "INDEXING OUT_OF_BOUNDS")]
fn test_index_out_of_bounds() {
    let m = Mat4::default();
    let _ = m[4];
}
