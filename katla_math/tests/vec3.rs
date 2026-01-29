use approx::assert_relative_eq;
use katla_math::Vec3;

#[test]
fn test_new_and_accessors() {
    let v = Vec3::new(1.0, 2.0, 3.0);
    assert_eq!(v.x(), 1.0);
    assert_eq!(v.y(), 2.0);
    assert_eq!(v.z(), 3.0);
    assert_eq!(v[0], 1.0);
    assert_eq!(v[1], 2.0);
    assert_eq!(v[2], 3.0);
}

#[test]
fn test_add_sub_assign() {
    let a = Vec3::new(1.0, 2.0, 3.0);
    let b = Vec3::new(4.0, -1.0, 0.5);
    assert_eq!(a + b, Vec3::new(5.0, 1.0, 3.5));
    assert_eq!(a - b, Vec3::new(-3.0, 3.0, 2.5));
    let mut c = a;
    c += b;
    assert_eq!(c, Vec3::new(5.0, 1.0, 3.5));
    let mut d = a;
    d -= b;
    assert_eq!(d, Vec3::new(-3.0, 3.0, 2.5));
}

#[test]
fn test_mul_scalar_and_vector() {
    let v = Vec3::new(2.0, -3.0, 4.0);
    assert_eq!(v * 2.0, Vec3::new(4.0, -6.0, 8.0));
    assert_eq!(2.0 * v, Vec3::new(4.0, -6.0, 8.0));
    let w = Vec3::new(1.0, 0.5, -1.0);
    assert_eq!(v * w, Vec3::new(2.0, -1.5, -4.0));
}

#[test]
fn test_normalize_and_is_zero() {
    let v = Vec3::new(3.0, 0.0, 4.0);
    let n = v.normalize();
    assert_relative_eq!(n.length(), 1.0);
    assert!(n.is_normalized());
    let zero = Vec3::default();
    assert!(zero.is_zero());
    assert!(!zero.is_normalized());
}

#[test]
fn test_dot_and_cross() {
    let a = Vec3::new(1.0, 0.0, 0.0);
    let b = Vec3::new(0.0, 1.0, 0.0);
    assert_eq!(a.dot(b), 0.0);
    let c = a.cross(b);
    assert_eq!(c, Vec3::new(0.0, 0.0, 1.0));
}

#[test]
fn test_lerp() {
    let a = Vec3::new(0.0, 0.0, 0.0);
    let b = Vec3::new(10.0, -5.0, 2.0);
    let mid = Vec3::lerp(a, b, 0.5);
    assert_eq!(mid, Vec3::new(5.0, -2.5, 1.0));
}

#[test]
fn test_edge_cases() {
    // Test with negative values
    let v = Vec3::new(-1.0, -2.0, -3.0);
    assert_eq!(v.x(), -1.0);
    assert_eq!(v.y(), -2.0);
    assert_eq!(v.z(), -3.0);

    // Test with very small values
    let v = Vec3::new(1e-6, 2e-6, 3e-6);
    assert_relative_eq!(v.x(), 1e-6);
    assert_relative_eq!(v.y(), 2e-6);
    assert_relative_eq!(v.z(), 3e-6);

    // Test with very large values
    let v = Vec3::new(1e6, -2e6, 3e6);
    assert_eq!(v.x(), 1e6);
    assert_eq!(v.y(), -2e6);
    assert_eq!(v.z(), 3e6);
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
fn test_division_by_scalar() {
    let v = Vec3::new(4.0, -6.0, 8.0);
    assert_eq!(v / 2.0, Vec3::new(2.0, -3.0, 4.0));
}

#[test]
fn test_lerp_edge_cases() {
    let a = Vec3::default();
    let b = Vec3::new(10.0, -5.0, 2.0);

    // Test lerp at boundaries
    assert_eq!(Vec3::lerp(a, b, 0.0), a);
    assert_eq!(Vec3::lerp(a, b, 1.0), b);

    // Test lerp with negative factor (should extrapolate)
    let extrapolated = Vec3::lerp(a, b, -0.5);
    assert_eq!(extrapolated, Vec3::new(-5.0, 2.5, -1.0));
}

#[test]
fn test_negation() {
    let v = Vec3::new(1.0, -2.0, 3.0);
    assert_eq!(-v, Vec3::new(-1.0, 2.0, -3.0));
    assert_eq!(-&v, Vec3::new(-1.0, 2.0, -3.0));
}

#[test]
fn test_index_mut() {
    let mut v = Vec3::new(1.0, 2.0, 3.0);
    v[0] = 10.0;
    v[1] = 20.0;
    v[2] = 30.0;
    assert_eq!(v, Vec3::new(10.0, 20.0, 30.0));
}

#[test]
fn test_mul_assign() {
    let mut v = Vec3::new(2.0, 3.0, 4.0);
    v *= 2.0;
    assert_eq!(v, Vec3::new(4.0, 6.0, 8.0));

    let mut a = Vec3::new(2.0, 3.0, 4.0);
    let b = Vec3::new(2.0, 2.0, 2.0);
    a *= b;
    assert_eq!(a, Vec3::new(4.0, 6.0, 8.0));
}

#[test]
fn test_div_assign() {
    let mut v = Vec3::new(4.0, 6.0, 8.0);
    v /= 2.0;
    assert_eq!(v, Vec3::new(2.0, 3.0, 4.0));

    let mut a = Vec3::new(4.0, 6.0, 8.0);
    let b = Vec3::new(2.0, 2.0, 2.0);
    a /= b;
    assert_eq!(a, Vec3::new(2.0, 3.0, 4.0));
}

#[test]
fn test_length() {
    let v = Vec3::new(3.0, 4.0, 0.0);
    assert_relative_eq!(v.length(), 5.0);
    assert_eq!(v.length_squared(), 25.0);
}

#[test]
fn test_from_array() {
    let arr = [1.0, 2.0, 3.0];
    let v: Vec3 = arr.into();
    assert_eq!(v, Vec3::new(1.0, 2.0, 3.0));
}

#[test]
fn test_default() {
    let v: Vec3 = Default::default();
    assert_eq!(v, Vec3::new(0.0, 0.0, 0.0));
}

#[test]
#[should_panic(expected = "INDEXING OUT_OF_BOUNDS")]
fn test_index_out_of_bounds() {
    let v = Vec3::new(1.0, 2.0, 3.0);
    let _ = v[3];
}

#[test]
fn test_cross_product_anti_commutative() {
    let a = Vec3::new(1.0, 2.0, 3.0);
    let b = Vec3::new(4.0, 5.0, 6.0);
    assert_eq!(a.cross(b), -b.cross(a));
}

#[test]
fn test_dot_product_commutative() {
    let a = Vec3::new(1.0, 2.0, 3.0);
    let b = Vec3::new(4.0, 5.0, 6.0);
    assert_relative_eq!(a.dot(b), b.dot(a));
}

#[test]
fn test_dot_product_distributive() {
    let a = Vec3::new(1.0, 2.0, 3.0);
    let b = Vec3::new(4.0, 5.0, 6.0);
    let c = Vec3::new(7.0, 8.0, 9.0);
    assert_relative_eq!(a.dot(b + c), a.dot(b) + a.dot(c));
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
fn test_lerp_is_linear() {
    let a = Vec3::new(0.0, 0.0, 0.0);
    let b = Vec3::new(10.0, 20.0, 30.0);

    let quarter = Vec3::lerp(a, b, 0.25);
    let half = Vec3::lerp(a, b, 0.5);

    assert_relative_eq!(half.x(), quarter.x() * 2.0);
    assert_relative_eq!(half.y(), quarter.y() * 2.0);
    assert_relative_eq!(half.z(), quarter.z() * 2.0);
}

#[test]
fn test_vector_division_by_vector() {
    let a = Vec3::new(10.0, 20.0, 30.0);
    let b = Vec3::new(2.0, 5.0, 3.0);
    let result = a / b;
    assert_relative_eq!(result.x(), 5.0);
    assert_relative_eq!(result.y(), 4.0);
    assert_relative_eq!(result.z(), 10.0);
}

#[test]
fn test_length_properties() {
    let v = Vec3::new(3.0, 4.0, 0.0);
    assert_relative_eq!(v.length(), v.length_squared().sqrt());
    assert!(v.length() >= 0.0);
    assert!(v.length_squared() >= 0.0);
}

#[test]
fn test_zero_vector_properties() {
    let zero = Vec3::default();
    assert_eq!(zero.length(), 0.0);
    assert_eq!(zero.length_squared(), 0.0);
    assert_eq!(zero.dot(zero), 0.0);
    assert_eq!(zero.cross(zero), zero);
}

#[test]
fn test_unit_axes() {
    assert_eq!(Vec3::X_AXIS, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(Vec3::Y_AXIS, Vec3::new(0.0, 1.0, 0.0));
    assert_eq!(Vec3::Z_AXIS, Vec3::new(0.0, 0.0, 1.0));

    assert!(Vec3::X_AXIS.is_normalized());
    assert!(Vec3::Y_AXIS.is_normalized());
    assert!(Vec3::Z_AXIS.is_normalized());
}
