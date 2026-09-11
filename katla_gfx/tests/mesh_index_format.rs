//! Mesh index format is preserved from creation through draw encoding.
//!
//! A mesh uploaded with `u16` indices must render byte-identically to the same
//! mesh uploaded with `u32` indices; the recorded format (not a hardcoded
//! `UINT32`) must reach `vkCmdBindIndexBuffer`.

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use katla_gfx::render_graph::{FrameGraphBuilder, GeometryPass};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::VertexPBR;
use katla_gfx::{
    CullMode, DepthState, DrawCall, DrawList, FrameUniforms, GpuRenderer, IndexType,
    PipelineDescriptor, ValidationMode, VulkanRenderer,
};

fn identity() -> [f32; 16] {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

/// One triangle covering the viewport center, in clip space (identity
/// view/proj), indexed at the caller's chosen width.
fn triangle_u16() -> (Vec<VertexPBR>, Vec<u16>) {
    (triangle_vertices(), vec![0, 1, 2])
}

fn triangle_u32() -> (Vec<VertexPBR>, Vec<u32>) {
    (triangle_vertices(), vec![0, 1, 2])
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

#[test]
#[ignore = "requires a Vulkan device"]
fn test_u16_and_u32_indexed_meshes_render_identically() {
    // ValidationMode::Disabled: compiling the PBR pipeline under the system
    // validation layer segfaults the Intel driver on this machine; the format
    // contract under test is exercised identically without it.
    let mut renderer = VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Disabled,
        CString::new("Index format test").unwrap(),
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
        .init_light_culling(64, 48, &shaders.join("lighting/light_cull.wgsl"))
        .unwrap();
    // PBR pipelines declare Set 4 for shadow data; the descriptor layouts must
    // exist before the material is compiled or pipeline creation is invalid.
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();
    let material = renderer
        .compile_material(
            &PipelineDescriptor::pbr(
                shaders
                    .join("model_pbr.wgsl")
                    .to_string_lossy()
                    .into_owned(),
            )
            .with_color_format(ImageFormat::B8G8R8A8Srgb)
            .with_depth(DepthState::disabled())
            .with_cull(CullMode::None),
        )
        .unwrap();

    let (u16_vertices, u16_indices) = triangle_u16();
    let (u32_vertices, u32_indices) = triangle_u32();
    let mesh_u16 = renderer
        .create_mesh(
            &u16_vertices,
            &u16_indices,
            katla_gfx::PrimitiveTopology::TriangleList,
        )
        .expect("test mesh creation");
    let mesh_u32 = renderer
        .create_mesh(
            &u32_vertices,
            &u32_indices,
            katla_gfx::PrimitiveTopology::TriangleList,
        )
        .expect("test mesh creation");

    assert_eq!(
        renderer.mesh_index_format(mesh_u16),
        Some(IndexType::Uint16)
    );
    assert_eq!(
        renderer.mesh_index_format(mesh_u32),
        Some(IndexType::Uint32)
    );
    let destroyed = mesh_u16;
    let stale: katla_gfx::MeshHandle = katla_gfx::Handle::NONE;
    renderer.destroy_mesh(destroyed);
    assert_eq!(renderer.mesh_index_format(destroyed), None);
    assert_eq!(renderer.mesh_index_format(stale), None);

    // Recreate the u16 mesh that was destroyed above.
    let mesh_u16 = renderer
        .create_mesh(
            &u16_vertices,
            &u16_indices,
            katla_gfx::PrimitiveTopology::TriangleList,
        )
        .expect("test mesh creation");

    let draw_list_for = |mesh| {
        let mut list = DrawList::new();
        list.push(DrawCall::new(mesh, material));
        list
    };

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

    let mut captured = Vec::new();
    for (frame, mesh) in [(0, mesh_u16), (1, mesh_u32), (2, mesh_u16), (3, mesh_u32)] {
        // Per-frame-slot storage: uniforms + object data must be refreshed
        // every frame (the recommended wait → uniforms → objects → render order).
        renderer.wait_for_frame().unwrap();
        renderer.set_frame_uniforms(uniforms.clone());
        let draw_list = draw_list_for(mesh);
        renderer.execute_draw_calls(&draw_list).unwrap();
        renderer
            .render(&mut graph, |frame_context| {
                frame_context.submit(geometry_pass, &draw_list);
            })
            .unwrap();
        renderer.queue_async_readback(frame).unwrap();
        let (_, pixels) = renderer.wait_for_pending_readback().unwrap().unwrap();
        assert_eq!(pixels.len(), 64 * 48 * 4);

        // The triangle must actually be rasterized: center covered, corner not.
        let center = (24 * 64 + 32) * 4;
        let corner = (4 * 64 + 4) * 4;
        assert_ne!(&pixels[center..center + 4], &pixels[corner..corner + 4]);
        assert_eq!(pixels[center + 3], 255);
        captured.push(pixels);
    }

    for pixels in &captured[1..] {
        assert_eq!(pixels, &captured[0], "u16 and u32 renders must match");
    }

    graph.cleanup();
    drop(graph);
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
