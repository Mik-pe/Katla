use approx::assert_abs_diff_eq;
use std::f32::consts::FRAC_PI_2;

use katla_math::{mat4_mul_vec3, Quat, Vec3};

#[test]
fn test_quat_identity() {
    let quat = Quat::new();
    let vec = Vec3::new(1.0, 1.0, 1.0);
    let rotated_vec = quat * vec;
    assert_eq!(vec[0], rotated_vec[0]);
    assert_eq!(vec[1], rotated_vec[1]);
    assert_eq!(vec[2], rotated_vec[2]);
}

#[test]
fn test_quat_rotate() {
    let x_axis = Vec3::new(1.0, 0.0, 0.0);
    let quat = Quat::from_axis_angle(x_axis, FRAC_PI_2);
    let vec = Vec3::new(1.0, 1.0, 0.0);
    let rotated_vec = quat * vec;
    assert_abs_diff_eq!(rotated_vec[0], 1.0, epsilon = 0.0001);
    assert_abs_diff_eq!(rotated_vec[1], 0.0, epsilon = 0.0001);
    assert_abs_diff_eq!(rotated_vec[2], 1.0, epsilon = 0.0001);
    let rotated_vec = quat * rotated_vec;
    assert_abs_diff_eq!(rotated_vec[0], 1.0, epsilon = 0.0001);
    assert_abs_diff_eq!(rotated_vec[1], -1.0, epsilon = 0.0001);
    assert_abs_diff_eq!(rotated_vec[2], 0.0, epsilon = 0.0001);
    let rotated_vec = quat * rotated_vec;
    assert_abs_diff_eq!(rotated_vec[0], 1.0, epsilon = 0.0001);
    assert_abs_diff_eq!(rotated_vec[1], 0.0, epsilon = 0.0001);
    assert_abs_diff_eq!(rotated_vec[2], -1.0, epsilon = 0.0001);
    let rotated_vec = quat * rotated_vec;
    assert_abs_diff_eq!(rotated_vec[0], vec[0], epsilon = 0.0001);
    assert_abs_diff_eq!(rotated_vec[1], vec[1], epsilon = 0.0001);
    assert_abs_diff_eq!(rotated_vec[2], vec[2], epsilon = 0.0001);
}

#[test]
fn test_quat_inverse() {
    let quat = Quat::from_axis_angle(Vec3::new(2.25, 6.21, 1.22), 0.25);
    let vec = Vec3::new(1.0, 2.0, 3.0);
    let rotated_vec = quat * vec;
    let inv_quat = quat.inverse();
    let unrotated_vec = inv_quat * rotated_vec;
    assert_abs_diff_eq!(vec[0], unrotated_vec[0], epsilon = 0.0001);
    assert_abs_diff_eq!(vec[1], unrotated_vec[1], epsilon = 0.0001);
    assert_abs_diff_eq!(vec[2], unrotated_vec[2], epsilon = 0.0001);
}

#[test]
fn test_quat_mat() {
    let axis = Vec3::new(1.0, 1.0, 1.0);
    let quat = Quat::from_axis_angle(axis, FRAC_PI_2);
    let mat = quat.make_mat4();
    let vec = Vec3::new(1.0, 2.0, 3.0);
    let mat_rotated = mat4_mul_vec3(&mat, &vec);
    let quat_rotated = quat * vec;
    assert_abs_diff_eq!(mat_rotated[0], quat_rotated[0], epsilon = 0.0001);
    assert_abs_diff_eq!(mat_rotated[1], quat_rotated[1], epsilon = 0.0001);
    assert_abs_diff_eq!(mat_rotated[2], quat_rotated[2], epsilon = 0.0001);
}

#[test]
fn test_quat_mul() {
    let q1 = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 90.0);
    let q2 = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 90.0);
    let q3 = q1 * q2;
    assert!(q3.is_normalized());
}

#[test]
fn test_quat_mul_vec3() {
    let q = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 90.0);
    let v = Vec3::new(1.0, 0.0, 0.0);
    let v_rotated = q * v;
    assert!(v_rotated.is_normalized());
}

#[test]
fn test_quat_mul_vec3_identity() {
    let q = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 90.0);
    let v = Vec3::new(1.0, 0.0, 0.0);
    let v_rotated = q * v;
    assert!(v_rotated.is_normalized());
}

#[test]
fn test_quat_mul_vec3_zero() {
    let q = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 90.0);
    let v = Vec3::new(0.0, 0.0, 0.0);
    let v_rotated = q * v;
    assert!(v_rotated.is_zero());
}

#[test]
fn test_quat_mul_vec3_axes() {
    let q = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 90.0);
    let v = Vec3::new(1.0, 0.0, 0.0);
    let v_rotated = q * v;
    assert!(v_rotated.is_normalized());
    assert!(
        v_rotated.x().abs() > 0.999 && v_rotated.y().abs() < 0.001 && v_rotated.z().abs() < 0.001
    );
}
