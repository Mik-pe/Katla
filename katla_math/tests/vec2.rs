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
