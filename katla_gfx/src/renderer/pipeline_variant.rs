//! Canonical graphics pipeline variant identity.
//!
//! A material's compilation inputs (`PipelineDescriptor`) plus one concrete
//! render-target configuration identify exactly one compiled pipeline. The
//! same material can therefore serve multiple render-target configurations:
//! each configuration resolves to its own [`PipelineVariantKey`], and the
//! backend caches one native pipeline per key on the material.
//!
//! Both backends derive the same key for the same inputs, so variant
//! identity is logical and backend-neutral; only the cached pipeline object
//! behind a key is native.

use crate::renderer::pipeline_descriptor::PipelineDescriptor;
use crate::texture::ImageFormat;

/// Depth/stencil format every depth-enabled graphics pipeline is compiled
/// for on both backends.
pub const DEFAULT_DEPTH_FORMAT: ImageFormat = ImageFormat::D32SfloatS8Uint;

/// Color format used when neither the descriptor nor the caller resolves one.
const FALLBACK_COLOR_FORMAT: ImageFormat = ImageFormat::B8G8R8A8Srgb;

/// Canonical identity of one compiled graphics pipeline variant.
///
/// The key carries every input that can affect pipeline compatibility or
/// code generation: shader identity and entry points, vertex layout,
/// blend/cull/depth/wireframe state, specialization constants, backend
/// extension options, the resolved color attachment format, the derived
/// depth/stencil format, and the sample count. Two keys are equal exactly
/// when the compiled pipelines are interchangeable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PipelineVariantKey {
    /// Compilation inputs with `color_format` resolved to a concrete format
    /// (never `Auto`).
    descriptor: PipelineDescriptor,
    /// Depth/stencil attachment format, or `None` for depth-free pipelines
    /// (UI, compositing, depth-test-disabled).
    depth_format: Option<ImageFormat>,
    /// Sample count of the target configuration (currently always 1).
    samples: u32,
}

impl PipelineVariantKey {
    /// Resolve a material descriptor against a requested color format.
    ///
    /// A concrete `requested_color` wins over the descriptor's declaration;
    /// `Auto` falls back to the descriptor's declared format, and to
    /// [`FALLBACK_COLOR_FORMAT`] when both are `Auto`. The depth/stencil
    /// format is derived once here so both backends build the variant for
    /// the same attachments: UI layouts, Vulkan compositing options, and
    /// depth-test-disabled state render without a depth attachment;
    /// everything else uses [`DEFAULT_DEPTH_FORMAT`].
    pub fn resolve(descriptor: &PipelineDescriptor, requested_color: ImageFormat) -> Self {
        let mut resolved = descriptor.clone();
        resolved.color_format = match requested_color {
            ImageFormat::Auto => match descriptor.color_format {
                ImageFormat::Auto => FALLBACK_COLOR_FORMAT,
                declared => declared,
            },
            requested => requested,
        };
        Self {
            depth_format: derive_depth_format(&resolved),
            descriptor: resolved,
            samples: 1,
        }
    }

    /// The resolved compilation inputs (concrete `color_format`).
    pub fn descriptor(&self) -> &PipelineDescriptor {
        &self.descriptor
    }

    /// Resolved color attachment format (never `Auto`).
    pub fn color_format(&self) -> ImageFormat {
        self.descriptor.color_format
    }

    /// Derived depth/stencil attachment format (`None` = depth-free pass).
    pub fn depth_format(&self) -> Option<ImageFormat> {
        self.depth_format
    }

    /// Sample count of the target configuration.
    pub fn samples(&self) -> u32 {
        self.samples
    }
}

/// One shared derivation so a key never disagrees with the pipeline built
/// from it.
fn derive_depth_format(descriptor: &PipelineDescriptor) -> Option<ImageFormat> {
    if descriptor.is_ui_layout() || !descriptor.depth.test || descriptor.native.vulkan.compositing {
        None
    } else {
        Some(DEFAULT_DEPTH_FORMAT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::pipeline_descriptor::{DepthState, PipelineDescriptor};

    #[test]
    fn test_same_inputs_resolve_equal_keys() {
        let a = PipelineVariantKey::resolve(
            &PipelineDescriptor::pbr("shaders/pbr.wgsl"),
            ImageFormat::R16G16B16A16Sfloat,
        );
        let b = PipelineVariantKey::resolve(
            &PipelineDescriptor::pbr("shaders/pbr.wgsl"),
            ImageFormat::R16G16B16A16Sfloat,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn test_different_color_formats_resolve_distinct_keys() {
        let hdr = PipelineVariantKey::resolve(
            &PipelineDescriptor::pbr("shaders/pbr.wgsl"),
            ImageFormat::R16G16B16A16Sfloat,
        );
        let ldr = PipelineVariantKey::resolve(
            &PipelineDescriptor::pbr("shaders/pbr.wgsl"),
            ImageFormat::B8G8R8A8Srgb,
        );
        assert_ne!(hdr, ldr);
        assert_eq!(hdr.color_format(), ImageFormat::R16G16B16A16Sfloat);
        assert_eq!(ldr.color_format(), ImageFormat::B8G8R8A8Srgb);
    }

    #[test]
    fn test_requested_format_wins_over_declaration() {
        let declared_hdr = PipelineDescriptor::pbr("shaders/pbr.wgsl")
            .with_color_format(ImageFormat::R16G16B16A16Sfloat);
        let key = PipelineVariantKey::resolve(&declared_hdr, ImageFormat::B8G8R8A8Srgb);
        assert_eq!(key.color_format(), ImageFormat::B8G8R8A8Srgb);
    }

    #[test]
    fn test_auto_resolves_to_declaration_then_fallback() {
        let declared_hdr = PipelineDescriptor::pbr("shaders/pbr.wgsl")
            .with_color_format(ImageFormat::R16G16B16A16Sfloat);
        assert_eq!(
            PipelineVariantKey::resolve(&declared_hdr, ImageFormat::Auto).color_format(),
            ImageFormat::R16G16B16A16Sfloat
        );
        assert_eq!(
            PipelineVariantKey::resolve(
                &PipelineDescriptor::pbr("shaders/pbr.wgsl"),
                ImageFormat::Auto
            )
            .color_format(),
            FALLBACK_COLOR_FORMAT
        );
    }

    #[test]
    fn test_depth_format_follows_declared_state() {
        let depth = PipelineVariantKey::resolve(
            &PipelineDescriptor::pbr("shaders/pbr.wgsl"),
            ImageFormat::R16G16B16A16Sfloat,
        );
        assert_eq!(depth.depth_format(), Some(DEFAULT_DEPTH_FORMAT));

        let no_depth =
            PipelineDescriptor::pbr("shaders/pbr.wgsl").with_depth(DepthState::disabled());
        assert_eq!(
            PipelineVariantKey::resolve(&no_depth, ImageFormat::R16G16B16A16Sfloat).depth_format(),
            None
        );

        let compositing = PipelineDescriptor::pbr("shaders/pbr.wgsl").with_vulkan_compositing(true);
        assert_eq!(
            PipelineVariantKey::resolve(&compositing, ImageFormat::R16G16B16A16Sfloat)
                .depth_format(),
            None
        );

        let ui = PipelineVariantKey::resolve(
            &PipelineDescriptor::ui("shaders/ui.wgsl"),
            ImageFormat::B8G8R8A8Srgb,
        );
        assert_eq!(ui.depth_format(), None);
    }

    #[test]
    fn test_render_state_distinguishes_keys() {
        let opaque = PipelineDescriptor::pbr("shaders/pbr.wgsl");
        let blended = opaque
            .clone()
            .with_blend(crate::renderer::pipeline_descriptor::BlendMode::AlphaBlend);
        let double_sided = opaque.clone().with_cull(crate::pipeline::CullMode::None);
        let wireframe = opaque.clone().with_wireframe(true);

        let base = PipelineVariantKey::resolve(&opaque, ImageFormat::R16G16B16A16Sfloat);
        assert_ne!(
            base,
            PipelineVariantKey::resolve(&blended, ImageFormat::R16G16B16A16Sfloat)
        );
        assert_ne!(
            base,
            PipelineVariantKey::resolve(&double_sided, ImageFormat::R16G16B16A16Sfloat)
        );
        assert_ne!(
            base,
            PipelineVariantKey::resolve(&wireframe, ImageFormat::R16G16B16A16Sfloat)
        );
    }
}
