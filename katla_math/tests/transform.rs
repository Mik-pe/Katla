use std::f32::consts::{FRAC_PI_2, PI};

use approx::assert_abs_diff_eq;
use katla_math::{mat4_mul_vec3, Mat4, Quat, Transform, Vec3, Vec4};

#[test]
fn test_scale_mat() {
    let scale_vec = Vec3::new(1.0, 0.0, 2.0);
    let transform = Transform::new_from_scale(scale_vec);
    let vertex = Vec3::new(1.0, 1.0, 1.0);
    let transform_mat = transform.make_mat4();
    let transformed_vertex = mat4_mul_vec3(&transform_mat, &vertex);
    println!("Matrix: {:?}", transform_mat);
    println!("Vertex: {:?}", transformed_vertex);
    assert_abs_diff_eq!(transformed_vertex[0], scale_vec[0], epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], scale_vec[1], epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], scale_vec[2], epsilon = 0.0001);
}

#[test]
fn test_rotation_mat() {
    let rotation = Quat::new_from_axis_angle(Vec3::new(1.0, 0.0, 0.0), PI);
    let transform = Transform::new_from_rotation(rotation);
    let vertex = Vec3::new(1.0, 1.0, 1.0);
    let transform_mat = transform.make_mat4();
    let transformed_vertex = mat4_mul_vec3(&transform_mat, &vertex);
    println!("Matrix: {:?}", transform_mat);
    println!("Vertex: {:?}", transformed_vertex);
    assert_abs_diff_eq!(transformed_vertex[0], 1.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], -1.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], -1.0, epsilon = 0.0001);
}

#[test]
fn test_position_mat() {
    let position = Vec3::new(2.0, 1.0, -3.0);
    let transform = Transform::new_from_position(position);
    let vertex = Vec3::new(0.0, 0.0, 0.0);
    let transform_mat = transform.make_mat4();
    let transformed_vertex = mat4_mul_vec3(&transform_mat, &vertex);
    println!("Matrix: {:?}", transform_mat);
    println!("Vertex: {:?}", transformed_vertex);
    assert_abs_diff_eq!(transformed_vertex[0], position[0], epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], position[1], epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], position[2], epsilon = 0.0001);
}

#[test]
fn test_transform_mat() {
    let position = Vec3::new(1.0, 0.0, 0.0);
    let scale = Vec3::new(2.0, 2.0, 2.0);
    let rotation = Quat::new_from_axis_angle(Vec3::new(1.0, 0.0, 0.0), FRAC_PI_2);
    let transform = Transform {
        position,
        scale,
        rotation,
    };
    let vertex = Vec3::new(1.0, 0.0, 1.0);
    let transform_mat = transform.make_mat4();
    let transformed_vertex = mat4_mul_vec3(&transform_mat, &vertex);
    println!("Matrix: {:?}", transform_mat);
    println!("Vertex: {:?}", transformed_vertex);
    assert_abs_diff_eq!(transformed_vertex[0], 3.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], -2.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], 0.0, epsilon = 0.0001);
}

#[test]
fn test_transform() {
    let position = Vec3::new(1.0, 0.0, 0.0);
    let scale = Vec3::new(2.0, 2.0, 2.0);
    let rotation = Quat::new_from_axis_angle(Vec3::new(1.0, 0.0, 0.0), FRAC_PI_2);
    let transform = Transform {
        position,
        scale,
        rotation,
    };
    let vertex = Vec3::new(1.0, 0.0, 1.0);
    let transformed_vertex = transform * vertex;
    println!("Vertex: {:?}", transformed_vertex);
    assert_abs_diff_eq!(transformed_vertex[0], 3.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], -2.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], 0.0, epsilon = 0.0001);
}
