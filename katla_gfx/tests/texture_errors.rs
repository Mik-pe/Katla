//! Typed texture errors for issue #99.
//!
//! Texture creation and upload fail with structured errors carrying the
//! expected vs actual context — never a silent placeholder, a panic, or a
//! WARN-and-pretend-success. Failed operations leave previous valid state
//! intact: a rejected creation inserts nothing, a rejected upload changes
//! nothing, and a stale handle never aliases a live resource.
//!
//! Pure validation tests run everywhere; device tests need a Vulkan device
//! (`#[ignore]`, run like the other GPU contract tests).

use std::ffi::CString;

use katla_gfx::texture::ImageFormat;
use katla_gfx::{RendererError, TextureDescriptor, ValidationMode, VulkanRenderer};

// ---------------------------------------------------------------------------
// Pure tests (no device)
// ---------------------------------------------------------------------------

#[test]
fn test_validate_data_accepts_exact_and_empty() {
    let desc = TextureDescriptor::rgba8_srgb(4, 4);
    assert_eq!(desc.expected_bytes(), Some(64));
    assert!(desc.validate_data(64).is_ok());
    // Empty data creates the texture uninitialized for later upload.
    assert!(desc.validate_data(0).is_ok());
}

#[test]
fn test_validate_data_rejects_mismatch_with_context() {
    let desc = TextureDescriptor::rgba8_srgb(4, 4);
    let error = desc.validate_data(4).unwrap_err();
    match &error {
        RendererError::InvalidDescriptor { resource, reason } => {
            assert_eq!(resource, "texture");
            assert!(reason.contains("64"), "reason names expected: {reason}");
            assert!(reason.contains('4'), "reason names actual: {reason}");
        }
        other => panic!("expected InvalidDescriptor, got {other:?}"),
    }
    // No string parsing needed by callers: the structure carries the facts.
    assert!(error.to_string().contains("64"));
}

#[test]
fn test_validate_data_rejects_zero_extent() {
    let desc = TextureDescriptor::new(0, 4, ImageFormat::R8G8B8A8Srgb);
    let error = desc.validate_data(0).unwrap_err();
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );
}

#[test]
fn test_error_display_needs_no_parsing() {
    let failures = [
        RendererError::InvalidDescriptor {
            resource: "texture".to_string(),
            reason: "4x4 R8G8B8A8Srgb: expected 64 bytes, got 4".to_string(),
        },
        RendererError::AllocationFailed {
            resource: "bindless texture slot".to_string(),
            reason: "4096/4096 slots used".to_string(),
        },
        RendererError::UploadFailed {
            resource: "texture".to_string(),
            expected_bytes: 64,
            actual_bytes: 4,
            detail: "4x4 R8G8B8A8Srgb".to_string(),
        },
        RendererError::StaleHandle {
            resource: "texture".to_string(),
            detail: "TextureHandle(7) in update_texture".to_string(),
        },
    ];
    for error in &failures {
        let text = error.to_string();
        assert!(!text.is_empty());
        assert!(text.contains("texture") || text.contains("bindless"));
    }
    // Structured context survives without parsing the message.
    match &failures[2] {
        RendererError::UploadFailed {
            expected_bytes,
            actual_bytes,
            ..
        } => {
            assert_eq!((*expected_bytes, *actual_bytes), (64, 4));
        }
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Device tests (ignored without a GPU)
// ---------------------------------------------------------------------------

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Disabled,
        CString::new("Texture error test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn rgba8_2x2() -> (TextureDescriptor, Vec<u8>) {
    (TextureDescriptor::rgba8_srgb(2, 2), vec![255u8; 2 * 2 * 4])
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_create_texture_rejects_mismatched_data() {
    let mut renderer = headless_renderer();

    let (desc, _) = rgba8_2x2();
    let error = renderer.create_texture(&desc, &[1, 2, 3, 4]).unwrap_err();
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );

    // Previous valid state intact: a good creation still works afterwards,
    // and the failed one inserted nothing observable.
    let (desc, data) = rgba8_2x2();
    let handle = renderer.create_texture(&desc, &data).unwrap();
    assert!(renderer.get_bindless_slot(handle).is_some());

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_update_texture_rejects_size_mismatch() {
    use katla_gfx::GpuRenderer;
    let mut renderer = headless_renderer();
    let (desc, data) = rgba8_2x2();
    let handle = renderer.create_texture(&desc, &data).unwrap();

    let error = renderer.update_texture(handle, &[9, 9]).unwrap_err();
    match error {
        RendererError::UploadFailed {
            expected_bytes,
            actual_bytes,
            ..
        } => assert_eq!((expected_bytes, actual_bytes), (16, 2)),
        other => panic!("expected UploadFailed, got {other:?}"),
    }

    // The texture still holds the original upload: a correct update works.
    renderer.update_texture(handle, &data).unwrap();

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_update_texture_stale_handle_fails_typed() {
    use katla_gfx::GpuRenderer;
    let mut renderer = headless_renderer();
    let (desc, data) = rgba8_2x2();
    let handle = renderer.create_texture(&desc, &data).unwrap();
    renderer.destroy_texture(handle);

    let error = renderer.update_texture(handle, &data).unwrap_err();
    assert!(
        matches!(error, RendererError::StaleHandle { .. }),
        "destroyed handle must not alias a live resource, got {error:?}"
    );

    renderer.destroy();
}
