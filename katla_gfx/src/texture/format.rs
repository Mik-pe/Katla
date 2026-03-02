use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    R8G8B8A8Srgb,
    R8G8B8A8Unorm,
    B8G8R8A8Srgb,
    R8Unorm,
    Rg8Unorm,
    R32Sfloat,
    R16G16B16A16Sfloat,
    D32Sfloat,
    D32SfloatS8Uint,
    D24UnormS8Uint,
}

impl From<ImageFormat> for vk::Format {
    fn from(format: ImageFormat) -> Self {
        match format {
            ImageFormat::R8G8B8A8Srgb => vk::Format::R8G8B8A8_SRGB,
            ImageFormat::R8G8B8A8Unorm => vk::Format::R8G8B8A8_UNORM,
            ImageFormat::B8G8R8A8Srgb => vk::Format::B8G8R8A8_SRGB,
            ImageFormat::R8Unorm => vk::Format::R8_UNORM,
            ImageFormat::Rg8Unorm => vk::Format::R8G8_UNORM,
            ImageFormat::R32Sfloat => vk::Format::R32_SFLOAT,
            ImageFormat::R16G16B16A16Sfloat => vk::Format::R16G16B16A16_SFLOAT,
            ImageFormat::D32Sfloat => vk::Format::D32_SFLOAT,
            ImageFormat::D32SfloatS8Uint => vk::Format::D32_SFLOAT_S8_UINT,
            ImageFormat::D24UnormS8Uint => vk::Format::D24_UNORM_S8_UINT,
        }
    }
}
