use katla_math::*;

#[test]
fn test_saturating_mul() {
    assert_eq!(saturating_mul(1e20, 1e20), f32::MAX);
    assert_eq!(saturating_mul(5.0, 3.0), 15.0);
}

#[test]
fn test_safe_div() {
    assert_eq!(safe_div(10.0, 2.0), 5.0);
    assert_eq!(safe_div(10.0, 0.0), 0.0);
    assert_eq!(safe_div(10.0, 0.0001), 100000.0);
}

#[test]
fn test_reciprocal() {
    assert_eq!(reciprocal(2.0), 0.5);
    assert_eq!(reciprocal(4.0), 0.25);
    assert_eq!(reciprocal(0.0), 0.0);
}

#[test]
fn test_safe_sqrt() {
    assert_eq!(safe_sqrt(4.0), 2.0);
    assert_eq!(safe_sqrt(0.0), 0.0);
    assert_eq!(safe_sqrt(-4.0), 0.0);
}

#[test]
fn test_clamp() {
    assert_eq!(clamp(5.0, 0.0, 10.0), 5.0);
    assert_eq!(clamp(-5.0, 0.0, 10.0), 0.0);
    assert_eq!(clamp(15.0, 0.0, 10.0), 10.0);
    // min > max case: bounds are swapped, so it's clamp(5.0, 0.0, 10.0) = 5.0
    assert_eq!(clamp(5.0, 10.0, 0.0), 5.0);
    assert_eq!(clamp(-5.0, 10.0, 0.0), 0.0);
    assert_eq!(clamp(15.0, 10.0, 0.0), 10.0);
}

#[test]
fn test_lerp() {
    assert_eq!(lerp(0.0, 10.0, 0.0), 0.0);
    assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
    assert_eq!(lerp(0.0, 10.0, 1.0), 10.0);
    assert_eq!(lerp(5.0, 15.0, 0.5), 10.0);
}

#[test]
fn test_map_range() {
    assert_eq!(map_range(0.5, 0.0, 1.0, 0.0, 100.0), 50.0);
    assert_eq!(map_range(0.0, 0.0, 1.0, 0.0, 10.0), 0.0);
    assert_eq!(map_range(1.0, 0.0, 1.0, 0.0, 10.0), 10.0);
    assert_eq!(map_range(2.0, 0.0, 4.0, 0.0, 100.0), 50.0);
}

#[test]
fn test_remap_clamp() {
    assert_eq!(remap_clamp(1.5, 0.0, 1.0, 0.0, 100.0), 100.0);
    assert_eq!(remap_clamp(-0.5, 0.0, 1.0, 0.0, 100.0), 0.0);
    assert_eq!(remap_clamp(0.5, 0.0, 1.0, 0.0, 100.0), 50.0);
}

#[test]
fn test_smoothstep() {
    assert_eq!(smoothstep(0.0, 1.0, 0.0), 0.0);
    assert_eq!(smoothstep(0.0, 1.0, 0.5), 0.5);
    assert_eq!(smoothstep(0.0, 1.0, 1.0), 1.0);
    assert_eq!(smoothstep(0.0, 1.0, -0.5), 0.0);
    assert_eq!(smoothstep(0.0, 1.0, 1.5), 1.0);
}

#[test]
fn test_smootherstep() {
    assert_eq!(smootherstep(0.0, 1.0, 0.0), 0.0);
    assert_eq!(smootherstep(0.0, 1.0, 0.5), 0.5);
    assert_eq!(smootherstep(0.0, 1.0, 1.0), 1.0);
    assert_eq!(smootherstep(0.0, 1.0, -0.5), 0.0);
    assert_eq!(smootherstep(0.0, 1.0, 1.5), 1.0);
}

#[test]
fn test_inverse_smoothstep() {
    assert_eq!(inverse_smoothstep(0.0, 1.0, 0.0), 0.0);
    assert_eq!(inverse_smoothstep(0.0, 1.0, 0.5), 0.5);
    assert_eq!(inverse_smoothstep(0.0, 1.0, 1.0), 1.0);
}

#[test]
fn test_approx_zero() {
    assert!(approx_zero(0.0));
    assert!(approx_zero_eps(0.00001, 0.0001));
    assert!(!approx_zero(1.0));
    assert!(!approx_zero(-0.1));
    assert!(approx_zero_eps(0.001, 0.01));
}

#[test]
fn test_approx_equal() {
    assert!(approx_equal(1.0, 1.0));
    assert!(approx_equal_eps(1.0, 1.00001, 0.0001));
    assert!(!approx_equal(1.0, 1.1));
    assert!(approx_equal_eps(1.0, 1.001, 0.01));
}

#[test]
fn test_next_power_of_two() {
    assert_eq!(next_power_of_two(5.0), 8.0);
    assert_eq!(next_power_of_two(16.0), 16.0);
    assert_eq!(next_power_of_two(17.0), 32.0);
    assert_eq!(next_power_of_two(0.5), 1.0);
    assert_eq!(next_power_of_two(-5.0), 1.0);
}

#[test]
fn test_prev_power_of_two() {
    assert_eq!(prev_power_of_two(5.0), 4.0);
    assert_eq!(prev_power_of_two(16.0), 16.0);
    assert_eq!(prev_power_of_two(17.0), 16.0);
    assert_eq!(prev_power_of_two(0.5), 1.0);
}

#[test]
fn test_is_power_of_two() {
    assert!(is_power_of_two(1.0));
    assert!(is_power_of_two(2.0));
    assert!(is_power_of_two(4.0));
    assert!(is_power_of_two(16.0));
    assert!(!is_power_of_two(3.0));
    assert!(!is_power_of_two(5.0));
    assert!(!is_power_of_two(0.0));
    assert!(!is_power_of_two(-4.0));
}

#[test]
fn test_fast_inverse_sqrt() {
    let x = 4.0;
    let result = fast_inverse_sqrt(x);
    let expected = 1.0 / x.sqrt();
    assert!((result - expected).abs() < 0.001);
}

#[test]
fn test_fast_sqrt() {
    assert!((fast_sqrt(4.0) - 2.0).abs() < 0.01);
    assert_eq!(fast_sqrt(0.0), 0.0);
    assert_eq!(fast_sqrt(-4.0), 0.0);
}

#[test]
fn test_saturating_add() {
    assert_eq!(saturating_add(5.0, 3.0), 8.0);
    assert!((saturating_add(1e38, 1e38) - 2e38).abs() < 1e37);
}

#[test]
fn test_saturating_sub() {
    assert_eq!(saturating_sub(5.0, 3.0), 2.0);
    assert!((saturating_sub(-1e38, 1e38) - (-2e38)).abs() < 1e37);
}

#[test]
fn test_mod_f32() {
    assert_eq!(mod_f32(10.0, 3.0), 1.0);
    assert_eq!(mod_f32(-10.0, 3.0), -1.0);
}
