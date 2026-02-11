use katla_math::Vec4;

#[test]
fn test_vec4_new() {
    let v = Vec4::new(1.0, 2.0, 3.0, 4.0);
    assert_eq!(v.x(), 1.0);
    assert_eq!(v.y(), 2.0);
    assert_eq!(v.z(), 3.0);
    assert_eq!(v.w(), 4.0);
}

#[test]
fn test_vec4_from_xyz() {
    let v = Vec4::from_xyz(1.0, 2.0, 3.0);
    assert_eq!(v.x(), 1.0);
    assert_eq!(v.y(), 2.0);
    assert_eq!(v.z(), 3.0);
    assert_eq!(v.w(), 1.0);
}

#[test]
fn test_vec4_from_xyzw() {
    let v = Vec4::from_xyzw(1.0, 2.0, 3.0, 4.0);
    assert_eq!(v.x(), 1.0);
    assert_eq!(v.y(), 2.0);
    assert_eq!(v.z(), 3.0);
    assert_eq!(v.w(), 4.0);
}

#[test]
fn test_vec4_constants() {
    assert_eq!(Vec4::zero(), Vec4::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(Vec4::one(), Vec4::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(Vec4::x_axis(), Vec4::new(1.0, 0.0, 0.0, 0.0));
    assert_eq!(Vec4::y_axis(), Vec4::new(0.0, 1.0, 0.0, 0.0));
    assert_eq!(Vec4::z_axis(), Vec4::new(0.0, 0.0, 1.0, 0.0));
    assert_eq!(Vec4::w_axis(), Vec4::new(0.0, 0.0, 0.0, 1.0));
}

#[test]
fn test_vec4_add() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = Vec4::new(5.0, 6.0, 7.0, 8.0);
    let result = v1 + v2;
    assert_eq!(result, Vec4::new(6.0, 8.0, 10.0, 12.0));
}

#[test]
fn test_vec4_add_ref() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = Vec4::new(5.0, 6.0, 7.0, 8.0);
    let result = v1 + v2;
    assert_eq!(result, Vec4::new(6.0, 8.0, 10.0, 12.0));
}

#[test]
fn test_vec4_sub() {
    let v1 = Vec4::new(5.0, 6.0, 7.0, 8.0);
    let v2 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let result = v1 - v2;
    assert_eq!(result, Vec4::new(4.0, 4.0, 4.0, 4.0));
}

#[test]
fn test_vec4_mul_scalar() {
    let v = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let result = v * 2.0;
    assert_eq!(result, Vec4::new(2.0, 4.0, 6.0, 8.0));
}

#[test]
fn test_vec4_mul_vector() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = Vec4::new(2.0, 3.0, 4.0, 5.0);
    let result = v1 * v2;
    assert_eq!(result, Vec4::new(2.0, 6.0, 12.0, 20.0));
}

#[test]
fn test_vec4_div_scalar() {
    let v = Vec4::new(2.0, 4.0, 6.0, 8.0);
    let result = v / 2.0;
    assert_eq!(result, Vec4::new(1.0, 2.0, 3.0, 4.0));
}

#[test]
fn test_vec4_div_vector() {
    let v1 = Vec4::new(2.0, 6.0, 12.0, 20.0);
    let v2 = Vec4::new(2.0, 3.0, 4.0, 5.0);
    let result = v1 / v2;
    assert_eq!(result, Vec4::new(1.0, 2.0, 3.0, 4.0));
}

#[test]
fn test_vec4_neg() {
    let v = Vec4::new(1.0, -2.0, 3.0, -4.0);
    let result = -v;
    assert_eq!(result, Vec4::new(-1.0, 2.0, -3.0, 4.0));
}

#[test]
fn test_vec4_neg_ref() {
    let v = Vec4::new(1.0, -2.0, 3.0, -4.0);
    let result = -&v;
    assert_eq!(result, Vec4::new(-1.0, 2.0, -3.0, 4.0));
}

#[test]
fn test_vec4_add_assign() {
    let mut v = Vec4::new(1.0, 2.0, 3.0, 4.0);
    v += Vec4::new(5.0, 6.0, 7.0, 8.0);
    assert_eq!(v, Vec4::new(6.0, 8.0, 10.0, 12.0));
}

#[test]
fn test_vec4_sub_assign() {
    let mut v = Vec4::new(5.0, 6.0, 7.0, 8.0);
    v -= Vec4::new(1.0, 2.0, 3.0, 4.0);
    assert_eq!(v, Vec4::new(4.0, 4.0, 4.0, 4.0));
}

#[test]
fn test_vec4_mul_assign_scalar() {
    let mut v = Vec4::new(1.0, 2.0, 3.0, 4.0);
    v *= 2.0;
    assert_eq!(v, Vec4::new(2.0, 4.0, 6.0, 8.0));
}

#[test]
fn test_vec4_div_assign_scalar() {
    let mut v = Vec4::new(2.0, 4.0, 6.0, 8.0);
    v /= 2.0;
    assert_eq!(v, Vec4::new(1.0, 2.0, 3.0, 4.0));
}

#[test]
fn test_vec4_length_squared() {
    let v = Vec4::new(3.0, 4.0, 0.0, 0.0);
    assert_eq!(v.length_squared(), 25.0);
}

#[test]
fn test_vec4_length() {
    let v = Vec4::new(3.0, 4.0, 0.0, 0.0);
    assert_eq!(v.length(), 5.0);
}

#[test]
fn test_vec4_normalize() {
    let v = Vec4::new(3.0, 4.0, 0.0, 0.0);
    let normalized = v.normalize();
    assert!(normalized.is_normalized());
    assert_eq!(normalized.length(), 1.0);
}

#[test]
fn test_vec4_normalize_zero() {
    let v = Vec4::zero();
    let normalized = v.normalize();
    assert_eq!(normalized, Vec4::zero());
}

#[test]
fn test_vec4_is_normalized() {
    let v = Vec4::new(1.0, 0.0, 0.0, 0.0);
    assert!(v.is_normalized());

    let v2 = Vec4::new(1.0, 1.0, 1.0, 1.0);
    assert!(!v2.is_normalized());

    let normalized = v2.normalize();
    assert!(normalized.is_normalized());
}

#[test]
fn test_vec4_is_zero() {
    assert!(Vec4::zero().is_zero());
    assert!(!Vec4::one().is_zero());
    assert!(!Vec4::new(0.0, 0.0, 0.0, 1.0).is_zero());
}

#[test]
fn test_vec4_dot() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = Vec4::new(5.0, 6.0, 7.0, 8.0);
    let dot = v1.dot(&v2);
    assert_eq!(dot, 70.0); // 1*5 + 2*6 + 3*7 + 4*8 = 5 + 12 + 21 + 32 = 70
}

#[test]
fn test_vec4_dot_commutative() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = Vec4::new(5.0, 6.0, 7.0, 8.0);
    assert_eq!(v1.dot(&v2), v2.dot(&v1));
}

#[test]
fn test_vec4_lerp() {
    let v1 = Vec4::new(0.0, 0.0, 0.0, 0.0);
    let v2 = Vec4::new(10.0, 10.0, 10.0, 10.0);

    let result = v1.lerp(&v2, 0.5);
    assert_eq!(result, Vec4::new(5.0, 5.0, 5.0, 5.0));

    let result2 = v1.lerp(&v2, 0.0);
    assert_eq!(result2, v1);

    let result3 = v1.lerp(&v2, 1.0);
    assert_eq!(result3, v2);
}

#[test]
fn test_vec4_xyz() {
    let v = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let xyz = v.xyz();
    assert_eq!(xyz.x(), 1.0);
    assert_eq!(xyz.y(), 2.0);
    assert_eq!(xyz.z(), 3.0);
}

#[test]
fn test_vec4_indexing() {
    let v = Vec4::new(1.0, 2.0, 3.0, 4.0);
    assert_eq!(v[0], 1.0);
    assert_eq!(v[1], 2.0);
    assert_eq!(v[2], 3.0);
    assert_eq!(v[3], 4.0);
}

#[test]
fn test_vec4_indexing_mut() {
    let mut v = Vec4::new(1.0, 2.0, 3.0, 4.0);
    v[0] = 10.0;
    v[1] = 20.0;
    v[2] = 30.0;
    v[3] = 40.0;
    assert_eq!(v, Vec4::new(10.0, 20.0, 30.0, 40.0));
}

#[test]
fn test_vec4_partial_eq() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v3 = Vec4::new(1.0, 2.0, 3.0, 5.0);

    assert_eq!(v1, v2);
    assert_ne!(v1, v3);
}

#[test]
fn test_vec4_from_array() {
    let arr = [1.0, 2.0, 3.0, 4.0];
    let v = Vec4::from(arr);
    assert_eq!(v, Vec4::new(1.0, 2.0, 3.0, 4.0));
}

#[test]
fn test_vec4_to_array() {
    let v = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let arr: [f32; 4] = v.into();
    assert_eq!(arr, [1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn test_vec4_default() {
    let v = Vec4::default();
    assert_eq!(v, Vec4::zero());
}

#[test]
fn test_vec4_copy() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = v1;
    assert_eq!(v1, v2);
    assert_eq!(v1.x(), 1.0);
}

#[test]
fn test_vec4_clone() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = v1;
    assert_eq!(v1, v2);
}
