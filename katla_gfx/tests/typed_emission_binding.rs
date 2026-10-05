//! Typed draw emission binding contract tests for issue #98.
//!
//! `DrawCall::emission` refers to its texture by handle; the backend
//! resolves the handle to a binding-table slot only when preparing work.
//! `NONE` and stale handles resolve to 0 — the shaders' no-emission
//! sentinel — so a dead handle can never sample whatever texture now
//! occupies a recycled slot.
//!
//! Device tests need a Vulkan device (`#[ignore]`, run like the other GPU
//! contract suites:
//! `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test typed_emission_binding -- --ignored`).

use std::ffi::CString;

use katla_gfx::{
    MaterialHandle, MeshHandle, TextureHandle, ValidationMode, VulkanRenderer, renderer::DrawCall,
};

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Disabled,
        CString::new("Typed emission binding test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

#[test]
fn test_draw_call_emission_defaults_to_none() {
    let mesh = MeshHandle::from_raw(0, 0);
    let material = MaterialHandle::from_raw(0, 0);

    let draw = DrawCall::new(mesh, material);
    assert!(draw.emission.is_none());

    let instanced = DrawCall::instanced(mesh, material, Vec::new());
    assert!(instanced.emission.is_none());
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_emission_resolution_maps_registered_handle_and_none() {
    let mut renderer = headless_renderer();

    // NONE never resolves to a slot.
    assert_eq!(
        renderer.resolve_emission_texture_slot(TextureHandle::NONE),
        0,
        "NONE emission handle must resolve to the no-emission sentinel"
    );

    // A registered handle resolves to exactly its bindless slot.
    let texture = renderer.create_texture_solid([0, 255, 0, 255]).unwrap();
    assert_eq!(
        renderer.resolve_emission_texture_slot(texture),
        renderer.get_bindless_slot(texture).unwrap()
    );

    renderer.destroy_texture(texture);
    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_stale_emission_handle_never_resolves_to_recycled_slot() {
    let mut renderer = headless_renderer();

    let first = renderer.create_texture_solid([255, 0, 0, 255]).unwrap();
    let destroyed_slot = renderer.get_bindless_slot(first).unwrap();
    renderer.destroy_texture(first);

    // The stale handle falls back to 0, never to the destroyed texture's
    // slot (which stays withheld per #84 retirement).
    assert_eq!(
        renderer.resolve_emission_texture_slot(first),
        0,
        "stale emission handle must resolve to the no-emission sentinel"
    );
    assert_ne!(
        renderer.resolve_emission_texture_slot(first),
        destroyed_slot,
        "a stale emission handle must never resolve to the destroyed texture's slot"
    );

    // A later registration keeps resolving to its own slot.
    let second = renderer.create_texture_solid([0, 0, 255, 255]).unwrap();
    assert_eq!(
        renderer.resolve_emission_texture_slot(second),
        renderer.get_bindless_slot(second).unwrap()
    );

    renderer.destroy_texture(second);
    renderer.destroy();
}
