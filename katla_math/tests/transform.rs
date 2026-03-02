use std::f32::consts::{FRAC_PI_2, PI};

use approx::assert_abs_diff_eq;
use katla_math::{Quat, Transform, Vec3};

#[test]
fn test_scale_mat() {
    let scale_vec = Vec3::new(1.0, 0.0, 2.0);
    let transform = Transform::new_from_scale(scale_vec);
    let vertex = Vec3::new(1.0, 1.0, 1.0);
    let transform_mat = transform.make_mat4();
    println!("Matrix: {transform_mat:?}");
    let transformed_vertex = transform_mat * vertex;
    println!("Vertex: {transformed_vertex:?}");
    assert_abs_diff_eq!(transformed_vertex[0], scale_vec[0], epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], scale_vec[1], epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], scale_vec[2], epsilon = 0.0001);
}

#[test]
fn test_rotation_mat() {
    let rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), PI);
    let transform = Transform::new_from_rotation(rotation);
    let vertex = Vec3::new(1.0, 1.0, 1.0);
    let transform_mat = transform.make_mat4();
    println!("Matrix: {transform_mat:?}");
    let transformed_vertex = transform_mat * vertex;
    println!("Vertex: {transformed_vertex:?}");
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
    println!("Matrix: {transform_mat:?}");
    let transformed_vertex = transform_mat * vertex;
    println!("Vertex: {transformed_vertex:?}");
    assert_abs_diff_eq!(transformed_vertex[0], position[0], epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], position[1], epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], position[2], epsilon = 0.0001);
}

#[test]
fn test_transform_mat() {
    let position = Vec3::new(1.0, 0.0, 0.0);
    let scale = Vec3::new(2.0, 2.0, 2.0);
    let rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), FRAC_PI_2);
    let transform = Transform {
        position,
        scale,
        rotation,
    };
    let vertex = Vec3::new(1.0, 0.0, 1.0);
    let transform_mat = transform.make_mat4();
    println!("Matrix: {transform_mat:?}");
    let transformed_vertex = transform_mat * vertex;
    println!("Vertex: {transformed_vertex:?}");
    assert_abs_diff_eq!(transformed_vertex[0], 3.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], -2.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], 0.0, epsilon = 0.0001);
}

#[test]
fn test_transform() {
    let position = Vec3::new(1.0, 0.0, 0.0);
    let scale = Vec3::new(2.0, 2.0, 2.0);
    let rotation = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), FRAC_PI_2);
    let transform = Transform {
        position,
        scale,
        rotation,
    };
    let vertex = Vec3::new(1.0, 0.0, 1.0);
    let transformed_vertex = transform * vertex;
    println!("Vertex: {transformed_vertex:?}");
    assert_abs_diff_eq!(transformed_vertex[0], 3.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[1], -2.0, epsilon = 0.0001);
    assert_abs_diff_eq!(transformed_vertex[2], 0.0, epsilon = 0.0001);
}

#[test]
fn test_transform_inverse() {
    // Test inverse with a simpler transform (rotation + translation only)
    let transform = Transform {
        position: Vec3::new(5.0, 10.0, 15.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::from_euler(0.5, 0.3, 0.7),
    };

    let inverse = transform.inverse();

    // Verify that inverse has the expected properties
    // (inverted position and rotation)
    let test_point = Vec3::new(1.0, 2.0, 3.0);
    let transformed = transform * test_point;
    let restored = inverse * transformed;

    // The point should be restored to approximately the original
    assert!((restored[0] - test_point[0]).abs() < 1e-4);
    assert!((restored[1] - test_point[1]).abs() < 1e-4);
    assert!((restored[2] - test_point[2]).abs() < 1e-4);
}

#[test]
fn test_transform_forward() {
    // Transform with identity rotation should have forward pointing along -Z
    let transform = Transform::new();
    let forward = transform.forward();

    assert!((forward[0] - 0.0).abs() < 1e-5);
    assert!((forward[1] - 0.0).abs() < 1e-5);
    assert!((forward[2] - (-1.0)).abs() < 1e-5);
}

#[test]
fn test_transform_up() {
    // Transform with identity rotation should have up pointing along +Y
    let transform = Transform::new();
    let up = transform.up();

    assert!((up[0] - 0.0).abs() < 1e-5);
    assert!((up[1] - 1.0).abs() < 1e-5);
    assert!((up[2] - 0.0).abs() < 1e-5);
}

#[test]
fn test_transform_right() {
    // Transform with identity rotation should have right pointing along +X
    let transform = Transform::new();
    let right = transform.right();

    assert!((right[0] - 1.0).abs() < 1e-5);
    assert!((right[1] - 0.0).abs() < 1e-5);
    assert!((right[2] - 0.0).abs() < 1e-5);
}

#[test]
fn test_transform_lerp() {
    let t1 = Transform {
        position: Vec3::new(0.0, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::new(),
    };

    let t2 = Transform {
        position: Vec3::new(10.0, 20.0, 30.0),
        scale: Vec3::new(2.0, 2.0, 2.0),
        rotation: Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), std::f32::consts::FRAC_PI_2),
    };

    // t=0 should give t1
    let result = t1.lerp(&t2, 0.0);
    assert!((result.position[0] - t1.position[0]).abs() < 1e-5);
    assert!((result.position[1] - t1.position[1]).abs() < 1e-5);
    assert!((result.position[2] - t1.position[2]).abs() < 1e-5);

    // t=1 should give t2
    let result = t1.lerp(&t2, 1.0);
    assert!((result.position[0] - t2.position[0]).abs() < 1e-5);
    assert!((result.position[1] - t2.position[1]).abs() < 1e-5);
    assert!((result.position[2] - t2.position[2]).abs() < 1e-5);

    // t=0.5 should give midpoint
    let result = t1.lerp(&t2, 0.5);
    assert!((result.position[0] - 5.0).abs() < 1e-5);
    assert!((result.position[1] - 10.0).abs() < 1e-5);
    assert!((result.position[2] - 15.0).abs() < 1e-5);
}

#[test]
fn test_transform_look_direction() {
    // Test that look_direction creates a transform with a valid rotation
    let transform = Transform::look_direction(
        Vec3::new(0.0, 0.0, -1.0), // Looking along -Z (forward direction)
        Vec3::new(0.0, 1.0, 0.0),  // Up is +Y
    );

    // Verify the rotation is normalized
    assert!(transform.rotation.is_normalized());

    // The forward vector should point along -Z
    let forward = transform.forward();
    // We expect forward to be approximately (0, 0, -1)
    assert!((forward[0]).abs() < 0.1); // X should be near 0
    assert!((forward[1]).abs() < 0.1); // Y should be near 0
    assert!((forward[2] - (-1.0)).abs() < 0.1); // Z should be near -1
}
