use katla_math::Vec4;

#[test]
fn test_vec4_lerp() {
    let v1 = Vec4::new(0.0, 0.0, 0.0, 0.0);
    let v2 = Vec4::new(10.0, 10.0, 10.0, 10.0);

    let result = v1.lerp(v2, 0.5);
    assert_eq!(result, Vec4::new(5.0, 5.0, 5.0, 5.0));

    let result2 = v1.lerp(v2, 0.0);
    assert_eq!(result2, v1);

    let result3 = v1.lerp(v2, 1.0);
    assert_eq!(result3, v2);
}

#[test]
fn test_vec4_dot_commutative() {
    let v1 = Vec4::new(1.0, 2.0, 3.0, 4.0);
    let v2 = Vec4::new(5.0, 6.0, 7.0, 8.0);
    assert_eq!(v1.dot(v2), v2.dot(v1));
}
