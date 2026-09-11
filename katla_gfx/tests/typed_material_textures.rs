//! Typed material texture binding contract tests for issue #98.
//!
//! Materials refer to textures by handle per named role; backends resolve
//! handles to binding-table slots only when preparing/encoding work. `NONE`
//! and stale handles resolve to the role's default texture slot, so a dead
//! handle can never sample whatever texture now occupies a recycled slot.
//!
//! Device tests need a Vulkan device (`#[ignore]`, run like the other GPU
//! contract suites:
//! `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test typed_material_textures -- --ignored`).

use std::ffi::CString;

use katla_gfx::texture::ImageFormat;
use katla_gfx::{
    GpuRenderer, MaterialHandle, MaterialOptions, MaterialTextures, ValidationMode, VertexType,
    VulkanRenderer,
};

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Disabled,
        CString::new("Typed material textures test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn compile_ui_material(renderer: &mut VulkanRenderer) -> MaterialHandle {
    let shaders = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/shaders");
    renderer
        .compile_material(
            shaders.join("ui/ui.wgsl"),
            MaterialOptions {
                vertex_type: VertexType::Ui,
                color_format: ImageFormat::B8G8R8A8Srgb,
                depth_test: false,
                alpha_blended: true,
                double_sided: true,
                ..Default::default()
            },
        )
        .unwrap()
}

#[test]
fn test_default_material_textures_are_all_none() {
    let textures = MaterialTextures::default();
    assert!(textures.albedo.is_none());
    assert!(textures.normal.is_none());
    assert!(textures.metallic_roughness.is_none());
    assert!(textures.occlusion.is_none());
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_resolver_maps_handles_and_falls_back_per_role() {
    let mut renderer = headless_renderer();

    let material = compile_ui_material(&mut renderer);

    // Default material: every role falls back to its default texture slot.
    assert_eq!(
        renderer.resolve_material_texture_slots(material),
        [0, 1, 2, 3]
    );

    // Bind one handle per role; the resolver must return exactly the
    // registered slots.
    let albedo = renderer.create_texture_solid([255, 0, 0, 255]).unwrap();
    let normal = renderer.create_texture_solid([0, 255, 0, 255]).unwrap();
    let mr = renderer.create_texture_solid([0, 0, 255, 255]).unwrap();
    let ao = renderer.create_texture_solid([255, 255, 0, 255]).unwrap();
    renderer.set_material_textures(
        material,
        MaterialTextures {
            albedo,
            normal,
            metallic_roughness: mr,
            occlusion: ao,
        },
    );
    let expected = [
        renderer.get_bindless_slot(albedo).unwrap(),
        renderer.get_bindless_slot(normal).unwrap(),
        renderer.get_bindless_slot(mr).unwrap(),
        renderer.get_bindless_slot(ao).unwrap(),
    ];
    assert_eq!(renderer.resolve_material_texture_slots(material), expected);

    // A stale handle must fall back to the role default, never to the
    // destroyed texture's slot (which stays withheld per #84 retirement).
    let destroyed_slot = renderer.get_bindless_slot(albedo).unwrap();
    renderer.destroy_texture(albedo);
    let resolved = renderer.resolve_material_texture_slots(material);
    assert_eq!(
        resolved[0], 0,
        "stale albedo handle must resolve to the default albedo slot"
    );
    assert_ne!(
        resolved[0], destroyed_slot,
        "a stale handle must never resolve to the destroyed texture's slot"
    );
    // Other roles are unaffected.
    assert_eq!(resolved[1], expected[1]);
    assert_eq!(resolved[2], expected[2]);
    assert_eq!(resolved[3], expected[3]);

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_shared_texture_resolves_identically_across_materials() {
    let mut renderer = headless_renderer();

    let first = compile_ui_material(&mut renderer);
    let second = compile_ui_material(&mut renderer);

    let shared = renderer.create_texture_solid([255, 0, 255, 255]).unwrap();
    let textures = MaterialTextures {
        albedo: shared,
        ..Default::default()
    };
    renderer.set_material_textures(first, textures);
    renderer.set_material_textures(second, textures);

    let first_slots = renderer.resolve_material_texture_slots(first);
    let second_slots = renderer.resolve_material_texture_slots(second);
    assert_eq!(
        first_slots, second_slots,
        "materials sharing a texture handle must resolve to the same slot"
    );
    assert_eq!(first_slots[0], renderer.get_bindless_slot(shared).unwrap());

    renderer.destroy();
}
