//! Render graph integration tests.
//!
//! This test module verifies render graph error types and pass builder configuration.
//! Graph compilation and dependency analysis are tested in compiler.rs inline tests.

use katla_gfx::render_graph::GeometryPass;
use katla_gfx::texture::ImageFormat;

#[test]
fn test_render_graph_error_display() {
    let err = katla_gfx::RenderGraphError::ResourceNotFound("my_resource".to_string());
    assert!(err.to_string().contains("my_resource"));

    let err = katla_gfx::RenderGraphError::PassNotFound("my_pass".to_string());
    assert!(err.to_string().contains("my_pass"));

    let err = katla_gfx::RenderGraphError::AllocationFailed(1024);
    assert!(err.to_string().contains("1024"));

    let err = katla_gfx::RenderGraphError::DependencyCycle("A -> B -> A".to_string());
    assert!(err.to_string().contains("Cycle detected"));

    let err = katla_gfx::RenderGraphError::InvalidConfiguration("bad config".to_string());
    assert!(err.to_string().contains("bad config"));

    let err = katla_gfx::RenderGraphError::BackendError("device lost".to_string());
    assert!(err.to_string().contains("Backend error"));
}

#[test]
fn test_geometry_pass_chained_reads() {
    let pass = GeometryPass::new("geometry")
        .write_color("color", ImageFormat::R16G16B16A16Sfloat)
        .read("shadow_map")
        .read("environment_map")
        .read("previous_frame");

    assert_eq!(pass.reads().len(), 3);
    assert_eq!(pass.reads()[0], "shadow_map");
    assert_eq!(pass.reads()[1], "environment_map");
    assert_eq!(pass.reads()[2], "previous_frame");
    assert_eq!(pass.color_output_count(), 1);
}

#[test]
fn test_geometry_pass_multiple_color_outputs() {
    let pass = GeometryPass::new("geometry")
        .write_color("albedo", ImageFormat::R8G8B8A8Srgb)
        .write_color("normals", ImageFormat::R16G16B16A16Sfloat);

    assert_eq!(pass.color_output_count(), 2);
}
