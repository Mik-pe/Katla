//! Geometry instancing uploads and encodes every submitted instance.
//!
//! One instanced draw of N instances must render byte-identically to N direct
//! draws carrying the same per-object data: every instance is uploaded to its
//! own frame-local object slot, and the encode passes the exact instance count
//! with the assigned base slot (the shader walks `objects[@builtin(instance_index)]`).

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use katla_gfx::render_graph::{FrameGraphBuilder, GeometryPass};
use katla_gfx::renderer::{DrawCall, DrawList, InstanceData};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::VertexPBR;
use katla_gfx::{FrameUniforms, MaterialOptions, ValidationMode, VertexType, VulkanRenderer};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

fn scaled_translation(offset: [f32; 2], scale: f32) -> [f32; 16] {
    // Column-major T * S: scale columns, translation in column 3.
    [
        scale, 0.0, 0.0, 0.0, //
        0.0, scale, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        offset[0], offset[1], 0.0, 1.0,
    ]
}

/// Distinct quadrant centers in NDC; the 0.5-scaled triangle spans ±0.25
/// around its centroid, so each copy stays inside its quadrant.
const QUADRANTS: [[f32; 2]; 4] = [[0.5, 0.6], [-0.5, 0.6], [-0.5, -0.6], [0.5, -0.6]];

const INSTANCE_COLORS: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [1.0, 1.0, 0.0, 1.0],
];

fn instance(i: usize) -> InstanceData {
    InstanceData::default()
        .with_transform(scaled_translation(QUADRANTS[i], 0.5))
        .with_color(INSTANCE_COLORS[i])
}

fn identity() -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

fn triangle_vertices() -> Vec<VertexPBR> {
    vec![
        VertexPBR {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.0, 0.0],
        },
        VertexPBR {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [1.0, 0.0],
        },
        VertexPBR {
            position: [0.0, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.5, 1.0],
        },
    ]
}

fn quadrant_pixel(i: usize) -> usize {
    // Triangle centroid in NDC: (0, -1/6) before the quadrant offset.
    // Vulkan NDC y points down: y = -1 is the top row of the target.
    let ndc_x = QUADRANTS[i][0];
    let ndc_y = QUADRANTS[i][1] - 1.0 / 6.0;
    let col = ((ndc_x + 1.0) * 0.5 * WIDTH as f32) as usize;
    let row = ((ndc_y + 1.0) * 0.5 * HEIGHT as f32) as usize;
    (row * WIDTH as usize + col) * 4
}

fn direct_call(
    mesh: katla_gfx::MeshHandle,
    material: katla_gfx::MaterialHandle,
    i: usize,
) -> DrawCall {
    let inst = instance(i);
    DrawCall::new(mesh, material)
        .with_transform(inst.model_matrix)
        .with_color(inst.color)
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_instanced_draw_matches_direct_draws() {
    // ValidationMode::Disabled: compiling the PBR pipeline under the system
    // validation layer segfaults the Intel driver on this machine; the
    // instancing contract under test is exercised identically without it.
    let mut renderer = VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        ValidationMode::Disabled,
        CString::new("Instanced draw test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured_errors = errors.clone();
    renderer
        .context()
        .set_validation_callback(move |message, level| {
            if level == katla_gfx::ValidationLevel::Error {
                captured_errors.lock().unwrap().push(message.to_owned());
            }
        });

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
    // PBR pipelines declare Set 4 for shadow data; the descriptor layouts must
    // exist before the material is compiled or pipeline creation is invalid.
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();
    let material = renderer
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

    let vertices = triangle_vertices();
    let indices: Vec<u32> = vec![0, 1, 2];
    let mesh = renderer.create_mesh(&vertices, &indices);

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

    let mut render_and_capture = |draw_list: &DrawList, frame: usize| -> Vec<u8> {
        // Per-frame-slot storage: uniforms + object data must be refreshed
        // every frame (the recommended wait → uniforms → objects → render order).
        renderer.wait_for_frame().unwrap();
        renderer.set_frame_uniforms(uniforms.clone());
        renderer.execute_draw_calls(draw_list).unwrap();
        renderer
            .render(&mut graph, |frame_context| {
                frame_context.submit(geometry_pass, draw_list);
            })
            .unwrap();
        renderer.queue_async_readback(frame).unwrap();
        let (_, pixels) = renderer.wait_for_pending_readback().unwrap().unwrap();
        assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
        pixels
    };

    // Frame 0: one instanced draw carrying all four instances.
    let instanced = {
        let mut list = DrawList::new();
        list.push(DrawCall::instanced(
            mesh,
            material,
            (0..4).map(instance).collect(),
        ));
        assert_eq!(list.len(), 1);
        list
    };
    let instanced_pixels = render_and_capture(&instanced, 0);

    // Every quadrant must actually show its own instance's color — if only
    // the first instance were uploaded, all quadrants would match instance 0.
    let background = 0;
    for i in 0..4 {
        let px = quadrant_pixel(i);
        assert_ne!(
            &instanced_pixels[px..px + 4],
            &instanced_pixels[background..background + 4],
            "quadrant {i} must be covered by its instance"
        );
    }

    // Frame 1: four direct draws with the same per-object data.
    let direct = {
        let mut list = DrawList::new();
        for i in 0..4 {
            list.push(direct_call(mesh, material, i));
        }
        list
    };
    let direct_pixels = render_and_capture(&direct, 1);
    assert_eq!(
        instanced_pixels, direct_pixels,
        "one instanced draw must render identically to four direct draws"
    );

    // Frame 2: mixed direct + instanced draws in one list, with a different
    // push order — per-object data must follow each draw regardless.
    let mixed = {
        let mut list = DrawList::new();
        list.push(direct_call(mesh, material, 0));
        list.push(DrawCall::instanced(
            mesh,
            material,
            (1..4).map(instance).collect(),
        ));
        list
    };
    let mixed_pixels = render_and_capture(&mixed, 2);
    assert_eq!(mixed_pixels, instanced_pixels, "mixed list must match");

    // Frame 3: frame-slot reuse — rewrite one late instance's color in a new
    // instanced draw; the new color must reach its quadrant.
    let recolored = {
        let mut list = DrawList::new();
        let mut instances: Vec<InstanceData> = (0..4).map(instance).collect();
        instances[3].color = [0.0, 1.0, 1.0, 1.0];
        list.push(DrawCall::instanced(mesh, material, instances));
        list
    };
    let recolored_pixels = render_and_capture(&recolored, 3);
    let px3 = quadrant_pixel(3);
    assert_ne!(
        &recolored_pixels[px3..px3 + 4],
        &instanced_pixels[px3..px3 + 4],
        "instance 3's rewritten color must reach the GPU"
    );

    // Capacity exhaustion must return the typed error, never overwrite.
    let oversized = {
        let mut list = DrawList::new();
        for _ in 0..300 {
            list.push(DrawCall::new(mesh, material));
        }
        list
    };
    renderer.wait_for_frame().unwrap();
    let err = renderer
        .execute_draw_calls(&oversized)
        .expect_err("a draw range past the per-frame object limit must fail");
    assert!(err.to_string().contains("MAX_OBJECTS_PER_FRAME"));

    graph.cleanup();
    drop(graph);
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
