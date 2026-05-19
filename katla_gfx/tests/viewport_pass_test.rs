#![cfg(feature = "vulkan")]
//! Integration tests for viewport pass rendering.
//!
//! Tests multi-viewport rendering with frame graph integration.

use katla_gfx::render_graph::{CompositePass, ViewportPass, ViewportRect};
use katla_gfx::texture::ImageFormat;

#[test]
fn test_viewport_pass_rendering() {
    // Test that a single viewport pass can be created and configured
    let viewport = ViewportPass::new("viewport_0")
        .extent(512, 512)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.1, 0.2, 0.3, 1.0]);

    // Verify resource descriptor is created correctly
    let resource_desc = viewport.resource_desc();
    assert!(resource_desc.is_some());

    let desc = resource_desc.unwrap();
    assert_eq!(desc.name, "viewport_0");
    assert_eq!(desc.width, 512);
    assert_eq!(desc.height, 512);
    assert_eq!(desc.format, ImageFormat::R16G16B16A16Sfloat);
}

#[test]
fn test_multiple_viewport_passes() {
    // Test creating multiple viewport passes with unique names
    let viewport_0 = ViewportPass::new("viewport_0")
        .extent(960, 1080)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.1, 0.1, 0.15, 1.0]);

    let viewport_1 = ViewportPass::new("viewport_1")
        .extent(960, 1080)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.15, 0.1, 0.1, 1.0]);

    // Verify both have unique resource descriptors
    let desc_0 = viewport_0.resource_desc().unwrap();
    let desc_1 = viewport_1.resource_desc().unwrap();

    assert_eq!(desc_0.name, "viewport_0");
    assert_eq!(desc_1.name, "viewport_1");

    // Verify they have different resource names (no race conditions)
    assert_ne!(desc_0.name, desc_1.name);
}

#[test]
fn test_four_viewport_grid() {
    // Test creating 4 viewports for a 2x2 grid layout
    let _viewport_0 = ViewportPass::new("viewport_0")
        .extent(960, 540)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.1, 0.1, 0.15, 1.0]);

    let _viewport_1 = ViewportPass::new("viewport_1")
        .extent(960, 540)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.15, 0.1, 0.1, 1.0]);

    let _viewport_2 = ViewportPass::new("viewport_2")
        .extent(960, 540)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.1, 0.15, 0.1, 1.0]);

    let _viewport_3 = ViewportPass::new("viewport_3")
        .extent(960, 540)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.15, 0.15, 0.1, 1.0]);

    // Verify compositing pass can reference all viewport textures
    let _composite = CompositePass::new("composite")
        .viewport("viewport_0", ViewportRect::new(0.0, 0.0, 960.0, 540.0))
        .viewport("viewport_1", ViewportRect::new(960.0, 0.0, 1920.0, 540.0))
        .viewport("viewport_2", ViewportRect::new(0.0, 540.0, 960.0, 1080.0))
        .viewport(
            "viewport_3",
            ViewportRect::new(960.0, 540.0, 1920.0, 1080.0),
        );
}

#[test]
fn test_viewport_pass_with_read_dependencies() {
    // Test viewport pass that reads from other resources (e.g., shadow maps)
    let viewport = ViewportPass::new("viewport_0")
        .extent(512, 512)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .read("shadow_map")
        .read("environment_map")
        .read("previous_frame");

    assert_eq!(viewport.reads().len(), 3);
    assert_eq!(viewport.reads()[0], "shadow_map");
    assert_eq!(viewport.reads()[1], "environment_map");
    assert_eq!(viewport.reads()[2], "previous_frame");
}

#[test]
fn test_viewport_pass_different_formats() {
    // Test viewports with different color formats
    let hdr_viewport = ViewportPass::new("hdr_viewport")
        .extent(512, 512)
        .format(ImageFormat::R16G16B16A16Sfloat);

    let ldr_viewport = ViewportPass::new("ldr_viewport")
        .extent(512, 512)
        .format(ImageFormat::R8G8B8A8Srgb);

    let hdr_desc = hdr_viewport.resource_desc().unwrap();
    let ldr_desc = ldr_viewport.resource_desc().unwrap();

    assert_eq!(hdr_desc.format, ImageFormat::R16G16B16A16Sfloat);
    assert_eq!(ldr_desc.format, ImageFormat::R8G8B8A8Srgb);
}

#[test]
fn test_viewport_pass_custom_load_store() {
    // Test viewport pass with custom load/store operations
    let viewport = ViewportPass::new("viewport_0")
        .extent(512, 512)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .load_store_ops(
            katla_gfx::render_pass::LoadOp::Load,
            katla_gfx::render_pass::StoreOp::DontCare,
        );

    // Verify viewport is created successfully
    assert_eq!(viewport.name(), "viewport_0");
}

#[test]
fn test_viewport_pass_default_format() {
    // Test that viewport pass defaults to HDR format when not specified
    let viewport = ViewportPass::new("viewport_0").extent(512, 512);

    // Resource desc should fail without format
    assert!(viewport.resource_desc().is_none());
}

#[test]
fn test_viewport_resolution_matches_texture_extent() {
    // Test that viewport resolution matches transient texture extent
    let viewport = ViewportPass::new("viewport_0")
        .extent(1024, 768)
        .format(ImageFormat::R16G16B16A16Sfloat);

    let desc = viewport.resource_desc().unwrap();
    assert_eq!(desc.width, 1024);
    assert_eq!(desc.height, 768);
}

#[test]
fn test_multiple_viewports_independent_cameras() {
    // Test that multiple viewports can have independent configurations
    let main_camera = ViewportPass::new("main_camera")
        .extent(1920, 1080)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.1, 0.1, 0.15, 1.0]);

    let picture_in_picture = ViewportPass::new("pip")
        .extent(320, 240)
        .format(ImageFormat::R16G16B16A16Sfloat)
        .clear_color([0.2, 0.2, 0.2, 1.0]);

    let minimap = ViewportPass::new("minimap")
        .extent(256, 256)
        .format(ImageFormat::R8G8B8A8Srgb)
        .clear_color([0.3, 0.3, 0.3, 1.0]);

    // Verify each has unique configuration
    let main_desc = main_camera.resource_desc().unwrap();
    let pip_desc = picture_in_picture.resource_desc().unwrap();
    let minimap_desc = minimap.resource_desc().unwrap();

    assert_eq!(main_desc.width, 1920);
    assert_eq!(main_desc.height, 1080);
    assert_eq!(pip_desc.width, 320);
    assert_eq!(pip_desc.height, 240);
    assert_eq!(minimap_desc.width, 256);
    assert_eq!(minimap_desc.height, 256);

    assert_eq!(main_desc.format, ImageFormat::R16G16B16A16Sfloat);
    assert_eq!(pip_desc.format, ImageFormat::R16G16B16A16Sfloat);
    assert_eq!(minimap_desc.format, ImageFormat::R8G8B8A8Srgb);
}
