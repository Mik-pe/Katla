use katla_math::{Color, HSV, Vec4};

#[test]
fn test_saturate() {
    let gray = Color::new(0.5, 0.5, 0.5, 1.0);
    // Saturating gray should produce gray (no color to saturate)
    let result = gray.saturate(1.0);
    assert_eq!(result, gray);

    // Desaturating any color should produce grayscale
    let red = Color::RED;
    let grayscale = red.saturate(0.0);
    // Red luminance is ~0.299
    assert!((grayscale.r - 0.299).abs() < 0.001);
    assert!((grayscale.g - 0.299).abs() < 0.001);
    assert!((grayscale.b - 0.299).abs() < 0.001);
}

#[test]
fn test_hsv_conversions() {
    // Test RED -> HSV -> RED
    let red = Color::RED;
    let hsv = red.to_hsv();
    assert!((hsv.h - 0.0).abs() < 0.001);
    assert!((hsv.s - 1.0).abs() < 0.001);
    assert!((hsv.v - 1.0).abs() < 0.001);

    let back = Color::from_hsv(hsv);
    assert!((back.r - red.r).abs() < 0.001);
    assert!((back.g - red.g).abs() < 0.001);
    assert!((back.b - red.b).abs() < 0.001);

    // Test GREEN -> HSV -> GREEN
    let green = Color::GREEN;
    let hsv = green.to_hsv();
    assert!((hsv.h - 120.0).abs() < 0.001);
    assert!((hsv.s - 1.0).abs() < 0.001);
    assert!((hsv.v - 1.0).abs() < 0.001);

    let back = Color::from_hsv(hsv);
    assert!((back.r - green.r).abs() < 0.001);
    assert!((back.g - green.g).abs() < 0.001);
    assert!((back.b - green.b).abs() < 0.001);

    // Test BLUE -> HSV -> BLUE
    let blue = Color::BLUE;
    let hsv = blue.to_hsv();
    assert!((hsv.h - 240.0).abs() < 0.001);
    assert!((hsv.s - 1.0).abs() < 0.001);
    assert!((hsv.v - 1.0).abs() < 0.001);

    let back = Color::from_hsv(hsv);
    assert!((back.r - blue.r).abs() < 0.001);
    assert!((back.g - blue.g).abs() < 0.001);
    assert!((back.b - blue.b).abs() < 0.001);

    // Test custom HSV color
    let hsv = HSV::new(180.0, 0.8, 0.9);
    let color = Color::from_hsv(hsv);
    let hsv_back = color.to_hsv();
    assert!((hsv.h - hsv_back.h).abs() < 1.0);
    assert!((hsv.s - hsv_back.s).abs() < 0.01);
    assert!((hsv.v - hsv_back.v).abs() < 0.01);
}

#[test]
fn test_gamma_correction() {
    // Test sRGB -> linear -> sRGB roundtrip
    let original = Color::new(0.5, 0.7, 0.9, 1.0);
    let linear = original.to_linear();
    let back = linear.to_srgb();

    assert!((back.r - original.r).abs() < 0.001);
    assert!((back.g - original.g).abs() < 0.001);
    assert!((back.b - original.b).abs() < 0.001);

    // Linear should be darker for midtones (gamma correction)
    let gray = Color::new(0.5, 0.5, 0.5, 1.0);
    let linear = gray.to_linear();
    assert!(linear.r < 0.5);
    assert!(linear.g < 0.5);
    assert!(linear.b < 0.5);

    // sRGB should be lighter (inverse gamma)
    let linear = Color::new(0.2, 0.2, 0.2, 1.0);
    let srgb = linear.to_srgb();
    assert!(srgb.r > 0.2);
    assert!(srgb.g > 0.2);
    assert!(srgb.b > 0.2);
}

#[test]
fn test_edge_cases() {
    // NaN handling - clamped should handle NaN
    let c = Color::new(f32::NAN, 0.5, 0.5, 1.0);
    let _bytes = c.to_bytes();
    // NaN clamping behavior is implementation-defined, just ensure it doesn't crash

    // Infinity handling
    let c = Color::new(f32::INFINITY, 0.5, 0.5, 1.0);
    assert!(!c.is_valid());
    let clamped = c.clamped();
    assert!(clamped.is_valid());

    // Negative infinity
    let c = Color::new(f32::NEG_INFINITY, 0.5, 0.5, 1.0);
    assert!(!c.is_valid());
    let clamped = c.clamped();
    assert!(clamped.is_valid());

    // Zero values
    let c = Color::new(0.0, 0.0, 0.0, 0.0);
    assert!(c.is_valid());
    let bytes = c.to_bytes();
    assert_eq!(bytes, [0, 0, 0, 0]);
}

#[test]
fn test_clamping() {
    let c = Color::new(-0.5, 1.5, 0.5, 2.0);
    let clamped = c.clamped();
    assert_eq!(clamped, Color::new(0.0, 1.0, 0.5, 1.0));
}

#[test]
fn test_is_valid() {
    let valid = Color::new(0.5, 0.5, 0.5, 1.0);
    assert!(valid.is_valid());

    let invalid_low = Color::new(-0.1, 0.5, 0.5, 1.0);
    assert!(!invalid_low.is_valid());

    let invalid_high = Color::new(0.5, 1.5, 0.5, 1.0);
    assert!(!invalid_high.is_valid());

    let invalid_alpha = Color::new(0.5, 0.5, 0.5, 1.5);
    assert!(!invalid_alpha.is_valid());

    let all_valid = Color::new(0.0, 1.0, 0.5, 1.0);
    assert!(all_valid.is_valid());
}

#[test]
fn test_to_bytes_clamping() {
    let c = Color::new(1.5, -0.5, 0.5, 1.0);
    let bytes = c.to_bytes();
    assert_eq!(bytes[0], 255); // clamped to 1.0
    assert_eq!(bytes[1], 0); // clamped to 0.0
    assert_eq!(bytes[2], 128); // 0.5 -> 128
}

#[test]
fn test_hex_parsing() {
    let red = Color::from_rgb_hex(0xFF0000);
    assert_eq!(red, Color::RED);

    let green = Color::from_rgb_hex(0x00FF00);
    assert_eq!(green, Color::GREEN);

    let blue = Color::from_rgb_hex(0x0000FF);
    assert_eq!(blue, Color::BLUE);

    let red_alpha = Color::from_rgba_hex(0xFF0000FF);
    assert_eq!(red_alpha, Color::RED);

    let transparent_red = Color::from_rgba_hex(0xFF000080);
    assert_eq!(transparent_red.r, 1.0);
    assert_eq!(transparent_red.g, 0.0);
    assert_eq!(transparent_red.b, 0.0);
    assert_eq!(transparent_red.a, 0x80 as f32 / 255.0);
}

#[test]
fn test_vec4_conversion() {
    let c = Color::new(0.1, 0.2, 0.3, 0.4);

    // Color -> Vec4 via From trait
    let v: Vec4 = c.into();
    assert_eq!(v[0], 0.1);
    assert_eq!(v[1], 0.2);
    assert_eq!(v[2], 0.3);
    assert_eq!(v[3], 0.4);

    // Vec4 -> Color via From trait
    let v = Vec4::new(0.5, 0.6, 0.7, 0.8);
    let c: Color = v.into();
    assert_eq!(c.r, 0.5);
    assert_eq!(c.g, 0.6);
    assert_eq!(c.b, 0.7);
    assert_eq!(c.a, 0.8);
}
