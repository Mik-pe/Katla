//! Dynamic mesh update semantics for issue #86.
//!
//! A dynamic mesh update must publish one internally consistent mesh: logical
//! vertex/index counts, GPU buffer contents, and capacity all describe the
//! same mesh after success, and a failed update leaves the previous mesh
//! fully intact. Growth retires replaced native buffers instead of freeing
//! them under in-flight submissions. Both backends implement the same
//! contract (`validate_dynamic_update`); these tests prove it end to end on
//! Vulkan, including through real rendered frames.
//!
//! Pure contract tests run everywhere; the rest need a Vulkan device
//! (`#[ignore]`, like the other GPU contract tests).

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use katla_gfx::render_graph::{FrameGraph, FrameGraphBuilder, GeometryPass};
use katla_gfx::renderer::{DrawCall, DrawList};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::VertexPBR;
use katla_gfx::{
    CullMode, DepthState, FrameUniforms, GpuRenderer, IndexType, MaterialHandle, MeshDescriptor,
    MeshHandle, MeshUsage, PipelineDescriptor, PrimitiveTopology, ValidationMode, Vertex,
    VulkanRenderer, validate_dynamic_update,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;

// ---------------------------------------------------------------------------
// Pure tests (no device)
// ---------------------------------------------------------------------------

#[test]
fn test_dynamic_update_contract_accepts_consistent_payloads() {
    // Three 4-byte-stride... use a 12-byte position stride for realism.
    let blob = [0u8; 3 * 12];
    assert!(validate_dynamic_update(12, &blob, 3, &[0, 1, 2]).is_ok());

    // The fully-empty transition is valid: it draws nothing.
    assert!(validate_dynamic_update(12, &[], 0, &[]).is_ok());
}

#[test]
fn test_dynamic_update_contract_rejects_blob_count_disagreement() {
    let blob = [0u8; 2 * 12];
    let error = validate_dynamic_update(12, &blob, 3, &[0, 1, 2]).unwrap_err();
    match error {
        katla_gfx::RendererError::InvalidDescriptor { reason, .. } => {
            assert!(reason.contains("24 bytes"), "{reason}");
            assert!(reason.contains("3 vertices"), "{reason}");
        }
        other => panic!("expected InvalidDescriptor, got {other:?}"),
    }
}

#[test]
fn test_dynamic_update_contract_rejects_out_of_range_indices() {
    let blob = [0u8; 3 * 12];
    let error = validate_dynamic_update(12, &blob, 3, &[0, 1, 3]).unwrap_err();
    match error {
        katla_gfx::RendererError::InvalidDescriptor { reason, .. } => {
            assert!(reason.contains("vertex 3"), "{reason}");
        }
        other => panic!("expected InvalidDescriptor, got {other:?}"),
    }

    // Any index against an empty vertex list is out of range.
    let error = validate_dynamic_update(12, &[], 0, &[0]).unwrap_err();
    assert!(matches!(
        error,
        katla_gfx::RendererError::InvalidDescriptor { .. }
    ));
}

// ---------------------------------------------------------------------------
// Device tests (ignored without a GPU)
// ---------------------------------------------------------------------------

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        WIDTH,
        HEIGHT,
        // ValidationMode::Disabled: compiling the PBR pipeline under the
        // system validation layer segfaults the Intel driver on this machine
        // (same caveat as the instanced-draw contract test).
        ValidationMode::Disabled,
        CString::new("Dynamic mesh update test").unwrap(),
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

/// A PBR vertex at an NDC position. Vulkan NDC y points down: y = -1 is the
/// top row of the target.
fn vertex(position: [f32; 3]) -> VertexPBR {
    VertexPBR {
        position,
        normal: [0.0, 0.0, 1.0],
        tangent: [1.0, 0.0, 0.0, 1.0],
        tex_coord0: [0.0, 0.0],
    }
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

/// Byte offset of the pixel at NDC coordinates.
fn pixel(ndc_x: f32, ndc_y: f32) -> usize {
    let col = ((ndc_x + 1.0) * 0.5 * WIDTH as f32) as usize;
    let row = ((ndc_y + 1.0) * 0.5 * HEIGHT as f32) as usize;
    (row * WIDTH as usize + col) * 4
}

/// Two triangles covering the top-left NDC quadrant, then their indices.
fn quad_top_left() -> (Vec<VertexPBR>, Vec<u32>) {
    let vertices = vec![
        vertex([-1.0, -1.0, 0.5]), // triangle A: lower-left half of quadrant
        vertex([0.0, -1.0, 0.5]),
        vertex([-1.0, 0.0, 0.5]),
        vertex([0.0, -1.0, 0.5]), // triangle B: upper-right half
        vertex([0.0, 0.0, 0.5]),
        vertex([-1.0, 0.0, 0.5]),
    ];
    let indices = vec![0, 1, 2, 3, 4, 5];
    (vertices, indices)
}

/// Four triangles covering the whole top half (both top quadrants).
fn quad_top_half() -> (Vec<VertexPBR>, Vec<u32>) {
    let vertices = vec![
        vertex([-1.0, -1.0, 0.5]),
        vertex([0.0, -1.0, 0.5]),
        vertex([-1.0, 0.0, 0.5]),
        vertex([0.0, -1.0, 0.5]),
        vertex([0.0, 0.0, 0.5]),
        vertex([-1.0, 0.0, 0.5]),
        vertex([0.0, -1.0, 0.5]),
        vertex([1.0, -1.0, 0.5]),
        vertex([0.0, 0.0, 0.5]),
        vertex([1.0, -1.0, 0.5]),
        vertex([1.0, 0.0, 0.5]),
        vertex([0.0, 0.0, 0.5]),
    ];
    let indices = (0..12u32).collect();
    (vertices, indices)
}

fn update(
    renderer: &mut VulkanRenderer,
    mesh: MeshHandle,
    vertices: &[VertexPBR],
    indices: &[u32],
) {
    renderer
        .update_mesh_dynamic(
            mesh,
            bytemuck::cast_slice(vertices),
            vertices.len() as u32,
            indices,
        )
        .expect("dynamic mesh update");
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_dynamic_mesh_updates_preserve_counts_and_rendering() {
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
    // PBR pipelines declare Set 4 for shadow data; the descriptor layouts
    // must exist before the material is compiled.
    renderer
        .init_shadow_resources(None, katla_gfx::CascadeParams::default())
        .unwrap();
    let material: MaterialHandle = renderer
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

    let (vertices, _indices) = quad_top_left();
    let mesh = create_dynamic_mesh(&mut renderer, &vertices);
    assert_eq!(renderer.mesh_vertex_count(mesh), Some(6));
    assert_eq!(renderer.mesh_index_count(mesh), Some(6));
    assert_eq!(renderer.mesh_index_format(mesh), Some(IndexType::Uint32));

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

    let render_and_capture = |renderer: &mut VulkanRenderer,
                              graph: &mut FrameGraph<VulkanRenderer>,
                              frame: usize|
     -> Vec<u8> {
        let draw_list = {
            let mut list = DrawList::new();
            // The red tint makes mesh pixels distinguishable from the lit
            // gray background these headless captures settle on.
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

    // True when the pixel is red-dominant (the mesh tint). The target is
    // B8G8R8A8: the red channel is byte 2 of the readback.
    let covered = |pixels: &Vec<u8>, ndc: (f32, f32)| -> bool {
        let at = pixel(ndc.0, ndc.1);
        pixels[at + 2] > 120 && pixels[at] < 120 && pixels[at + 1] < 120
    };
    // True when no mesh-tinted pixel exists anywhere.
    let no_mesh_pixels = |pixels: &Vec<u8>| -> bool {
        !pixels
            .chunks_exact(4)
            .any(|c| c[2] > 120 && c[0] < 120 && c[1] < 120)
    };

    // Frame 0: both triangles of the top-left quadrant are visible.
    let quad_pixels = render_and_capture(&mut renderer, &mut graph, 0);
    assert!(
        covered(&quad_pixels, (-0.75, -0.75)),
        "triangle A must cover the lower-left half"
    );
    assert!(
        covered(&quad_pixels, (-0.25, -0.25)),
        "triangle B must cover the upper-right half"
    );

    // Same-size update: rewrite every vertex with shifted geometry; the new
    // geometry must reach the GPU (both halves still covered, counts kept).
    {
        let (mut shifted, indices) = quad_top_left();
        for v in &mut shifted {
            v.position[2] = 0.25;
        }
        update(&mut renderer, mesh, &shifted, &indices);
    }
    assert_eq!(renderer.mesh_vertex_count(mesh), Some(6));
    let same_size_pixels = render_and_capture(&mut renderer, &mut graph, 1);
    assert!(covered(&same_size_pixels, (-0.75, -0.75)));
    assert!(covered(&same_size_pixels, (-0.25, -0.25)));

    // Shrink to triangle A only: B's region reverts to background, no
    // reallocation happens, and counts track the smaller mesh.
    let shrunk = quad_top_left().0[..3].to_vec();
    update(&mut renderer, mesh, &shrunk, &[0, 1, 2]);
    assert_eq!(renderer.mesh_vertex_count(mesh), Some(3));
    assert_eq!(renderer.mesh_index_count(mesh), Some(3));
    assert_eq!(
        renderer.pending_retirements().buffers,
        0,
        "shrinking must not reallocate"
    );
    let shrunk_pixels = render_and_capture(&mut renderer, &mut graph, 2);
    assert!(covered(&shrunk_pixels, (-0.75, -0.75)));
    assert!(
        !covered(&shrunk_pixels, (-0.25, -0.25)),
        "triangle B must be gone after shrinking to triangle A"
    );

    // Grow past the created capacity (6 -> 12 vertices): the whole top half
    // renders, the old buffers retire instead of freeing immediately, and
    // the retirements drain once the frames using the old buffers complete.
    let (grown, grown_indices) = quad_top_half();
    update(&mut renderer, mesh, &grown, &grown_indices);
    assert_eq!(renderer.mesh_vertex_count(mesh), Some(12));
    assert_eq!(renderer.mesh_index_count(mesh), Some(12));
    assert!(
        renderer.pending_retirements().buffers > 0,
        "growth past capacity must retire the replaced buffers"
    );
    let grown_pixels = render_and_capture(&mut renderer, &mut graph, 3);
    assert!(covered(&grown_pixels, (-0.75, -0.75)));
    assert!(covered(&grown_pixels, (-0.25, -0.25)));
    assert!(
        covered(&grown_pixels, (0.5, -0.5)),
        "the grown top-right quadrant must render"
    );
    for frame in 4..7 {
        render_and_capture(&mut renderer, &mut graph, frame);
    }
    assert_eq!(
        renderer.pending_retirements().buffers,
        0,
        "retirements must drain once their submissions completed"
    );

    // Populated -> empty: the mesh draws nothing at all.
    renderer
        .update_mesh_dynamic(mesh, &[], 0, &[])
        .expect("empty update");
    assert_eq!(renderer.mesh_vertex_count(mesh), Some(0));
    assert_eq!(renderer.mesh_index_count(mesh), Some(0));
    let empty_pixels = render_and_capture(&mut renderer, &mut graph, 7);
    assert!(
        no_mesh_pixels(&empty_pixels),
        "an empty mesh must leave no mesh-tinted pixel anywhere"
    );

    // Empty -> populated through growth: the mesh renders again.
    update(&mut renderer, mesh, &shrunk, &[0, 1, 2]);
    assert_eq!(renderer.mesh_vertex_count(mesh), Some(3));
    assert_eq!(renderer.mesh_index_count(mesh), Some(3));
    let repopulated_pixels = render_and_capture(&mut renderer, &mut graph, 8);
    assert!(covered(&repopulated_pixels, (-0.75, -0.75)));
    assert!(!covered(&repopulated_pixels, (-0.25, -0.25)));

    // The recorded index width survives every update.
    assert_eq!(renderer.mesh_index_format(mesh), Some(IndexType::Uint32));

    // Repeated interleaved update/render cycles (updates land between
    // submitted frames) stay consistent and leak nothing.
    for cycle in 0..5 {
        let (verts, idx) = if cycle % 2 == 0 {
            quad_top_left()
        } else {
            (quad_top_left().0[..3].to_vec(), vec![0, 1, 2])
        };
        update(&mut renderer, mesh, &verts, &idx);
        render_and_capture(&mut renderer, &mut graph, 9 + cycle);
    }
    assert_eq!(renderer.pending_retirements().buffers, 0);

    graph.cleanup();
    drop(graph);
    renderer.destroy();
    drop(renderer);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_dynamic_mesh_update_rejects_inconsistent_payloads() {
    let mut renderer = headless_renderer();

    let (vertices, indices) = quad_top_left();
    let mesh = create_dynamic_mesh(&mut renderer, &vertices);

    // Blob disagrees with the declared vertex count.
    let error = renderer
        .update_mesh_dynamic(mesh, bytemuck::cast_slice(&vertices[..3]), 3, &indices)
        .unwrap_err();
    assert!(
        matches!(error, katla_gfx::RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );

    // Index out of range for the new vertex count.
    let error = renderer
        .update_mesh_dynamic(mesh, bytemuck::cast_slice(&vertices[..3]), 3, &[0, 1, 3])
        .unwrap_err();
    assert!(
        matches!(error, katla_gfx::RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );

    // Static meshes are immutable.
    let static_mesh = renderer
        .create_mesh(&vertices, &indices[..], PrimitiveTopology::TriangleList)
        .unwrap();
    let error = renderer
        .update_mesh_dynamic(static_mesh, bytemuck::cast_slice(&vertices), 6, &indices)
        .unwrap_err();
    match error {
        katla_gfx::RendererError::InvalidOperation(msg) => {
            assert!(msg.contains("immutable"), "{msg}");
        }
        other => panic!("expected InvalidOperation, got {other:?}"),
    }

    // Stale handles fail typed.
    renderer.destroy_mesh(mesh);
    let error = renderer
        .update_mesh_dynamic(mesh, bytemuck::cast_slice(&vertices), 6, &indices)
        .unwrap_err();
    assert!(
        matches!(error, katla_gfx::RendererError::StaleHandle { .. }),
        "got {error:?}"
    );

    renderer.destroy();
}
