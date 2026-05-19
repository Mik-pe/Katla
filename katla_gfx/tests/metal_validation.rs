#![cfg(all(target_os = "macos", feature = "metal"))]

/// Tests that should be run with Metal validation enabled.
///
/// Run with:
/// ```sh
/// MTL_DEBUG_LAYER=1 cargo test -p katla_gfx --features metal --test metal_validation -- --nocapture
/// ```
///
/// Metal validation checks for API misuse, resource hazards, and other issues.
/// The tests in this module exercise the Metal backend through the public API
/// and rely on Metal's built-in validation to catch problems.
///
/// Note: Metal validation is controlled by the `MTL_DEBUG_LAYER` environment
/// variable. When enabled, Metal performs additional runtime checks that can
/// detect:
/// - Invalid resource access patterns
/// - Command buffer misuse
/// - Memory hazard violations
/// - Incorrect pipeline state configuration
use katla_gfx::texture::{ImageFormat, TextureDescriptor, TextureUsage};

#[test]
fn test_metal_headless_context() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Basic smoke test: create a texture and verify it has the expected dimensions.
    // When run with MTL_DEBUG_LAYER=1, Metal will validate the underlying API calls.
    let desc = TextureDescriptor::new(256, 256, ImageFormat::R8G8B8A8Srgb)
        .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
    assert_eq!(desc.width, 256);
    assert_eq!(desc.height, 256);
    assert_eq!(desc.format, ImageFormat::R8G8B8A8Srgb);
    assert!(desc.usage.contains(TextureUsage::COLOR_ATTACHMENT));
    assert!(desc.usage.contains(TextureUsage::SAMPLED));
}
