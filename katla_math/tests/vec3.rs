use approx::assert_relative_eq;
use katla_math::Vec3;

#[test]
fn test_cross_product_edge_cases() {
    // Parallel vectors should produce zero vector
    let a = Vec3::new(1.0, 2.0, 3.0);
    let b = Vec3::new(2.0, 4.0, 6.0);
    assert_eq!(a.cross(b), Vec3::default());

    // Perpendicular vectors
    let i = Vec3::new(1.0, 0.0, 0.0);
    let j = Vec3::new(0.0, 1.0, 0.0);
    assert_eq!(i.cross(j), Vec3::new(0.0, 0.0, 1.0));
}

#[test]
fn test_cross_product_anti_commutative() {
    let a = Vec3::new(1.0, 2.0, 3.0);
    let b = Vec3::new(4.0, 5.0, 6.0);
    assert_eq!(a.cross(b), -b.cross(a));
}

#[test]
fn test_dot_product_distributive() {
    let a = Vec3::new(1.0, 2.0, 3.0);
    let b = Vec3::new(4.0, 5.0, 6.0);
    let c = Vec3::new(7.0, 8.0, 9.0);
    assert_relative_eq!(a.dot(b + c), a.dot(b) + a.dot(c));
}

#[test]
fn test_normalization_edge_cases() {
    let zero = Vec3::default();
    let normalized_zero = zero.normalize();
    assert!(normalized_zero.is_zero());

    // Test normalization of already-normalized vector
    let v = Vec3::new(0.5, 0.8660254, 0.0);
    let n = v.normalize();
    assert_relative_eq!(n.length(), 1.0);
}

#[test]
fn test_normalization_preserves_direction() {
    let v = Vec3::new(3.0, 4.0, 0.0);
    let n = v.normalize();
    let normalized_again = n.normalize();
    assert_relative_eq!(n.x(), normalized_again.x());
    assert_relative_eq!(n.y(), normalized_again.y());
    assert_relative_eq!(n.z(), normalized_again.z());
}

#[test]
fn test_lerp_edge_cases() {
    let a = Vec3::default();
    let b = Vec3::new(10.0, -5.0, 2.0);

    // Test lerp at boundaries
    assert_eq!(a.lerp(b, 0.0), a);
    assert_eq!(a.lerp(b, 1.0), b);

    // Test lerp with negative factor (should extrapolate)
    let extrapolated = a.lerp(b, -0.5);
    assert_eq!(extrapolated, Vec3::new(-5.0, 2.5, -1.0));
}

#[test]
fn test_reflect() {
    let incident = Vec3::new(1.0, -1.0, 0.0).normalize();
    let normal = Vec3::new(0.0, 1.0, 0.0);
    let reflected = incident.reflect(normal);

    // Angle of incidence should equal angle of reflection
    assert!((reflected.x() - incident.x()).abs() < 1e-5);
    assert!((reflected.y() - (-incident.y())).abs() < 1e-5);
    assert!((reflected.z() - incident.z()).abs() < 1e-5);
}

#[test]
fn test_project_plus_reject_equals_original() {
    let v = Vec3::new(3.0, 4.0, 2.0);
    let onto = Vec3::new(1.0, 1.0, 0.0);

    let projected = v.project(onto);
    let rejected = v.reject(onto);

    // Project + Reject should equal original
    let sum = projected + rejected;
    assert!((sum.x() - v.x()).abs() < 1e-5);
    assert!((sum.y() - v.y()).abs() < 1e-5);
    assert!((sum.z() - v.z()).abs() < 1e-5);
}

#[test]
fn test_angle_between_parallel() {
    let a = Vec3::new(1.0, 0.0, 0.0);
    let b = Vec3::new(2.0, 0.0, 0.0);
    let angle = a.angle_between(b);

    assert!(angle.abs() < 1e-5);
}

#[test]
fn test_angle_between_perpendicular() {
    let a = Vec3::new(1.0, 0.0, 0.0);
    let b = Vec3::new(0.0, 1.0, 0.0);
    let angle = a.angle_between(b);

    assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
}

#[test]
fn test_angle_between_opposite() {
    let a = Vec3::new(1.0, 0.0, 0.0);
    let b = Vec3::new(-1.0, 0.0, 0.0);
    let angle = a.angle_between(b);

    assert!((angle - std::f32::consts::PI).abs() < 1e-5);
}

#[test]
fn test_from_spherical() {
    // From +Y axis (phi = 0), any theta gives +Y
    let v = Vec3::from_spherical(0.0, 0.0);
    assert!((v.x() - 0.0).abs() < 1e-5);
    assert!((v.y() - 1.0).abs() < 1e-5);
    assert!((v.z() - 0.0).abs() < 1e-5);

    // From X axis (phi = pi/2, theta = 0)
    let v = Vec3::from_spherical(std::f32::consts::FRAC_PI_2, 0.0);
    assert!((v.x() - 1.0).abs() < 1e-5);
    assert!((v.y() - 0.0).abs() < 1e-5);
    assert!((v.z() - 0.0).abs() < 1e-5);

    // From Z axis (phi = pi/2, theta = pi/2)
    let v = Vec3::from_spherical(std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
    assert!((v.x() - 0.0).abs() < 1e-5);
    assert!((v.y() - 0.0).abs() < 1e-5);
    assert!((v.z() - 1.0).abs() < 1e-5);
}

#[test]
fn test_from_spherical_unit_vectors() {
    let v = Vec3::from_spherical(std::f32::consts::FRAC_PI_2, 0.0);
    assert!(v.is_normalized());

    let v = Vec3::from_spherical(0.5, 1.2);
    assert!(v.is_normalized());
}

#[test]
#[should_panic(expected = "INDEXING OUT_OF_BOUNDS")]
fn test_index_out_of_bounds() {
    let v = Vec3::new(1.0, 2.0, 3.0);
    let _ = v[3];
}

#[test]
fn test_normalize_near_zero_vector() {
    let near_zero = Vec3::new(1e-30, 2e-30, 3e-30);
    let normalized = near_zero.normalize();
    // Should not produce NaN or Inf
    assert!(normalized.x().is_finite());
    assert!(normalized.y().is_finite());
    assert!(normalized.z().is_finite());
}

#[test]
fn test_cross_product_normalization() {
    // Use perpendicular unit vectors so cross product is exactly unit length
    let a = Vec3::new(1.0, 2.0, 3.0).normalize();
    let b = Vec3::new(3.0, -2.0, 1.0); // constructed to be perpendicular
    // Make b perpendicular to a: b = b - (b.a)*a
    let b = (b - a * b.dot(a)).normalize();
    let cross = a.cross(b);
    // Cross product of two perpendicular unit vectors should be a unit vector
    assert!((cross.length() - 1.0).abs() < 1e-5);
    // Should be perpendicular to both
    assert!(cross.dot(a).abs() < 1e-5);
    assert!(cross.dot(b).abs() < 1e-5);
}
