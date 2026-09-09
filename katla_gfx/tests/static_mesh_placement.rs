//! Static mesh staging and memory placement for issue #96.
//!
//! Static meshes upload through one batched staged submission into
//! GPU-optimal memory (host-visible fallback when the device-local
//! allocation fails), dynamic meshes stay host-visible, and the selected
//! placement is observable through `mesh_memory_report`. Staged uploads
//! complete before any later draw can read them; their staging memory and
//! fences release at frame boundaries.
//!
//! All tests need a Vulkan device (`#[ignore]`, like the other GPU
//! contract tests).

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use katla_gfx::render_graph::{FrameGraph, FrameGraphBuilder, GeometryPass};
use katla_gfx::renderer::{DrawCall, DrawList};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::{Vertex, VertexPBR};
use katla_gfx::{
    FrameUniforms, IndexType, MaterialHandle, MaterialOptions, MeshDescriptor, MeshHandle,
    MeshMemoryClass, MeshUsage, PrimitiveTopology, ValidationMode, VertexType, VulkanRenderer,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        // ValidationMode::Disabled: compiling the PBR pipeline under the
        // system validation layer segfaults the Intel driver on this machine
        // (same caveat as the other GPU contract tests).
        ValidationMode::Disabled,
        CString::new("Static mesh placement test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn identity() -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

fn vertex(position: [f32; 3]) -> VertexPBR {
    VertexPBR {
        position,
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0, 1.0],
        tex_coord0: [0.0, 0.0],
    }
}

fn triangle() -> (Vec<VertexPBR>, Vec<u32>) {
    (
        vec![
            vertex([-0.5, -0.5, 0.5]),
            vertex([0.5, -0.5, 0.5]),
            vertex([0.0, 0.5, 0.5]),
        ],
        vec![0, 1, 2],
    )
}

fn create_dynamic_mesh(renderer: &mut VulkanRenderer, vertices: &[VertexPBR]) -> MeshHandle {
    let descriptor = MeshDescriptor {
        layout: VertexPBR::layout(),
        attributes: VertexPBR::attribute_kinds(),
        topology: PrimitiveTopology::TriangleList,
        usage: MeshUsage::Dynamic,
        vertex_count: vertices.len() as u32,
        index_count: vertices.len() as u32,
        index_format: IndexType::Uint32,
    };
    let indices: Vec<u32> = (0..vertices.len() as u32).collect();
    renderer
        .create_mesh_dynamic(&descriptor, bytemuck::cast_slice(vertices), &indices)
        .expect("dynamic mesh creation")
}

fn pixel(ndc_x: f32, ndc_y: f32) -> usize {
    let col = ((ndc_x + 1.0) * 0.5 * WIDTH as f32) as usize;
    let row = ((ndc_y + 1.0) * 0.5 * HEIGHT as f32) as usize;
    (row * WIDTH as usize + col) * 4
}

/// True when the pixel is red-dominant (the mesh tint). The target is
/// B8G8R8A8: the red channel is byte 2 of the readback.
fn covered(pixels: &Vec<u8>, ndc: (f32, f32)) -> bool {
    let at = pixel(ndc.0, ndc.1);
    pixels[at + 2] > 120 && pixels[at] < 120 && pixels[at + 1] < 120
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_static_mesh_stages_into_device_local_and_renders() {
    let mut renderer = headless_renderer();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured_errors = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured_errors.lock().unwrap().push(message.to_owned());
            }
        });

    let (vertices, indices) = triangle();
    let mesh = renderer
        .create_mesh(&vertices, &indices, PrimitiveTopology::TriangleList)
        .expect("static mesh creation");
    let report = renderer
        .mesh_memory_report(mesh)
        .expect("mesh must be live");
    assert!(
        report.device_local_buffers >= 1 && report.host_visible_buffers == 0,
        "static mesh buffers must be device-local on this device, got {report:?}"
    );
    assert_eq!(report.index_buffer, Some(MeshMemoryClass::DeviceLocal));

    // A dynamic mesh stays host-visible for direct writes.
    let dynamic = create_dynamic_mesh(&mut renderer, &vertices);
    let report = renderer
        .mesh_memory_report(dynamic)
        .expect("mesh must be live");
    assert!(
        report.device_local_buffers == 0 && report.host_visible_buffers >= 1,
        "dynamic mesh buffers must stay host-visible, got {report:?}"
    );
    assert_eq!(report.index_buffer, Some(MeshMemoryClass::HostVisible));

    // Both must render: the staged static mesh through its copy submission,
    // the dynamic mesh through its direct writes.
    let uniforms = FrameUniforms {
        view_matrix: identity(),
        proj_matrix: identity(),
        inv_view_proj_matrix: identity(),
        ..Default::default()
    };
    let shaders = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders");
    renderer
        .init_light_culling(WIDTH, HEIGHT, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();
    let material: MaterialHandle = renderer
        .compile_material(
            shaders.join("model_pbr.wgsl"),
            MaterialOptions {
                vertex_type: VertexType::Pbr,
                color_format: ImageFormat::B8G8R8A8Srgb,
                depth_test: false,
                double_sided: true,
                ..Default::default()
            },
        )
        .unwrap();

    let mut graph = FrameGraphBuilder::new()
        .add_pass(
            GeometryPass::new("geometry")
                .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                .clear_color([0.0, 0.0, 0.0, 1.0])
                .material(material),
        )
        .build::<VulkanRenderer>()
        .unwrap();
    let geometry_pass = graph.pass_id("geometry").unwrap();

    let mut render_and_capture = |renderer: &mut VulkanRenderer,
                                  graph: &mut FrameGraph<VulkanRenderer>,
                                  mesh: MeshHandle,
                                  frame: usize|
     -> Vec<u8> {
        let draw_list = {
            let mut list = DrawList::new();
            list.push(DrawCall::new(mesh, material).with_color([1.0, 0.1, 0.1, 1.0]));
            list
        };
        renderer.wait_for_frame().unwrap();
        renderer.set_frame_uniforms(uniforms.clone());
        renderer.execute_draw_calls(&draw_list).unwrap();
        renderer
            .render(graph, |frame_context| {
                frame_context.submit(geometry_pass, &draw_list);
            })
            .unwrap();
        renderer.queue_async_readback(frame).unwrap();
        let (_, pixels) = renderer.wait_for_pending_readback().unwrap().unwrap();
        pixels
    };

    let staged_pixels = render_and_capture(&mut renderer, &mut graph, mesh, 0);
    assert!(
        covered(&staged_pixels, (0.0, 0.0)),
        "the staged static mesh must render through its copy submission"
    );
    let dynamic_pixels = render_and_capture(&mut renderer, &mut graph, dynamic, 1);
    assert!(covered(&dynamic_pixels, (0.0, 0.0)));

    // Frame boundaries released the staged uploads.
    assert_eq!(renderer.pending_staged_uploads(), 0);

    // Destroying the meshes retires their native buffers (staged uploads
    // and in-flight frames may still reference them); rendered frames
    // advance the retirement age until they free.
    renderer.destroy_mesh(mesh);
    renderer.destroy_mesh(dynamic);
    for frame in 2..6 {
        renderer.wait_for_frame().unwrap();
        renderer.set_frame_uniforms(uniforms.clone());
        renderer
            .render(&mut graph, |_| {})
            .expect("empty frame render");
        let _ = frame;
    }
    assert_eq!(renderer.pending_buffer_retirements(), 0);
    assert_eq!(renderer.pending_staged_uploads(), 0);

    graph.cleanup();
    drop(graph);
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_many_small_and_large_static_meshes() {
    let mut renderer = headless_renderer();

    // Many small meshes: every one lands device-local with one bounded
    // submission per creation.
    let mut handles = Vec::new();
    for _ in 0..50 {
        let (vertices, indices) = triangle();
        handles.push(
            renderer
                .create_mesh(&vertices, &indices, PrimitiveTopology::TriangleList)
                .expect("small static mesh"),
        );
    }
    for handle in &handles {
        let report = renderer
            .mesh_memory_report(*handle)
            .expect("mesh must be live");
        assert_eq!(report.host_visible_buffers, 0, "{report:?}");
    }

    // One large mesh (~48.6k vertices) stages through the same path.
    let large: Vec<VertexPBR> = (0..48_600)
        .map(|i| {
            let f = i as f32;
            vertex([f.cos() * 0.5, f.sin() * 0.5, 0.5])
        })
        .collect();
    let large_indices: Vec<u32> = (0..large.len() as u32).collect();
    let large_handle = renderer
        .create_mesh(&large, &large_indices, PrimitiveTopology::TriangleList)
        .expect("large static mesh");
    let report = renderer.mesh_memory_report(large_handle).unwrap();
    assert_eq!(report.host_visible_buffers, 0, "{report:?}");
    assert_eq!(renderer.mesh_vertex_count(large_handle), Some(48_600));

    // Destroying every mesh keeps the allocations bounded: retirement
    // frees within FRAMES_IN_FLIGHT frames and staged uploads release at
    // the same boundaries.
    for handle in handles {
        renderer.destroy_mesh(handle);
    }
    renderer.destroy_mesh(large_handle);

    // The large copy needs real completion time: a device-wide idle wait
    // proves every submission finished, and the next frame boundary
    // releases the staged uploads.
    renderer.wait_for_device();
    renderer.wait_for_frame().unwrap();
    assert_eq!(
        renderer.pending_staged_uploads(),
        0,
        "staged uploads must release once their submissions completed"
    );

    renderer.destroy();
}
