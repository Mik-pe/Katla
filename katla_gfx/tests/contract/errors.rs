//! Error contracts: invalid descriptors and unsupported capabilities must
//! fail with typed errors before touching GPU state, failed creation must
//! register nothing, and rejected updates must leave the previous state
//! intact. This module compiles no PBR pipelines, so API validation is
//! captured throughout — a rejection must never produce a driver-level
//! validation error.

use katla_gfx::renderer::features::RendererFeature;
use katla_gfx::vertex::VertexPBR;
use katla_gfx::{
    GpuRenderer, IndexType, MeshDescriptor, MeshUsage, PrimitiveTopology, RendererError,
    TextureDescriptor, UIDrawList, Vertex,
};

use crate::harness::{self, ContractRenderer};

fn dynamic_descriptor(vertex_count: u32, index_count: u32) -> MeshDescriptor {
    MeshDescriptor {
        layout: VertexPBR::layout(),
        attributes: VertexPBR::attribute_kinds(),
        topology: PrimitiveTopology::TriangleList,
        usage: MeshUsage::Dynamic,
        vertex_count,
        index_count,
        index_format: IndexType::Uint32,
    }
}

fn triangle() -> Vec<VertexPBR> {
    harness::clip_triangle()
}

/// Every rejection in this scenario must be a typed `RendererError` (never a
/// panic, never a silent success), must not disturb live state, and must not
/// produce Vulkan validation errors.
#[test]
#[ignore = "requires a graphics device"]
fn test_contract_invalid_descriptors_fail_typed_without_disturbing_state() {
    let mut renderer = ContractRenderer::open("contract: invalid descriptors");

    // Zero-extent texture descriptors are invalid before any GPU work.
    let error = renderer
        .gfx()
        .create_texture(&TextureDescriptor::rgba8_unorm(0, 0), &[])
        .expect_err("zero-extent texture must be rejected");
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );

    // Data that disagrees with the descriptor is invalid.
    let error = renderer
        .gfx()
        .create_texture(&TextureDescriptor::rgba8_unorm(4, 4), &[1, 2, 3, 4])
        .expect_err("mismatched texture data must be rejected");
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );

    // Empty and out-of-range meshes are invalid.
    let error = renderer
        .gfx()
        .create_mesh::<VertexPBR, u32>(&[], &[], PrimitiveTopology::TriangleList)
        .expect_err("an empty mesh must be rejected");
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );
    let error = renderer
        .gfx()
        .create_mesh(&triangle(), &[0u32, 1, 3], PrimitiveTopology::TriangleList)
        .expect_err("an out-of-range index must be rejected");
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );

    // Dynamic updates: inconsistent payloads, immutable static meshes, and
    // stale handles are all typed, and a rejected update leaves the mesh as
    // it was.
    let mesh = renderer
        .gfx()
        .create_mesh_dynamic(
            &dynamic_descriptor(3, 3),
            bytemuck::cast_slice(&triangle()[..3]),
            &[0, 1, 2],
        )
        .expect("dynamic contract mesh");
    let vertices = triangle();
    let error = renderer
        .gfx()
        .update_mesh_dynamic(mesh, bytemuck::cast_slice(&vertices), 6, &[0, 1, 2])
        .expect_err("blob/count disagreement must be rejected");
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );
    let error = renderer
        .gfx()
        .update_mesh_dynamic(mesh, bytemuck::cast_slice(&vertices[..3]), 3, &[0, 1, 3])
        .expect_err("out-of-range update index must be rejected");
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );
    assert_eq!(
        renderer.gfx().mesh_vertex_count(mesh),
        Some(3),
        "rejected updates must leave the mesh intact"
    );

    let static_mesh = renderer
        .gfx()
        .create_mesh(&vertices, &[0u32, 1, 2], PrimitiveTopology::TriangleList)
        .expect("static contract mesh");
    let error = renderer
        .gfx()
        .update_mesh_dynamic(static_mesh, bytemuck::cast_slice(&vertices), 3, &[0, 1, 2])
        .expect_err("static meshes are immutable");
    assert!(
        matches!(error, RendererError::InvalidOperation(_)),
        "got {error:?}"
    );

    renderer.gfx().destroy_mesh(mesh);
    let error = renderer
        .gfx()
        .update_mesh_dynamic(mesh, bytemuck::cast_slice(&vertices), 3, &[0, 1, 2])
        .expect_err("a destroyed mesh handle must fail typed");
    assert!(
        matches!(error, RendererError::StaleHandle { .. }),
        "got {error:?}"
    );

    // Texture updates: size mismatches and stale handles are typed too.
    let texture = renderer
        .gfx()
        .create_texture(&TextureDescriptor::rgba8_unorm(2, 2), &[9; 16])
        .expect("contract texture");
    let error = renderer
        .gfx()
        .update_texture(texture, &[9, 9])
        .expect_err("size-mismatched updates must be rejected");
    assert!(
        matches!(error, RendererError::UploadFailed { .. }),
        "got {error:?}"
    );
    renderer.gfx().destroy_texture(texture);
    let error = renderer
        .gfx()
        .update_texture(texture, &[9; 16])
        .expect_err("stale texture updates must be rejected");
    assert!(
        matches!(error, RendererError::StaleHandle { .. }),
        "got {error:?}"
    );

    // Failed creation registered nothing, so valid creation still works.
    let valid = renderer
        .gfx()
        .create_texture(&TextureDescriptor::rgba8_unorm(1, 1), &[1, 2, 3, 4])
        .expect("creation after failures must work");
    assert_ne!(renderer.gfx().get_bindless_slot(valid), None);

    renderer.finish();
}

/// `supports_feature` is the contract vocabulary for backend differences: it
/// must answer exactly what the platform capability table declares, and an
/// unsupported direct UI pass must behave as its documented explicit no-op.
#[test]
#[ignore = "requires a graphics device"]
fn test_contract_capability_table_matches_and_unsupported_ops_are_documented() {
    let mut renderer = ContractRenderer::open_without_api_validation("contract: capabilities");

    for feature in RendererFeature::ALL {
        assert_eq!(
            renderer.gfx().supports_feature(*feature),
            (harness::CAPS.feature_support)(*feature),
            "{} must match the declared capability table",
            feature.name()
        );
    }

    // The capability split is the direct UI pass: Vulkan composites UI
    // through the frame graph, so it reports `DirectUiPass` unsupported and
    // calling it is the documented explicit no-op (it returns nothing and
    // must not composite or panic). On Metal the same call is the supported
    // path and equally must not disturb a frame outside a graph render.
    renderer.gfx().render_ui_pass(UIDrawList::default());

    renderer.finish();
}
