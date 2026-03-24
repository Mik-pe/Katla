use katla_math::{Mat3, Mat4, Quat, Vec3};

#[test]
fn test_mat3_identity() {
    let m = Mat3::identity();
    assert_eq!(m[0], Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(m[1], Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(m[2], Vec3::new(0.0, 0.0, 1.0));
}

#[test]
fn test_mat3_default() {
    let m = Mat3::default();
    assert_eq!(m, Mat3::identity());
}

#[test]
fn test_mat3_from_scale() {
    let scale = Vec3::new(2.0, 3.0, 4.0);
    let m = Mat3::from_scale(scale);

    assert_eq!(m[0], Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(m[1], Vec3::new(0.0, 3.0, 0.0));
    assert_eq!(m[2], Vec3::new(0.0, 0.0, 4.0));
}

#[test]
fn test_mat3_from_rotation() {
    let q = Quat::from_axis_angle(Vec3::Z_AXIS, std::f32::consts::FRAC_PI_2);
    let m = Mat3::from_rotation(q);

    // 90-degree rotation around Z should map (1,0,0) to (0,1,0)
    let v = Vec3::new(1.0, 0.0, 0.0);
    let result = m * v;
    assert!((result.x() - 0.0).abs() < 1e-5);
    assert!((result.y() - 1.0).abs() < 1e-5);
    assert!((result.z() - 0.0).abs() < 1e-5);
}

#[test]
fn test_mat3_from_euler_angles() {
    let m = Mat3::from_euler_angles(0.0, 0.0, std::f32::consts::FRAC_PI_2);

    // 90-degree rotation around Z (roll only)
    let v = Vec3::new(1.0, 0.0, 0.0);
    let result = m * v;
    assert!((result.x() - 0.0).abs() < 1e-5);
    assert!((result.y() - 1.0).abs() < 1e-5);
}

#[test]
fn test_mat3_from_elements() {
    let m = Mat3::from_elements(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);

    assert_eq!(m[0], Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(m[1], Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(m[2], Vec3::new(7.0, 8.0, 9.0));
}

#[test]
fn test_mat3_mul() {
    let m1 = Mat3::from_scale(Vec3::new(2.0, 2.0, 2.0));
    let m2 = Mat3::from_scale(Vec3::new(3.0, 3.0, 3.0));
    let result = m1 * m2;

    assert_eq!(result[0], Vec3::new(6.0, 0.0, 0.0));
    assert_eq!(result[1], Vec3::new(0.0, 6.0, 0.0));
    assert_eq!(result[2], Vec3::new(0.0, 0.0, 6.0));
}

#[test]
fn test_mat3_mul_identity() {
    let m = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let result = m * Mat3::identity();
    assert_eq!(result, m);
}

#[test]
fn test_mat3_transpose() {
    let m = Mat3::from_elements(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);

    let t = m.transpose();

    assert_eq!(t[0], Vec3::new(1.0, 4.0, 7.0));
    assert_eq!(t[1], Vec3::new(2.0, 5.0, 8.0));
    assert_eq!(t[2], Vec3::new(3.0, 6.0, 9.0));
}

#[test]
fn test_mat3_transpose_twice() {
    let m = Mat3::from_elements(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);

    assert_eq!(m.transpose().transpose(), m);
}

#[test]
fn test_mat3_determinant_identity() {
    let m = Mat3::identity();
    assert_eq!(m.determinant(), 1.0);
}

#[test]
fn test_mat3_determinant_scale() {
    let m = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
    assert_eq!(m.determinant(), 2.0 * 3.0 * 4.0);
}

#[test]
fn test_mat3_determinant_singular() {
    let m = Mat3::from_elements(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);

    // This matrix has linearly dependent rows (det should be 0)
    let det = m.determinant();
    assert!(det.abs() < 1e-5);
}

#[test]
fn test_mat3_inverse_identity() {
    let m = Mat3::identity();
    let inv = m.inverse().unwrap();
    assert_eq!(inv, m);
}

#[test]
fn test_mat3_inverse_scale() {
    let m = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let inv = m.inverse().unwrap();

    assert_eq!(inv[0], Vec3::new(0.5, 0.0, 0.0));
    assert_eq!(inv[1], Vec3::new(0.0, 1.0 / 3.0, 0.0));
    assert_eq!(inv[2], Vec3::new(0.0, 0.0, 0.25));
}

#[test]
fn test_mat3_inverse_singular() {
    let m = Mat3::from_elements(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);

    assert!(m.inverse().is_none());
}

#[test]
fn test_mat3_inverse_then_mul_is_identity() {
    let m = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let inv = m.inverse().unwrap();
    let result = m * inv;

    assert!((result[0].x() - 1.0).abs() < 1e-5);
    assert!((result[1].y() - 1.0).abs() < 1e-5);
    assert!((result[2].z() - 1.0).abs() < 1e-5);
}

#[test]
fn test_mat3_mul_vec3() {
    let m = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let v = Vec3::new(1.0, 1.0, 1.0);
    let result = m * v;

    assert_eq!(result, Vec3::new(2.0, 3.0, 4.0));
}

#[test]
fn test_mat3_indexing() {
    let m = Mat3::from_elements(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);

    assert_eq!(m[0], Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(m[1], Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(m[2], Vec3::new(7.0, 8.0, 9.0));
}

#[test]
fn test_mat3_from_quat() {
    let q = Quat::from_axis_angle(Vec3::Y_AXIS, std::f32::consts::FRAC_PI_2);
    let m1 = Mat3::from(q);
    let m2 = Mat3::from_rotation(q);

    assert_eq!(m1, m2);
}

#[test]
fn test_mat3_to_mat4() {
    let m3 = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let m4 = m3.to_mat4();

    // Check that the 3x3 portion is preserved
    assert_eq!(m4[0].x(), 2.0);
    assert_eq!(m4[0].y(), 0.0);
    assert_eq!(m4[0].z(), 0.0);
    assert_eq!(m4[0].w(), 0.0);

    assert_eq!(m4[1].x(), 0.0);
    assert_eq!(m4[1].y(), 3.0);
    assert_eq!(m4[1].z(), 0.0);
    assert_eq!(m4[1].w(), 0.0);

    assert_eq!(m4[2].x(), 0.0);
    assert_eq!(m4[2].y(), 0.0);
    assert_eq!(m4[2].z(), 4.0);
    assert_eq!(m4[2].w(), 0.0);

    // Check bottom row is (0, 0, 0, 1)
    assert_eq!(m4[3].x(), 0.0);
    assert_eq!(m4[3].y(), 0.0);
    assert_eq!(m4[3].z(), 0.0);
    assert_eq!(m4[3].w(), 1.0);
}

#[test]
fn test_mat4_to_mat3() {
    let m4 = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let m3 = m4.to_mat3();

    assert_eq!(m3[0], Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(m3[1], Vec3::new(0.0, 3.0, 0.0));
    assert_eq!(m3[2], Vec3::new(0.0, 0.0, 4.0));
}

#[test]
fn test_mat3_partial_eq() {
    let m1 = Mat3::identity();
    let m2 = Mat3::identity();
    let m3 = Mat3::from_scale(Vec3::new(2.0, 1.0, 1.0));

    assert_eq!(m1, m2);
    assert_ne!(m1, m3);
}

#[test]
fn test_mat3_copy() {
    let m1 = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let m2 = m1;
    assert_eq!(m1, m2);
}

#[test]
fn test_mat3_clone() {
    let m1 = Mat3::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let m2 = m1.clone();
    assert_eq!(m1, m2);
}

#[test]
fn test_mat3_rotation_determinant_is_one() {
    let q = Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 1.5);
    let m = Mat3::from_rotation(q);
    let det = m.determinant();
    assert!((det - 1.0).abs() < 1e-5);
}
