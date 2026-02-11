use katla_math::*;

#[test]
fn test_pi() {
    assert!((PI - core::f32::consts::PI).abs() < 1e-5);
}

#[test]
fn test_tau() {
    assert!((TAU - 2.0 * PI).abs() < 1e-5);
}

#[test]
fn test_frac_pi_2() {
    assert!((FRAC_PI_2 - PI / 2.0).abs() < 1e-5);
}

#[test]
fn test_frac_pi_3() {
    assert!((FRAC_PI_3 - PI / 3.0).abs() < 1e-5);
}

#[test]
fn test_frac_pi_4() {
    assert!((FRAC_PI_4 - PI / 4.0).abs() < 1e-5);
}

#[test]
fn test_frac_pi_6() {
    assert!((FRAC_PI_6 - PI / 6.0).abs() < 1e-5);
}

#[test]
fn test_deg_to_rad() {
    assert!((deg_to_rad(180.0) - PI).abs() < 1e-5);
    assert!((deg_to_rad(90.0) - FRAC_PI_2).abs() < 1e-5);
    assert!((deg_to_rad(0.0) - 0.0).abs() < 1e-5);
}

#[test]
fn test_rad_to_deg() {
    assert!((rad_to_deg(PI) - 180.0).abs() < 1e-5);
    assert!((rad_to_deg(FRAC_PI_2) - 90.0).abs() < 1e-5);
    assert!((rad_to_deg(0.0) - 0.0).abs() < 1e-5);
}

#[test]
fn test_deg_to_rad_roundtrip() {
    let deg = 45.0;
    let rad = deg_to_rad(deg);
    let back = rad_to_deg(rad);
    assert!((deg - back).abs() < 1e-5);
}

#[test]
fn test_golden_ratio() {
    assert!((GOLDEN_RATIO - 1.618034).abs() < 1e-5);
}

#[test]
fn test_sqrt_3() {
    assert!((SQRT_3 - 1.7320508).abs() < 1e-5);
}
