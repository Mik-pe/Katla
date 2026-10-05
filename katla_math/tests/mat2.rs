use katla_math::{Mat2, Vec2};

#[test]
fn test_mat2_inverse_singular() {
    let m = Mat2::new(1.0, 2.0, 2.0, 4.0); // Singular matrix
    let inv = m.inverse();
    assert!(inv.is_none());
}

#[test]
fn test_mat2_rotation_transforms_vector() {
    // Rotate (1, 0) by 90 degrees should give (0, 1)
    let m = Mat2::from_rotation(std::f32::consts::FRAC_PI_2);
    let v = Vec2::new(1.0, 0.0);
    let result = m * v;

    assert!((result.x() - 0.0).abs() < 1e-5);
    assert!((result.y() - 1.0).abs() < 1e-5);
}

#[test]
fn test_mat2_inverse_then_mul_is_identity() {
    let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let inv = m.inverse().unwrap();
    let identity = m * inv;

    assert!((identity[0][0] - 1.0).abs() < 1e-5);
    assert!((identity[0][1]).abs() < 1e-5);
    assert!((identity[1][0]).abs() < 1e-5);
    assert!((identity[1][1] - 1.0).abs() < 1e-5);
}

#[test]
fn test_mat2_rotation_roundtrip() {
    let angle = std::f32::consts::FRAC_PI_3;
    let m = Mat2::from_rotation(angle);
    let extracted = m.to_rotation();

    assert!((angle - extracted).abs() < 1e-5);
}

#[test]
fn test_mat2_scale_roundtrip() {
    let scale = Vec2::new(2.5, 3.7);
    let m = Mat2::from_scale(scale);
    let extracted = m.to_scale();

    assert!((scale.x() - extracted.x()).abs() < 1e-5);
    assert!((scale.y() - extracted.y()).abs() < 1e-5);
}
