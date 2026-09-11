//! Generational handle lifecycle contract tests for issue #83.
//!
//! Every registry-backed resource class must reject stale handles after
//! destroy → slot reuse: lookups return `None`, updates fail typed (or are
//! observable no-ops), and double-destroy can never destroy the replacement
//! occupying the recycled slot.
//!
//! Emitter-pool semantics are pure-tested in `katla_gfx::particles`; the
//! suites here exercise the real Vulkan registries through public APIs.
//! Device tests need a Vulkan device (`#[ignore]`, run like the other GPU
//! contract suites:
//! `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test generational_handles -- --ignored`).

mod common;

use std::ffi::CString;

use common::create_headless_context;
use katla_gfx::particles::{EmitterConfig, GlobalParticleSystem};
use katla_gfx::{
    GpuRenderer, MeshHandle, PrimitiveTopology, RendererError, ValidationMode, VertexPBR,
    VulkanRenderer,
};
use std::rc::Rc;

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Disabled,
        CString::new("Generational handle test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn triangle() -> (Vec<VertexPBR>, Vec<u32>) {
    (
        vec![
            VertexPBR::from_position([-0.5, -0.5, 0.0]),
            VertexPBR::from_position([0.5, -0.5, 0.0]),
            VertexPBR::from_position([0.0, 0.5, 0.0]),
        ],
        vec![0, 1, 2],
    )
}

fn create_triangle(renderer: &mut VulkanRenderer) -> MeshHandle {
    let (vertices, indices) = triangle();
    renderer
        .create_mesh(&vertices, &indices, PrimitiveTopology::TriangleList)
        .unwrap()
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_mesh_stale_handle_rejected_after_slot_reuse() {
    let mut renderer = headless_renderer();
    let (vertices, indices) = triangle();

    let first = create_triangle(&mut renderer);
    assert_eq!(renderer.mesh_vertex_count(first), Some(3));

    renderer.destroy_mesh(first);

    // Stale queries and updates reject instead of aliasing.
    assert_eq!(renderer.mesh_vertex_count(first), None);
    assert_eq!(renderer.mesh_index_count(first), None);
    let blob = bytemuck::cast_slice(&vertices);
    let result = renderer.update_mesh_dynamic(first, blob, 3, &indices);
    assert!(matches!(result, Err(RendererError::StaleHandle { .. })));

    // Double-destroy is harmless.
    renderer.destroy_mesh(first);

    // The slot is reused under a new generation.
    let second = create_triangle(&mut renderer);
    assert_eq!(second.index(), first.index());
    assert_ne!(second, first);

    // The replacement answers; the stale handle still does not.
    assert_eq!(renderer.mesh_vertex_count(second), Some(3));
    assert_eq!(renderer.mesh_vertex_count(first), None);
    assert!(renderer.asset_registry.get_mesh(first).is_none());
    assert!(renderer.asset_registry.get_mesh(second).is_some());

    // Stale operations must not touch or destroy the replacement.
    let result = renderer.update_mesh_dynamic(first, blob, 3, &indices);
    assert!(matches!(result, Err(RendererError::StaleHandle { .. })));
    renderer.destroy_mesh(first);
    assert_eq!(
        renderer.mesh_vertex_count(second),
        Some(3),
        "stale destroy must not destroy the replacement"
    );
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_texture_stale_handle_rejected_after_slot_reuse() {
    let mut renderer = headless_renderer();

    let first = renderer.create_texture_solid([255, 0, 0, 255]).unwrap();
    assert!(renderer.get_bindless_slot(first).is_some());

    renderer.destroy_texture(first);

    // Stale update fails typed and the bindless registration is gone, so the
    // stale handle cannot resolve to any slot (old or reused).
    let result = renderer.update_texture(first, &[0, 255, 0, 255]);
    assert!(matches!(result, Err(RendererError::StaleHandle { .. })));
    assert_eq!(renderer.get_bindless_slot(first), None);

    // Double-destroy is harmless.
    renderer.destroy_texture(first);

    // Reuse the slot under a new generation.
    let second = renderer.create_texture_solid([0, 0, 255, 255]).unwrap();
    assert_eq!(second.index(), first.index());
    assert_ne!(second, first);
    assert!(renderer.get_bindless_slot(second).is_some());
    // The stale handle still resolves to nothing, not to the replacement's slot.
    assert_eq!(renderer.get_bindless_slot(first), None);

    // Stale destroy cannot destroy the replacement.
    renderer.destroy_texture(first);
    let result = renderer.update_texture(second, &[255, 255, 0, 255]);
    assert!(
        result.is_ok(),
        "replacement survives stale destroy: {result:?}"
    );
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_skeleton_stale_handle_rejected_after_slot_reuse() {
    let mut renderer = headless_renderer();
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];

    let first = renderer.create_skeleton(4).unwrap();
    assert!(renderer.get_skeleton_descriptor(first).is_some());

    renderer.destroy_skeleton(first);
    assert!(renderer.get_skeleton_descriptor(first).is_none());

    // Stale update is a safe no-op; double-destroy is harmless.
    renderer.update_skeleton(first, &[identity; 4]);
    renderer.destroy_skeleton(first);

    let second = renderer.create_skeleton(6).unwrap();
    assert_eq!(second.index(), first.index());
    assert_ne!(second, first);
    assert!(renderer.get_skeleton_descriptor(second).is_some());
    assert!(renderer.get_skeleton_descriptor(first).is_none());

    renderer.destroy_skeleton(first);
    assert!(
        renderer.get_skeleton_descriptor(second).is_some(),
        "stale destroy must not destroy the replacement"
    );
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_material_stale_handle_rejected_after_slot_reuse() {
    use katla_gfx::{GpuRenderer, PipelineDescriptor};

    let mut renderer = headless_renderer();
    let shaders = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders");

    let compile = |renderer: &mut VulkanRenderer| {
        renderer
            .compile_material(&PipelineDescriptor::ui(
                shaders.join("ui/ui.wgsl").to_string_lossy().into_owned(),
            ))
            .unwrap()
    };

    let first = compile(&mut renderer);
    assert!(renderer.asset_registry.get_material(first).is_some());

    renderer.destroy_material(first);
    assert!(renderer.asset_registry.get_material(first).is_none());

    // Double-destroy is harmless.
    renderer.destroy_material(first);

    let second = compile(&mut renderer);
    assert_eq!(second.index(), first.index());
    assert_ne!(second, first);

    // The replacement is live under its handle and unreachable via the stale one.
    assert!(renderer.asset_registry.get_material(second).is_some());
    assert!(renderer.asset_registry.get_material(first).is_none());

    // Stale pipeline replacement (hot-reload path) must not touch the replacement.
    renderer.destroy_material(first);
    assert!(
        renderer.asset_registry.get_material(second).is_some(),
        "stale destroy must not destroy the replacement"
    );
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_emitter_stale_handle_rejected_after_slot_reuse() {
    let context = Rc::new(create_headless_context(false));
    let mut system = GlobalParticleSystem::new(&context, 1024).unwrap();

    let config = EmitterConfig {
        emit_rate: 10.0,
        ..Default::default()
    };
    let first = system.create_emitter(config).unwrap();
    assert!(system.burst(first, 5).is_ok());

    system.destroy_emitter(first, false);
    // Stale burst fails typed.
    assert!(system.burst(first, 5).is_err());

    // Double-destroy harmless.
    system.destroy_emitter(first, false);

    let second = system.create_emitter(config).unwrap();
    assert_eq!(second.index(), first.index());
    assert_ne!(second, first);
    assert!(system.burst(second, 3).is_ok());
    assert!(system.burst(first, 3).is_err());

    // Stale destroy must not destroy the replacement.
    system.destroy_emitter(first, false);
    assert!(
        system.burst(second, 3).is_ok(),
        "stale destroy must not destroy the replacement"
    );
}
