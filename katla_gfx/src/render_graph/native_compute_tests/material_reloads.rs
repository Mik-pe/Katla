//! Native last-good replacement, source identity, interface and submission checks.

use super::*;
use crate::{
    ConstantBinding, CullMode, DepthState, ImageFormat, PassBindings, PassDraw, PassDrawPhase,
    PassPipeline, PipelineDescriptor, ShaderStages, TextureReadbackRegion, VertexLayout,
};

const MAIN: &str = r#"
#include "shared.wgsl"
@group(2) @binding(0) var<uniform> tint:vec4f;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
    return vec4f(p[i],0.,1.);
}
@fragment fn fs_main()->@location(0) vec4f { return tint*COLOR; }
"#;

fn native_variants(renderer: &NativeRenderer, material: crate::MaterialHandle) -> Vec<usize> {
    #[cfg(not(target_os = "macos"))]
    let mut values: Vec<_> = renderer
        .asset_registry
        .get_material(material)
        .unwrap()
        .variants
        .values()
        .flat_map(|variant| [Some(variant.pipeline), variant.instanced_pipeline])
        .flatten()
        .map(|handle| {
            use ash::vk::Handle;
            renderer
                .asset_registry
                .get_pipeline(handle)
                .unwrap()
                .vk_pipeline()
                .as_raw() as usize
        })
        .collect();
    #[cfg(target_os = "macos")]
    let mut values: Vec<_> = renderer
        .materials
        .get(material)
        .unwrap()
        .variants
        .values()
        .map(|pipeline| {
            (&*pipeline.pipeline_state
                as *const objc2::runtime::ProtocolObject<dyn objc2_metal::MTLRenderPipelineState>)
                .cast::<()>() as usize
        })
        .collect();
    values.sort_unstable();
    values
}

fn finish_reload(renderer: &mut NativeRenderer) {
    #[cfg(target_os = "macos")]
    {
        let start = std::time::Instant::now();
        while renderer
            .materials
            .iter()
            .any(|material| material.pending_reload.is_some())
        {
            renderer.poll_material_reloads_impl();
            assert!(start.elapsed().as_secs() < 30, "reload timed out");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = renderer;
}

fn descriptor(path: &std::path::Path) -> PipelineDescriptor {
    PipelineDescriptor::simple(path.to_string_lossy())
        .with_vertex_layout(VertexLayout::empty())
        .with_color_format(ImageFormat::B8G8R8A8Srgb)
        .with_depth(DepthState::disabled())
        .with_depth_format(None)
        .with_cull(CullMode::None)
}

fn draw(
    renderer: &mut NativeRenderer,
    graph: &mut FrameGraph<NativeRenderer>,
    material: crate::MaterialHandle,
    binding: u32,
) -> crate::renderer::texture_readback::TextureReadbackTicket {
    let pass = graph.pass_id("material").unwrap();
    graph
        .set_pass_bindings(
            pass,
            PassBindings {
                constants: vec![ConstantBinding {
                    group: 2,
                    binding,
                    stages: ShaderStages::FRAGMENT,
                    bytes: [1.0f32; 4].into_iter().flat_map(f32::to_ne_bytes).collect(),
                }],
                phases: vec![PassDrawPhase {
                    pipelines: vec![PassPipeline {
                        material,
                        vertex_layout: VertexLayout::empty(),
                    }],
                    constants: vec![],
                    draw: PassDraw::Vertices {
                        count: 3,
                        instances: 1,
                    },
                    viewport: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();
    let frame = acquire(renderer);
    renderer.render(&frame, graph, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    let source = renderer
        .graph_texture_source(graph.resource_id("backbuffer").unwrap())
        .unwrap();
    renderer
        .queue_texture_readback(source, TextureReadbackRegion::pixel(8, 8))
        .unwrap()
}

fn pixel(
    renderer: &mut NativeRenderer,
    ticket: crate::renderer::texture_readback::TextureReadbackTicket,
) -> Vec<u8> {
    renderer.wait_for_device();
    renderer
        .poll_texture_readback(ticket)
        .unwrap()
        .unwrap()
        .bytes
}

#[test]
fn test_native_material_reload_keeps_last_good_variants_and_canonical_dependencies() {
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    let root = std::env::temp_dir().join(format!(
        "katla-native-material-reload-{}",
        crate::renderer::texture_readback::fresh_readback_id()
    ));
    std::fs::create_dir_all(root.join("other")).unwrap();
    let path = root.join("model.wgsl");
    let shared = root.join("shared.wgsl");
    std::fs::write(&path, MAIN).unwrap();
    std::fs::write(&shared, "const COLOR=vec4f(1.,0.,0.,1.);").unwrap();
    std::fs::write(root.join("other/model.wgsl"), MAIN).unwrap();
    std::fs::write(
        root.join("other/shared.wgsl"),
        "const COLOR=vec4f(0.,0.,1.,1.);",
    )
    .unwrap();
    let material = renderer.compile_material(&descriptor(&path)).unwrap();
    let unrelated = renderer
        .compile_material(&descriptor(&root.join("other/model.wgsl")))
        .unwrap();
    #[cfg(not(target_os = "macos"))]
    renderer
        .ensure_material_compiled(material, ImageFormat::R8G8B8A8Unorm)
        .unwrap();
    let initial = native_variants(&renderer, material);
    let other_initial = native_variants(&renderer, unrelated);
    assert!(initial.len() >= 2);
    let mut graph = FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("material")
                .without_depth()
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb),
        )
        .export_resource("backbuffer")
        .build::<NativeRenderer>()
        .unwrap();
    let queued_red = draw(&mut renderer, &mut graph, material, 0);
    std::fs::write(&shared, "invalid WGSL").unwrap();
    assert_eq!(renderer.recompile_materials_for_shader(&shared), 1);
    finish_reload(&mut renderer);
    assert_eq!(native_variants(&renderer, material), initial);
    assert_eq!(native_variants(&renderer, unrelated), other_initial);
    assert_eq!(pixel(&mut renderer, queued_red), [0, 0, 255, 255]);
    let red = draw(&mut renderer, &mut graph, material, 0);
    assert_eq!(pixel(&mut renderer, red), [0, 0, 255, 255]);
    let in_flight_red = draw(&mut renderer, &mut graph, material, 0);
    std::fs::write(&shared, "const COLOR=vec4f(0.,1.,0.,1.);").unwrap();
    assert_eq!(renderer.recompile_materials_for_shader(&shared), 1);
    finish_reload(&mut renderer);
    assert_eq!(pixel(&mut renderer, in_flight_red), [0, 0, 255, 255]);
    let green_variants = native_variants(&renderer, material);
    assert_eq!(green_variants.len(), initial.len());
    assert!(
        green_variants
            .iter()
            .all(|pipeline| !initial.contains(pipeline))
    );
    assert_eq!(native_variants(&renderer, unrelated), other_initial);
    let green = draw(&mut renderer, &mut graph, material, 0);
    assert_eq!(pixel(&mut renderer, green), [0, 255, 0, 255]);
    std::fs::write(&path, MAIN.replace("@binding(0)", "@binding(3)")).unwrap();
    assert_eq!(renderer.recompile_materials_for_shader(&path), 1);
    finish_reload(&mut renderer);
    let rebound = draw(&mut renderer, &mut graph, material, 3);
    assert_eq!(pixel(&mut renderer, rebound), [0, 255, 0, 255]);
    assert_eq!(native_variants(&renderer, unrelated), other_initial);
    std::fs::write(&shared, "const COLOR=vec4f(0.,0.,1.,1.);").unwrap();
    assert_eq!(renderer.recompile_materials_for_shader(&shared), 1);
    std::fs::write(&shared, "const COLOR=vec4f(1.,1.,0.,1.);").unwrap();
    assert_eq!(renderer.recompile_materials_for_shader(&shared), 1);
    finish_reload(&mut renderer);
    let latest = draw(&mut renderer, &mut graph, material, 3);
    assert_eq!(pixel(&mut renderer, latest), [0, 255, 255, 255]);
    assert_eq!(native_variants(&renderer, unrelated), other_initial);
    graph.cleanup();
    renderer.destroy();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}

#[test]
fn test_native_material_reload_replaces_ui_instanced_pipeline_atomically() {
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    let original = crate::renderer::shader_source::ShaderSource::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders/ui/ui.wgsl"),
    )
    .unwrap()
    .code;
    let path = std::env::temp_dir().join(format!(
        "katla-native-ui-reload-{}.wgsl",
        crate::renderer::texture_readback::fresh_readback_id()
    ));
    std::fs::write(&path, &original).unwrap();
    let descriptor =
        PipelineDescriptor::ui(path.to_string_lossy()).with_color_format(ImageFormat::B8G8R8A8Srgb);
    let material = renderer.compile_material(&descriptor).unwrap();
    let initial = native_variants(&renderer, material);
    assert!(
        initial.len() >= 2,
        "plain and instanced UI pipelines must exist"
    );
    std::fs::write(
        &path,
        original.replace("fn vs_instanced(", "fn vs_missing("),
    )
    .unwrap();
    assert_eq!(renderer.recompile_materials_for_shader(&path), 1);
    finish_reload(&mut renderer);
    assert_eq!(
        native_variants(&renderer, material),
        initial,
        "failure preparing instanced entry must retain all old variants"
    );
    std::fs::write(&path, &original).unwrap();
    assert_eq!(renderer.recompile_materials_for_shader(&path), 1);
    finish_reload(&mut renderer);
    let replacement = native_variants(&renderer, material);
    assert_eq!(replacement.len(), initial.len());
    assert!(
        replacement
            .iter()
            .all(|pipeline| !initial.contains(pipeline))
    );
    renderer.destroy();
    std::fs::remove_file(path).unwrap();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}
