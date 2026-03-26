use katla_math::vec2::Vec2;

#[test]
fn test_normalize_zero() {
    let v = Vec2::ZERO;
    let normalized = v.normalize();
    assert_eq!(normalized, Vec2::ZERO);
}

#[test]
fn test_perpendicular() {
    let v = Vec2::new(1.0, 0.0);
    let perp = v.perpendicular();

    // Rotated 90 degrees counter-clockwise
    assert!((perp.x() - 0.0).abs() < 1e-5);
    assert!((perp.y() - 1.0).abs() < 1e-5);

    // Should be perpendicular (dot product = 0)
    assert!((v.dot(perp)).abs() < 1e-5);
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
fn test_from_angle_round_trip() {
    let v = Vec2::new(3.0, 4.0);
    let angle = v.angle();
    let reconstructed = Vec2::from_angle(angle);

    assert!(reconstructed.is_normalized());
}

#[test]
fn test_distance_vs_distance_squared() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(4.0, 5.0);
    let dist = a.distance(b);
    let dist_sq = a.distance_squared(b);

    assert!((dist_sq - dist * dist).abs() < 1e-5);
}

#[test]
fn test_cross_anti_commutative() {
    let a = Vec2::new(1.0, 2.0);
    let b = Vec2::new(3.0, 4.0);
    assert!((a.cross(b) + b.cross(a)).abs() < 1e-5);
}

#[test]
fn test_normalize_near_zero_vector() {
    let near_zero = Vec2::new(1e-30, 2e-30);
    let normalized = near_zero.normalize();
    assert!(normalized.x().is_finite());
    assert!(normalized.y().is_finite());
}
