use katla_math::vec2::Vec2;

#[test]
fn test_add() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(3.0, 4.0);
    let result = a + b;
    assert_eq!(result, Vec2::new(4.0, 6.0));
}

#[test]
fn test_sub() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(3.0, 4.0);
    let result = a - b;
    assert_eq!(result, Vec2::new(-2.0, -2.0));
}

#[test]
fn test_mul() {
    let a = Vec2::new(1.0, 2.0);
    let b = 3.0;
    let result = a * b;
    assert_eq!(result, Vec2::new(3.0, 6.0));
}

#[test]
fn test_div() {
    let a = Vec2::new(1.0, 2.0);
    let b = 2.0;
    let result = a / b;
    assert_eq!(result, Vec2::new(1.0 / 2.0, 2.0 / 2.0));
}

#[test]
fn test_length() {
    let a = Vec2::new(3.0, 4.0);
    let length = a.length();
    assert_eq!(length, 5.0);
}

#[test]
fn test_eq() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(1.0, 2.0);
    assert_eq!(a, b);
}

#[test]
fn test_ne() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(2.0, 3.0);
    assert_ne!(a, b);
}

#[test]
fn test_ord() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(2.0, 3.0);
    assert!(a < b);
}

#[test]
fn test_length_squared() {
    let v = Vec2::new(3.0, 4.0);
    assert_eq!(v.length_squared(), 25.0);
}

#[test]
fn test_normalize() {
    let v = Vec2::new(3.0, 4.0);
    let normalized = v.normalize();
    assert!((normalized.length() - 1.0).abs() < 1e-5);
}

#[test]
fn test_normalize_zero() {
    let v = Vec2::ZERO;
    let normalized = v.normalize();
    assert_eq!(normalized, Vec2::ZERO);
}

#[test]
fn test_is_normalized() {
    let v = Vec2::new(0.5, 0.8660254); // cos(30°), sin(30°)
    assert!(v.is_normalized());

    let v2 = Vec2::new(1.0, 1.0);
    assert!(!v2.is_normalized());
}

#[test]
fn test_is_zero() {
    assert!(Vec2::ZERO.is_zero());
    assert!(!Vec2::ONE.is_zero());
    assert!(!Vec2::new(0.0, 1.0).is_zero());
}

#[test]
fn test_dot() {
    let v1 = Vec2::new(1.0, 2.0);
    let v2 = Vec2::new(3.0, 4.0);
    assert_eq!(v1.dot(&v2), 11.0); // 1*3 + 2*4
}

#[test]
fn test_dot_commutative() {
    let v1 = Vec2::new(1.0, 2.0);
    let v2 = Vec2::new(3.0, 4.0);
    assert_eq!(v1.dot(&v2), v2.dot(&v1));
}

#[test]
fn test_lerp() {
    let a = Vec2::new(0.0, 0.0);
    let b = Vec2::new(10.0, 10.0);
    let result = a.lerp(&b, 0.5);
    assert_eq!(result, Vec2::new(5.0, 5.0));
}

#[test]
fn test_lerp_boundaries() {
    let a = Vec2::new(0.0, 0.0);
    let b = Vec2::new(10.0, 10.0);

    assert_eq!(a.lerp(&b, 0.0), a);
    assert_eq!(a.lerp(&b, 1.0), b);
}

#[test]
fn test_cross() {
    // 2D cross product gives the z-component of 3D cross product
    let i = Vec2::new(1.0, 0.0);  // X-axis
    let j = Vec2::new(0.0, 1.0);  // Y-axis
    let cross = i.cross(&j);

    // In 3D: (1,0,0) × (0,1,0) = (0,0,1), so z = 1
    assert!((cross - 1.0).abs() < 1e-5);
}

#[test]
fn test_cross_anti_commutative() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(3.0, 4.0);
    assert!((a.cross(&b) + b.cross(&a)).abs() < 1e-5);
}

#[test]
fn test_perpendicular() {
    let v = Vec2::new(1.0, 0.0);
    let perp = v.perpendicular();

    // Rotated 90 degrees counter-clockwise
    assert!((perp.x - 0.0).abs() < 1e-5);
    assert!((perp.y - 1.0).abs() < 1e-5);

    // Should be perpendicular (dot product = 0)
    assert!((v.dot(&perp)).abs() < 1e-5);
}

#[test]
fn test_perpendicular_double_rotation() {
    let v = Vec2::new(1.0, 0.0);
    let perp = v.perpendicular();
    let perp2 = perp.perpendicular();

    // Rotating 90 degrees twice should reverse the vector
    assert!((perp2 + v).length() < 1e-5);
}

#[test]
fn test_angle() {
    let v = Vec2::new(1.0, 0.0);  // +X axis
    assert!(v.angle().abs() < 1e-5);

    let v = Vec2::new(0.0, 1.0);  // +Y axis (90 degrees)
    assert!((v.angle() - std::f32::consts::FRAC_PI_2).abs() < 1e-5);

    let v = Vec2::new(-1.0, 0.0); // -X axis (180 degrees)
    assert!((v.angle() - std::f32::consts::PI).abs() < 1e-5);
}

#[test]
fn test_from_angle() {
    let v = Vec2::from_angle(0.0);
    assert!((v.x - 1.0).abs() < 1e-5);
    assert!((v.y - 0.0).abs() < 1e-5);

    let v = Vec2::from_angle(std::f32::consts::FRAC_PI_2);
    assert!((v.x - 0.0).abs() < 1e-5);
    assert!((v.y - 1.0).abs() < 1e-5);

    let v = Vec2::from_angle(std::f32::consts::PI);
    assert!((v.x - (-1.0)).abs() < 1e-5);
    assert!((v.y - 0.0).abs() < 1e-5);
}

#[test]
fn test_from_angle_round_trip() {
    let v = Vec2::new(3.0, 4.0);
    let angle = v.angle();
    let reconstructed = Vec2::from_angle(angle);

    assert!(reconstructed.is_normalized());
    // The angle should match, but the vector might not (length difference)
}

#[test]
fn test_distance() {
    let a = Vec2::new(1.0, 0.0);
    let b = Vec2::new(4.0, 0.0);
    assert!((a.distance(&b) - 3.0).abs() < 1e-5);
}

#[test]
fn test_distance_squared() {
    let a = Vec2::new(1.0, 0.0);
    let b = Vec2::new(4.0, 0.0);
    assert!((a.distance_squared(&b) - 9.0).abs() < 1e-5);
}

#[test]
fn test_distance_vs_distance_squared() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(4.0, 5.0);
    let dist = a.distance(&b);
    let dist_sq = a.distance_squared(&b);

    assert!((dist_sq - dist * dist).abs() < 1e-5);
}

#[test]
fn test_swizzle_xx() {
    let v = Vec2::new(1.0, 2.0);
    let xx = v.xx();
    assert_eq!(xx, Vec2::new(1.0, 1.0));
}

#[test]
fn test_swizzle_yx() {
    let v = Vec2::new(1.0, 2.0);
    let yx = v.yx();
    assert_eq!(yx, Vec2::new(2.0, 1.0));
}

#[test]
fn test_swizzle_yy() {
    let v = Vec2::new(1.0, 2.0);
    let yy = v.yy();
    assert_eq!(yy, Vec2::new(2.0, 2.0));
}

#[test]
fn test_one_constant() {
    assert_eq!(Vec2::ONE, Vec2::new(1.0, 1.0));
}
