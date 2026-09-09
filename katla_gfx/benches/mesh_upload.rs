//! Mesh upload-path benchmarks (issue #96).
//!
//! Measures the cost of creating static meshes (upload path), one large
//! static mesh, and repeated same-size dynamic updates. Requires a Vulkan
//! device; not part of CI (which never runs benches).

use std::ffi::CString;
use std::time::Instant;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use katla_gfx::vertex::VertexPBR;
use katla_gfx::{PrimitiveTopology, ValidationMode, VulkanRenderer};

fn vertex(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> VertexPBR {
    VertexPBR {
        position,
        normal,
        tangent: [1.0, 0.0, 0.0, 1.0],
        tex_coord0: uv,
    }
}

/// A cube-style small mesh: 8 vertices, 12 indices (two triangles per face
/// would be 24/36; small is the point here).
fn small_mesh() -> (Vec<VertexPBR>, Vec<u32>) {
    let vertices = vec![
        vertex([-0.5, -0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
        vertex([0.5, -0.5, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0]),
        vertex([0.5, 0.5, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0]),
        vertex([-0.5, 0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0]),
        vertex([-0.5, -0.5, -1.0], [0.0, 0.0, -1.0], [0.0, 0.0]),
        vertex([0.5, -0.5, -1.0], [0.0, 0.0, -1.0], [1.0, 0.0]),
        vertex([0.5, 0.5, -1.0], [0.0, 0.0, -1.0], [1.0, 1.0]),
        vertex([-0.5, 0.5, -1.0], [0.0, 0.0, -1.0], [0.0, 1.0]),
    ];
    let indices = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    (vertices, indices)
}

/// A large grid mesh: `quads * quads` unshared two-triangle quads.
fn large_mesh(quads: usize) -> (Vec<VertexPBR>, Vec<u32>) {
    let mut vertices = Vec::with_capacity(quads * quads * 6);
    let step = 2.0 / quads as f32;
    for gy in 0..quads {
        for gx in 0..quads {
            let x0 = -1.0 + gx as f32 * step;
            let y0 = -1.0 + gy as f32 * step;
            let x1 = x0 + step;
            let y1 = y0 + step;
            for (position, uv) in [
                ([x0, y0, 0.0], [0.0, 0.0]),
                ([x1, y0, 0.0], [1.0, 0.0]),
                ([x1, y1, 0.0], [1.0, 1.0]),
                ([x0, y0, 0.0], [0.0, 0.0]),
                ([x1, y1, 0.0], [1.0, 1.0]),
                ([x0, y1, 0.0], [0.0, 1.0]),
            ] {
                vertices.push(vertex(position, [0.0, 0.0, 1.0], uv));
            }
        }
    }
    let indices = (0..vertices.len() as u32).collect();
    (vertices, indices)
}

fn bench_mesh_upload(c: &mut Criterion) {
    let mut renderer = VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Disabled,
        CString::new("mesh upload bench").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .expect("bench renderer");

    let (small, small_indices) = small_mesh();
    let (large, large_indices) = large_mesh(90); // ~48.6k vertices

    {
        let mut group = c.benchmark_group("mesh_upload");
        group.sample_size(10);

        group.bench_function("small_static_x64", |b| {
            b.iter_custom(|iters| {
                let mut elapsed = std::time::Duration::ZERO;
                let mut handles = Vec::new();
                for _ in 0..iters {
                    let start = Instant::now();
                    for _ in 0..64 {
                        handles.push(
                            renderer
                                .create_mesh(
                                    black_box(&small),
                                    black_box(&small_indices),
                                    PrimitiveTopology::TriangleList,
                                )
                                .expect("static mesh creation"),
                        );
                    }
                    elapsed += start.elapsed();
                    for handle in handles.drain(..) {
                        renderer.destroy_mesh(handle);
                    }
                    // Frame boundary: release completed staged uploads and
                    // retired buffers, like the render loop would.
                    renderer.wait_for_frame().expect("frame wait");
                }
                elapsed
            })
        });

        group.bench_function("large_static_48k_verts", |b| {
            b.iter_custom(|iters| {
                let mut elapsed = std::time::Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    let handle = renderer
                        .create_mesh(
                            black_box(&large),
                            black_box(&large_indices),
                            PrimitiveTopology::TriangleList,
                        )
                        .expect("large static mesh creation");
                    elapsed += start.elapsed();
                    renderer.destroy_mesh(handle);
                    renderer.wait_for_frame().expect("frame wait");
                }
                elapsed
            })
        });

        group.finish();
    }

    renderer.destroy();
}

criterion_group!(benches, bench_mesh_upload);
criterion_main!(benches);
