use katla_math::{Mat2, Vec2};

#[test]
fn test_mat2_new() {
    let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
    assert_eq!(m[0][0], 1.0);
    assert_eq!(m[0][1], 3.0);
    assert_eq!(m[1][0], 2.0);
    assert_eq!(m[1][1], 4.0);
}

#[test]
fn test_mat2_identity() {
    let m = Mat2::identity();
    assert_eq!(m[0][0], 1.0);
    assert_eq!(m[0][1], 0.0);
    assert_eq!(m[1][0], 0.0);
    assert_eq!(m[1][1], 1.0);
}

#[test]
fn test_mat2_zero() {
    let m = Mat2::zero();
    assert_eq!(m[0][0], 0.0);
    assert_eq!(m[0][1], 0.0);
    assert_eq!(m[1][0], 0.0);
    assert_eq!(m[1][1], 0.0);
}

#[test]
fn test_mat2_from_rotation() {
    let m = Mat2::from_rotation(std::f32::consts::FRAC_PI_2); // 90 degrees
    assert!((m[0][0] - 0.0).abs() < 1e-5);
    assert!((m[0][1] - 1.0).abs() < 1e-5);
    assert!((m[1][0] - (-1.0)).abs() < 1e-5);
    assert!((m[1][1] - 0.0).abs() < 1e-5);
}

#[test]
fn test_mat2_from_scale() {
    let scale = Vec2::new(2.0, 3.0);
    let m = Mat2::from_scale(scale);
    assert_eq!(m[0][0], 2.0);
    assert_eq!(m[0][1], 0.0);
    assert_eq!(m[1][0], 0.0);
    assert_eq!(m[1][1], 3.0);
}

#[test]
fn test_mat2_mul() {
    let m1 = Mat2::new(1.0, 2.0, 3.0, 4.0);
    // m1[0] = Vec2 { x: 1.0, y: 3.0 } (column 0)
    // m1[1] = Vec2 { x: 2.0, y: 4.0 } (column 1)
    // Matrix: [[1, 2], [3, 4]] in row-major notation

    let m2 = Mat2::new(5.0, 6.0, 7.0, 8.0);
    // m2[0] = Vec2 { x: 5.0, y: 7.0 } (column 0)
    // m2[1] = Vec2 { x: 6.0, y: 8.0 } (column 1)
    // Matrix: [[5, 6], [7, 8]] in row-major notation

    let result = m1 * m2;

    // Standard matrix multiplication: [[1, 2], [3, 4]] * [[5, 6], [7, 8]]
    // result[0][0] = 1*5 + 2*7 = 5 + 14 = 19
    // result[0][1] = 3*5 + 4*7 = 15 + 28 = 43
    // result[1][0] = 1*6 + 2*8 = 6 + 16 = 22
    // result[1][1] = 3*6 + 4*8 = 18 + 32 = 50

    assert!((result[0][0] - 19.0).abs() < 1e-5);
    assert!((result[0][1] - 43.0).abs() < 1e-5);
    assert!((result[1][0] - 22.0).abs() < 1e-5);
    assert!((result[1][1] - 50.0).abs() < 1e-5);
}

#[test]
fn test_mat2_mul_vec2() {
    let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
    // m[0] = Vec2 { x: 1.0, y: 3.0 } (column 0)
    // m[1] = Vec2 { x: 2.0, y: 4.0 } (column 1)
    let v = Vec2::new(5.0, 6.0);
    let result = m * v;

    // Matrix-vector multiplication with column-major M:
    // result[row] = sum over col of M[col][row] * v[col]
    // result.x() = m[0][0] * v.x + m[1][0] * v.y = 1*5 + 2*6 = 17
    // result.y() = m[0][1] * v.x + m[1][1] * v.y = 3*5 + 4*6 = 39
    assert!((result.x() - 17.0).abs() < 1e-5);
    assert!((result.y() - 39.0).abs() < 1e-5);
}

#[test]
fn test_mat2_transpose() {
    let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let result = m.transpose();

    assert_eq!(result[0][0], 1.0);
    assert_eq!(result[0][1], 2.0);
    assert_eq!(result[1][0], 3.0);
    assert_eq!(result[1][1], 4.0);
}

#[test]
fn test_mat2_determinant() {
    let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let det = m.determinant();
    // det([1 3; 2 4]) = 1*4 - 2*3 = 4 - 6 = -2
    assert!((det - (-2.0)).abs() < 1e-5);
}

#[test]
fn test_mat2_inverse() {
    let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let inv = m.inverse();
    assert!(inv.is_some());

    let inv = inv.unwrap();
    let identity = m * inv;

    // Should be approximately identity
    assert!((identity[0][0] - 1.0).abs() < 1e-5);
    assert!((identity[0][1]).abs() < 1e-5);
    assert!((identity[1][0]).abs() < 1e-5);
    assert!((identity[1][1] - 1.0).abs() < 1e-5);
}

#[test]
fn test_mat2_inverse_singular() {
    let m = Mat2::new(1.0, 2.0, 2.0, 4.0); // Singular matrix
    let inv = m.inverse();
    assert!(inv.is_none());
}

#[test]
fn test_mat2_to_rotation() {
    let m = Mat2::from_rotation(std::f32::consts::FRAC_PI_4); // 45 degrees
    let angle = m.to_rotation();
    assert!((angle - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
}

#[test]
fn test_mat2_to_scale() {
    let scale = Vec2::new(2.0, 3.0);
    let m = Mat2::from_scale(scale);
    let extracted = m.to_scale();
    assert!((extracted.x() - scale.x()).abs() < 1e-5);
    assert!((extracted.y() - scale.y()).abs() < 1e-5);
}

#[test]
fn test_mat2_mul_scalar() {
    let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let result = m * 2.0;

    assert_eq!(result[0][0], 2.0);
    assert_eq!(result[0][1], 6.0);
    assert_eq!(result[1][0], 4.0);
    assert_eq!(result[1][1], 8.0);
}

#[test]
fn test_mat2_div_scalar() {
    let m = Mat2::new(2.0, 4.0, 6.0, 8.0);
    let result = m / 2.0;

    assert_eq!(result[0][0], 1.0);
    assert_eq!(result[0][1], 3.0);
    assert_eq!(result[1][0], 2.0);
    assert_eq!(result[1][1], 4.0);
}

#[test]
fn test_mat2_add() {
    let m1 = Mat2::new(1.0, 2.0, 3.0, 4.0);
    // m1[0] = Vec2 { x: 1.0, y: 3.0 } (column 0)
    // m1[1] = Vec2 { x: 2.0, y: 4.0 } (column 1)
    let m2 = Mat2::new(5.0, 6.0, 7.0, 8.0);
    // m2[0] = Vec2 { x: 5.0, y: 7.0 } (column 0)
    // m2[1] = Vec2 { x: 6.0, y: 8.0 } (column 1)
    let result = m1 + m2;
    // result[0] = Vec2 { x: 6.0, y: 10.0 }
    // result[1] = Vec2 { x: 8.0, y: 12.0 }

    assert_eq!(result[0][0], 6.0);   // column 0, x: 1+5=6
    assert_eq!(result[0][1], 10.0);  // column 0, y: 3+7=10
    assert_eq!(result[1][0], 8.0);   // column 1, x: 2+6=8
    assert_eq!(result[1][1], 12.0);  // column 1, y: 4+8=12
}

#[test]
fn test_mat2_sub() {
    let m1 = Mat2::new(5.0, 6.0, 7.0, 8.0);
    let m2 = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let result = m1 - m2;

    assert_eq!(result[0][0], 4.0);
    assert_eq!(result[0][1], 4.0);
    assert_eq!(result[1][0], 4.0);
    assert_eq!(result[1][1], 4.0);
}

#[test]
fn test_mat2_neg() {
    let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let result = -m;

    assert_eq!(result[0][0], -1.0);
    assert_eq!(result[0][1], -3.0);
    assert_eq!(result[1][0], -2.0);
    assert_eq!(result[1][1], -4.0);
}

#[test]
fn test_mat2_partial_eq() {
    let m1 = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let m2 = Mat2::new(1.0, 2.0, 3.0, 4.0);
    let m3 = Mat2::new(1.0, 2.0, 3.0, 5.0);

    assert_eq!(m1, m2);
    assert_ne!(m1, m3);
}

#[test]
fn test_mat2_default() {
    let m = Mat2::default();
    assert_eq!(m, Mat2::identity());
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

#[test]
fn test_mat2_constants() {
    assert_eq!(Mat2::zero(), Mat2::zero());
    assert_eq!(Mat2::identity(), Mat2::identity());
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
fn test_mat2_scale_transforms_vector() {
    let m = Mat2::from_scale(Vec2::new(2.0, 3.0));
    let v = Vec2::new(1.0, 1.0);
    let result = m * v;

    assert!((result.x() - 2.0).abs() < 1e-5);
    assert!((result.y() - 3.0).abs() < 1e-5);
}
