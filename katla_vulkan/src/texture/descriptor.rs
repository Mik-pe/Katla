//! Texture descriptor types for creating textures.
//!
//! These types provide a clean public API for texture creation that doesn't
//! expose Vulkan types directly.

use crate::render_graph::types::ImageFormat;
use bitflags::bitflags;

bitflags! {
    /// Usage flags for texture creation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TextureUsage: u32 {
        /// Texture can be sampled in shaders.
        const SAMPLED = 1 << 0;
        /// Texture can be used as transfer destination (for uploads).
        const COPY_DST = 1 << 1;
        /// Texture can be used as storage image (read/write).
        const STORAGE = 1 << 2;
        /// Texture can be used as color attachment.
        const COLOR_ATTACHMENT = 1 << 3;
        /// Texture can be used as depth/stencil attachment.
        const DEPTH_STENCIL_ATTACHMENT = 1 << 4;
    }
}

impl Default for TextureUsage {
    fn default() -> Self {
        TextureUsage::SAMPLED | TextureUsage::COPY_DST
    }
}

/// Descriptor for creating a texture.
///
/// This is a plain data struct that describes texture properties without
/// exposing any Vulkan types.
#[derive(Debug, Clone)]
pub struct TextureDescriptor {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel format.
    pub format: ImageFormat,
    /// Usage flags.
    pub usage: TextureUsage,
    /// Optional debug label.
    pub label: Option<&'static str>,
}

impl Default for TextureDescriptor {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            format: ImageFormat::R8G8B8A8Srgb,
            usage: TextureUsage::default(),
            label: None,
        }
    }
}

impl TextureDescriptor {
    /// Create a new texture descriptor with the given dimensions.
    pub fn new(width: u32, height: u32, format: ImageFormat) -> Self {
        Self {
            width,
            height,
            format,
            usage: TextureUsage::default(),
            label: None,
        }
    }

    /// Create an RGBA8 SRGB texture descriptor.
    pub fn rgba8_srgb(width: u32, height: u32) -> Self {
        Self::new(width, height, ImageFormat::R8G8B8A8Srgb)
    }

    /// Create an RGBA8 UNORM texture descriptor (for linear data like normals).
    pub fn rgba8_unorm(width: u32, height: u32) -> Self {
        Self::new(width, height, ImageFormat::R8G8B8A8Unorm)
    }

    /// Create an R8 UNORM texture descriptor (for single-channel data).
    pub fn r8_unorm(width: u32, height: u32) -> Self {
        Self::new(width, height, ImageFormat::R8Unorm)
    }

    /// Create an RG8 UNORM texture descriptor (for two-channel data).
    pub fn rg8_unorm(width: u32, height: u32) -> Self {
        Self::new(width, height, ImageFormat::Rg8Unorm)
    }

    /// Create an RGBA16 float texture descriptor (for HDR).
    pub fn rgba16_float(width: u32, height: u32) -> Self {
        Self::new(width, height, ImageFormat::R16G16B16A16Sfloat)
    }

    /// Set the usage flags.
    pub fn with_usage(mut self, usage: TextureUsage) -> Self {
        self.usage = usage;
        self
    }

    /// Set the debug label.
    pub fn with_label(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }
}
