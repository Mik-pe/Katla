#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    /// Automatic format detection - material will be compiled on-demand
    /// for each format it's used with. This allows a single material to work
    /// with multiple render target formats (e.g., HDR and LDR passes).
    Auto,
    R8G8B8A8Srgb,
    R8G8B8A8Unorm,
    B8G8R8A8Srgb,
    R8Unorm,
    Rg8Unorm,
    R32Sfloat,
    R32Uint,
    /// Four normalized unsigned 16-bit channels.
    R16G16B16A16Unorm,
    R16G16B16A16Sfloat,
    /// BC1 RGBA blocks, eight bytes per 4×4 texels.
    Bc1RgbaUnorm,
    /// BC3 RGBA blocks, sixteen bytes per 4×4 texels.
    Bc3RgbaUnorm,
    D32Sfloat,
    D32SfloatS8Uint,
    D24UnormS8Uint,
}

impl From<ImageFormat> for ash::vk::Format {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::Auto => ash::vk::Format::UNDEFINED,
            ImageFormat::Bc1RgbaUnorm => ash::vk::Format::BC1_RGBA_UNORM_BLOCK,
            ImageFormat::Bc3RgbaUnorm => ash::vk::Format::BC3_UNORM_BLOCK,
            ImageFormat::R8G8B8A8Srgb => ash::vk::Format::R8G8B8A8_SRGB,
            ImageFormat::R8G8B8A8Unorm => ash::vk::Format::R8G8B8A8_UNORM,
            ImageFormat::B8G8R8A8Srgb => ash::vk::Format::B8G8R8A8_SRGB,
            ImageFormat::R8Unorm => ash::vk::Format::R8_UNORM,
            ImageFormat::Rg8Unorm => ash::vk::Format::R8G8_UNORM,
            ImageFormat::R32Sfloat => ash::vk::Format::R32_SFLOAT,
            ImageFormat::R32Uint => ash::vk::Format::R32_UINT,
            ImageFormat::R16G16B16A16Unorm => ash::vk::Format::R16G16B16A16_UNORM,
            ImageFormat::R16G16B16A16Sfloat => ash::vk::Format::R16G16B16A16_SFLOAT,
            ImageFormat::D32Sfloat => ash::vk::Format::D32_SFLOAT,
            ImageFormat::D32SfloatS8Uint => ash::vk::Format::D32_SFLOAT_S8_UINT,
            ImageFormat::D24UnormS8Uint => ash::vk::Format::D24_UNORM_S8_UINT,
        }
    }
}

impl ImageFormat {
    /// Returns bytes per texel for uncompressed formats and bytes per block otherwise.
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            ImageFormat::Auto => 4,
            ImageFormat::Bc1RgbaUnorm => 8,
            ImageFormat::Bc3RgbaUnorm => 16,
            ImageFormat::R8Unorm => 1,
            ImageFormat::Rg8Unorm => 2,
            ImageFormat::R8G8B8A8Srgb
            | ImageFormat::R8G8B8A8Unorm
            | ImageFormat::B8G8R8A8Srgb
            | ImageFormat::R32Sfloat
            | ImageFormat::R32Uint => 4,
            ImageFormat::R16G16B16A16Unorm | ImageFormat::R16G16B16A16Sfloat => 8,
            ImageFormat::D32SfloatS8Uint => 8,
            ImageFormat::D32Sfloat | ImageFormat::D24UnormS8Uint => 4,
        }
    }
}

impl TryFrom<ash::vk::Format> for ImageFormat {
    type Error = ();

    fn try_from(format: ash::vk::Format) -> Result<Self, Self::Error> {
        match format {
            ash::vk::Format::R8G8B8A8_SRGB => Ok(ImageFormat::R8G8B8A8Srgb),
            ash::vk::Format::R8G8B8A8_UNORM => Ok(ImageFormat::R8G8B8A8Unorm),
            ash::vk::Format::B8G8R8A8_SRGB => Ok(ImageFormat::B8G8R8A8Srgb),
            ash::vk::Format::BC1_RGBA_UNORM_BLOCK => Ok(ImageFormat::Bc1RgbaUnorm),
            ash::vk::Format::BC3_UNORM_BLOCK => Ok(ImageFormat::Bc3RgbaUnorm),
            ash::vk::Format::R8_UNORM => Ok(ImageFormat::R8Unorm),
            ash::vk::Format::R8G8_UNORM => Ok(ImageFormat::Rg8Unorm),
            ash::vk::Format::R32_SFLOAT => Ok(ImageFormat::R32Sfloat),
            ash::vk::Format::R32_UINT => Ok(ImageFormat::R32Uint),
            ash::vk::Format::R16G16B16A16_UNORM => Ok(ImageFormat::R16G16B16A16Unorm),
            ash::vk::Format::R16G16B16A16_SFLOAT => Ok(ImageFormat::R16G16B16A16Sfloat),
            ash::vk::Format::D32_SFLOAT => Ok(ImageFormat::D32Sfloat),
            ash::vk::Format::D32_SFLOAT_S8_UINT => Ok(ImageFormat::D32SfloatS8Uint),
            ash::vk::Format::D24_UNORM_S8_UINT => Ok(ImageFormat::D24UnormS8Uint),
            _ => Err(()),
        }
    }
}

impl ImageFormat {
    /// Width and height of one independently uploadable format block.
    pub fn block_extent(self) -> [u32; 2] {
        match self {
            Self::Bc1RgbaUnorm | Self::Bc3RgbaUnorm => [4, 4],
            _ => [1, 1],
        }
    }
    /// Number of bytes stored in one format block.
    pub fn bytes_per_block(self) -> u32 {
        self.bytes_per_pixel()
    }
    /// Formats supported by the automatic filtered mip-generation policy.
    pub fn supports_mip_generation(self) -> bool {
        matches!(
            self,
            Self::R8G8B8A8Srgb
                | Self::R8G8B8A8Unorm
                | Self::B8G8R8A8Srgb
                | Self::R8Unorm
                | Self::Rg8Unorm
                | Self::R16G16B16A16Unorm
                | Self::R16G16B16A16Sfloat
        )
    }

    /// Whether this format contains depth or stencil data.
    pub fn is_depth_stencil(self) -> bool {
        matches!(
            self,
            Self::D32Sfloat | Self::D32SfloatS8Uint | Self::D24UnormS8Uint
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_format_block_sizes_and_native_roundtrip() {
        for (format, block, bytes) in [
            (ImageFormat::Bc1RgbaUnorm, [4, 4], 8),
            (ImageFormat::Bc3RgbaUnorm, [4, 4], 16),
            (ImageFormat::R8Unorm, [1, 1], 1),
            (ImageFormat::Rg8Unorm, [1, 1], 2),
            (ImageFormat::R32Sfloat, [1, 1], 4),
            (ImageFormat::R32Uint, [1, 1], 4),
            (ImageFormat::R16G16B16A16Sfloat, [1, 1], 8),
            (ImageFormat::R16G16B16A16Unorm, [1, 1], 8),
            (ImageFormat::D32SfloatS8Uint, [1, 1], 8),
        ] {
            assert_eq!(format.block_extent(), block);
            assert_eq!(format.bytes_per_block(), bytes);
            assert_eq!(
                ImageFormat::try_from(ash::vk::Format::from(format)),
                Ok(format)
            );
        }
    }
}
