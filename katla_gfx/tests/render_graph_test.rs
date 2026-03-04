//! Render graph integration tests.
//!
//! This test module demonstrates the render graph API and verifies
//! that all types work together correctly.

use katla_gfx::*;
use katla_gfx::texture::ImageFormat;

#[test]
fn test_render_graph_api_compilation() {
    // This test verifies that the render graph API compiles correctly.
    // Visual testing is done via `cargo run -- -s`.

    // Test that pass templates are accessible
    let _geometry = GeometryPass::new("geometry");
    let _fullscreen = FullscreenPass::new("fullscreen");
    let _shadow = ShadowPass::new("shadow");

    // Test that LightType is accessible
    let _directional = LightType::Directional;
    let _point = LightType::Point;
    let _spot = LightType::Spot;
}

#[test]
fn test_geometry_pass_builder() {
    // Test that GeometryPass builds correctly.
    let _pass = GeometryPass::new("test_geometry")
        .write_color("color", ImageFormat::R16G16B16A16Sfloat)
        .write_depth("depth", ImageFormat::D32Sfloat)
        .read("shadow_map");
}

#[test]
fn test_fullscreen_pass_builder() {
    // Test that FullscreenPass builds correctly.
    let _pass = FullscreenPass::new("test_fullscreen")
        .read("input_texture")
        .write("output", ImageFormat::R8G8B8A8Srgb);
}

#[test]
fn test_shadow_pass_builder() {
    // Test that ShadowPass builds correctly.
    let _pass = ShadowPass::new("test_shadows")
        .write_depth("shadow_map", ImageFormat::D32Sfloat)
        .resolution(2048, 2048)
        .light_type(LightType::Directional);
}

#[test]
fn test_pass_builder_types() {
    // Pass templates (GeometryPass, FullscreenPass, ShadowPass) are used
    // directly with FrameGraphBuilder. The PassBuilder trait is internal.
    // This test verifies that pass templates can be created and configured.

    let _g = GeometryPass::new("g").write_color("c", ImageFormat::R8G8B8A8Srgb);
    let _f = FullscreenPass::new("f").write("o", ImageFormat::R8G8B8A8Srgb);
    let _s = ShadowPass::new("s").light_type(LightType::Spot);
}

#[test]
fn test_render_graph_error_display() {
    // Test that RenderGraphError implements Display correctly.
    let err = RenderGraphError::ResourceNotFound("my_resource".to_string());
    assert!(err.to_string().contains("my_resource"));

    let err = RenderGraphError::PassNotFound("my_pass".to_string());
    assert!(err.to_string().contains("my_pass"));

    let err = RenderGraphError::AllocationFailed(1024);
    assert!(err.to_string().contains("1024"));
}

#[test]
fn test_light_type_equality() {
    // Test LightType comparisons.
    assert_eq!(LightType::Directional, LightType::Directional);
    assert_ne!(LightType::Directional, LightType::Point);
    assert_ne!(LightType::Point, LightType::Spot);
}
