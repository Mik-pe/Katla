use katla_math::{Color, HSV, Vec4};

#[test]
fn test_constructors() {
    // Test new constructor
    let c = Color::new(0.5, 0.5, 0.5, 1.0);
    assert_eq!(c.r, 0.5);
    assert_eq!(c.g, 0.5);
    assert_eq!(c.b, 0.5);
    assert_eq!(c.a, 1.0);

    // Test rgb constructor (alpha should be 1.0)
    let c = Color::rgb(1.0, 0.5, 0.0);
    assert_eq!(c.r, 1.0);
    assert_eq!(c.g, 0.5);
    assert_eq!(c.b, 0.0);
    assert_eq!(c.a, 1.0);
}

#[test]
fn test_from_u8() {
    let c = Color::from_u8(255, 128, 0);
    assert_eq!(c.r, 1.0);
    assert_eq!(c.g, 128.0 / 255.0);
    assert_eq!(c.b, 0.0);
    assert_eq!(c.a, 1.0);

    let c = Color::from_u8_rgba(255, 128, 64, 128);
    assert_eq!(c.r, 1.0);
    assert_eq!(c.g, 128.0 / 255.0);
    assert_eq!(c.b, 64.0 / 255.0);
    assert_eq!(c.a, 128.0 / 255.0);
}

#[test]
fn test_named_colors() {
    assert_eq!(Color::BLACK, Color::new(0.0, 0.0, 0.0, 1.0));
    assert_eq!(Color::WHITE, Color::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(Color::RED, Color::new(1.0, 0.0, 0.0, 1.0));
    assert_eq!(Color::GREEN, Color::new(0.0, 1.0, 0.0, 1.0));
    assert_eq!(Color::BLUE, Color::new(0.0, 0.0, 1.0, 1.0));
    assert_eq!(Color::YELLOW, Color::new(1.0, 1.0, 0.0, 1.0));
    assert_eq!(Color::CYAN, Color::new(0.0, 1.0, 1.0, 1.0));
    assert_eq!(Color::MAGENTA, Color::new(1.0, 0.0, 1.0, 1.0));
    assert_eq!(Color::TRANSPARENT, Color::new(0.0, 0.0, 0.0, 0.0));
}

#[test]
fn test_to_bytes() {
    let c = Color::new(1.0, 0.5, 0.0, 1.0);
    let bytes = c.to_bytes();
    assert_eq!(bytes[0], 255);
    assert_eq!(bytes[1], 128);
    assert_eq!(bytes[2], 0);
    assert_eq!(bytes[3], 255);

    // Test clamping
    let c = Color::new(1.5, -0.5, 0.5, 1.0);
    let bytes = c.to_bytes();
    assert_eq!(bytes[0], 255); // clamped to 1.0
    assert_eq!(bytes[1], 0); // clamped to 0.0
    assert_eq!(bytes[2], 128); // 0.5 -> 128
}

#[test]
fn test_to_array() {
    let c = Color::new(0.1, 0.2, 0.3, 0.4);
    let arr = c.to_array();
    assert_eq!(arr, [0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn test_to_clearcolor_value() {
    let c = Color::new(0.1, 0.2, 0.3, 0.4);
    let clear = c.to_clearcolor_value();
    assert_eq!(clear, [0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn test_indexing() {
    let c = Color::new(0.1, 0.2, 0.3, 0.4);
    assert_eq!(c[0], 0.1);
    assert_eq!(c[1], 0.2);
    assert_eq!(c[2], 0.3);
    assert_eq!(c[3], 0.4);
}

#[test]
#[should_panic(expected = "Index out of bounds")]
fn test_index_out_of_bounds() {
    let c = Color::WHITE;
    let _ = c[4];
}

#[test]
fn test_default() {
    let c = Color::default();
    assert_eq!(c, Color::WHITE);
}

#[test]
fn test_addition() {
    let c1 = Color::new(0.2, 0.3, 0.4, 0.5);
    let c2 = Color::new(0.1, 0.2, 0.3, 0.4);
    let result = c1 + c2;
    assert!((result.r - 0.3).abs() < 0.0001);
    assert!((result.g - 0.5).abs() < 0.0001);
    assert!((result.b - 0.7).abs() < 0.0001);
    assert!((result.a - 0.9).abs() < 0.0001);
}

#[test]
fn test_subtraction() {
    let c1 = Color::new(0.5, 0.6, 0.7, 0.8);
    let c2 = Color::new(0.1, 0.2, 0.3, 0.4);
    let result = c1 - c2;
    assert!((result.r - 0.4).abs() < 0.0001);
    assert!((result.g - 0.4).abs() < 0.0001);
    assert!((result.b - 0.4).abs() < 0.0001);
    assert!((result.a - 0.4).abs() < 0.0001);
}

#[test]
fn test_multiplication_scalar() {
    let c = Color::new(0.5, 0.5, 0.5, 1.0);
    let result = c * 2.0;
    assert_eq!(result, Color::new(1.0, 1.0, 1.0, 2.0));
}

#[test]
fn test_multiplication_component() {
    let c1 = Color::new(0.5, 0.5, 0.5, 1.0);
    let c2 = Color::new(0.2, 0.4, 0.6, 0.8);
    let result = c1 * c2;
    assert_eq!(result, Color::new(0.1, 0.2, 0.3, 0.8));
}

#[test]
fn test_add_assign() {
    let mut c = Color::new(0.2, 0.3, 0.4, 0.5);
    c += Color::new(0.1, 0.2, 0.3, 0.4);
    assert!((c.r - 0.3).abs() < 0.0001);
    assert!((c.g - 0.5).abs() < 0.0001);
    assert!((c.b - 0.7).abs() < 0.0001);
    assert!((c.a - 0.9).abs() < 0.0001);
}

#[test]
fn test_sub_assign() {
    let mut c = Color::new(0.5, 0.6, 0.7, 0.8);
    c -= Color::new(0.1, 0.2, 0.3, 0.4);
    assert!((c.r - 0.4).abs() < 0.0001);
    assert!((c.g - 0.4).abs() < 0.0001);
    assert!((c.b - 0.4).abs() < 0.0001);
    assert!((c.a - 0.4).abs() < 0.0001);
}

#[test]
fn test_mul_assign_scalar() {
    let mut c = Color::new(0.5, 0.5, 0.5, 1.0);
    c *= 2.0;
    assert_eq!(c, Color::new(1.0, 1.0, 1.0, 2.0));
}

#[test]
fn test_lerp() {
    let c1 = Color::RED;
    let c2 = Color::BLUE;

    // t = 0 should return c1
    let result = Color::lerp(c1, c2, 0.0);
    assert_eq!(result, c1);

    // t = 1 should return c2
    let result = Color::lerp(c1, c2, 1.0);
    assert_eq!(result, c2);

    // t = 0.5 should be midpoint
    let result = Color::lerp(c1, c2, 0.5);
    assert_eq!(result, Color::new(0.5, 0.0, 0.5, 1.0));
}

#[test]
fn test_brightness() {
    let c = Color::new(0.5, 0.5, 0.5, 0.5);
    let brighter = c.brightness(2.0);
    assert_eq!(brighter, Color::new(1.0, 1.0, 1.0, 0.5));

    let darker = c.brightness(0.5);
    assert_eq!(darker, Color::new(0.25, 0.25, 0.25, 0.5));
}

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
    assert!((hsv.h - hsv_back.h).abs() < 1.0); // Allow small error due to conversion
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
    assert!(linear.r < 0.5); // Linear should be darker
    assert!(linear.g < 0.5);
    assert!(linear.b < 0.5);

    // sRGB should be lighter (inverse gamma)
    let linear = Color::new(0.2, 0.2, 0.2, 1.0);
    let srgb = linear.to_srgb();
    assert!(srgb.r > 0.2); // sRGB should be lighter
    assert!(srgb.g > 0.2);
    assert!(srgb.b > 0.2);
}

#[test]
fn test_hex_parsing() {
    // Test RGB hex
    let red = Color::from_rgb_hex(0xFF0000);
    assert_eq!(red, Color::RED);

    let green = Color::from_rgb_hex(0x00FF00);
    assert_eq!(green, Color::GREEN);

    let blue = Color::from_rgb_hex(0x0000FF);
    assert_eq!(blue, Color::BLUE);

    // Test RGBA hex
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

    // Color -> Vec4
    let v = c.to_vec4();
    assert_eq!(v[0], 0.1);
    assert_eq!(v[1], 0.2);
    assert_eq!(v[2], 0.3);
    assert_eq!(v[3], 0.4);

    // Vec4 -> Color
    let v = Vec4([0.5, 0.6, 0.7, 0.8]);
    let c = Color::from_vec4(v);
    assert_eq!(c.r, 0.5);
    assert_eq!(c.g, 0.6);
    assert_eq!(c.b, 0.7);
    assert_eq!(c.a, 0.8);

    // Test From trait
    let c = Color::new(0.1, 0.2, 0.3, 0.4);
    let v: Vec4 = c.into();
    assert_eq!(v[0], 0.1);

    let v = Vec4([0.5, 0.6, 0.7, 0.8]);
    let c: Color = v.into();
    assert_eq!(c.r, 0.5);
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
fn test_checkerboard_pattern() {
    // This simulates the usage in material_helpers.rs
    let white_pixel = Color::WHITE.to_bytes();
    assert_eq!(white_pixel, [255, 255, 255, 255]);

    let black_pixel = Color::BLACK.to_bytes();
    assert_eq!(black_pixel, [0, 0, 0, 255]);
}
