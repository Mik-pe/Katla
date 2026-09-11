//! Backend-neutral public-API contract test for issue #92.
//!
//! The frame exercised here — descriptor-built material, graph passes,
//! submission closure — uses only portable crate-root imports. No
//! `vulkan_native` escape hatch, no backend-internal module path, and no
//! backend-specific material options appear in this file. The only backend
//! commitment is the renderer type parameter, which is the documented
//! compile-time selection point (`AnyRenderer` is the runtime alternative).
//!
//! Needs a Vulkan device (`#[ignore]`, run like the other GPU contract
//! suites:
//! `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test backend_neutral_api -- --ignored`).

use std::ffi::CString;

use katla_gfx::render_graph::{FrameGraph, FrameGraphBuilder, GeometryPass};
use katla_gfx::texture::ImageFormat;
use katla_gfx::{
    CullMode, DepthState, FrameUniforms, GpuRenderer, PipelineDescriptor, ValidationMode,
    VulkanRenderer,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        ValidationMode::Disabled,
        CString::new("Backend-neutral API test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn shaders() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders")
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_representative_frame_compiles_and_renders_portably() {
    let mut renderer = headless_renderer();
    let shaders = shaders();

    renderer
        .init_light_culling(WIDTH, HEIGHT, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    // PBR pipelines declare a descriptor layout for shadow data; it must
    // exist before the material is compiled (same setup as the other
    // device suites).
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();

    // Portable material state: PBR layout, HDR target, unculled, depth off.
    let descriptor = PipelineDescriptor::pbr(shaders.join("model_pbr.wgsl").to_string_lossy())
        .with_color_format(ImageFormat::R16G16B16A16Sfloat)
        .with_depth(DepthState::disabled())
        .with_cull(CullMode::None);
    let material = renderer.compile_material(&descriptor).unwrap();

    let mut graph: FrameGraph<VulkanRenderer> = FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("geometry")
                .write_color("backbuffer", ImageFormat::R16G16B16A16Sfloat)
                .clear_color([0.0, 0.0, 0.0, 1.0])
                .material(material),
        )
        .build::<VulkanRenderer>()
        .unwrap();

    renderer.wait_for_frame().unwrap();
    renderer.set_frame_uniforms(FrameUniforms::default());
    renderer
        .render(&mut graph, |_| {})
        .expect("backend-neutral frame renders without backend-module imports");
}
