//! Texture descriptor types for creating textures.
//!
//! These types provide a clean public API for texture creation that doesn't
//! expose Vulkan types directly.

use bitflags::bitflags;

use crate::texture::ImageFormat;

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
    /// Depth slices (greater than one creates a 3D texture).
    pub depth: u32,
    /// Array layers; 3D textures must use one layer.
    pub array_layers: u32,
    /// Allocated mip levels.
    pub mip_levels: u32,
    /// Generate the mip chain after a complete base-level upload.
    pub generate_mips: bool,
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
            depth: 1,
            array_layers: 1,
            mip_levels: 1,
            generate_mips: false,
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
            depth: 1,
            array_layers: 1,
            mip_levels: 1,
            generate_mips: false,
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

    /// Bytes for the tightly packed base mip in the first array layer.
    /// Returns `None` when the dimensions overflow.
    pub fn expected_bytes(&self) -> Option<usize> {
        let [bw, bh] = self.format.block_extent();
        (self.width.div_ceil(bw) as usize)
            .checked_mul(self.height.div_ceil(bh) as usize)?
            .checked_mul(self.depth as usize)?
            .checked_mul(self.format.bytes_per_block() as usize)
    }

    /// Validate pixel data against this descriptor without touching the GPU.
    ///
    /// Empty data is legitimate: it creates the texture uninitialized for
    /// later upload (render targets). Non-empty data with the wrong length
    /// fails with [`crate::error::RendererError::InvalidDescriptor`], as do
    /// invalid extent, mip count or array/3D combinations — never a silent mis-sized texture.
    pub fn validate_data(&self, data_len: usize) -> Result<(), crate::error::RendererError> {
        let Some(expected) = self.expected_bytes() else {
            return Err(crate::error::RendererError::InvalidDescriptor {
                resource: "texture".to_string(),
                reason: format!(
                    "{}x{} {:?}: dimensions overflow",
                    self.width, self.height, self.format
                ),
            });
        };
        if self.width == 0
            || self.height == 0
            || self.depth == 0
            || self.array_layers == 0
            || self.mip_levels == 0
            || self.mip_levels > 32 - self.width.max(self.height).max(self.depth).leading_zeros()
            || (self.depth > 1 && self.array_layers != 1)
        {
            return Err(crate::error::RendererError::InvalidDescriptor {
                resource: "texture".to_string(),
                reason: format!(
                    "{}x{} {:?}: invalid extent, mip count or array/3D combination",
                    self.width, self.height, self.format
                ),
            });
        }
        let depth = self.format.is_depth_stencil();
        let compressed = self.format.block_extent() != [1, 1];
        if self.format == ImageFormat::Auto
            || (depth
                && (data_len != 0
                    || self
                        .usage
                        .intersects(TextureUsage::COLOR_ATTACHMENT | TextureUsage::STORAGE)))
            || (!depth && self.usage.contains(TextureUsage::DEPTH_STENCIL_ATTACHMENT))
            || (compressed
                && (self.depth != 1
                    || self
                        .usage
                        .intersects(TextureUsage::COLOR_ATTACHMENT | TextureUsage::STORAGE)))
            || (self.generate_mips
                && (depth
                    || compressed
                    || self.array_layers != 1
                    || !self.format.supports_mip_generation()))
        {
            return Err(crate::error::RendererError::InvalidDescriptor {
                resource: self.label.unwrap_or("texture").into(),
                reason: format!(
                    "{:?} {}x{}x{} layers {} mips {}: unsupported format, upload, usage or mip-generation policy",
                    self.format,
                    self.width,
                    self.height,
                    self.depth,
                    self.array_layers,
                    self.mip_levels
                ),
            });
        }
        if data_len != 0 && data_len != expected {
            return Err(crate::error::RendererError::InvalidDescriptor {
                resource: "texture".to_string(),
                reason: format!(
                    "{}x{} {:?}: expected {expected} bytes, got {data_len}",
                    self.width, self.height, self.format
                ),
            });
        }
        Ok(())
    }
}
