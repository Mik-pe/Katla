use objc2_metal::{
    MTLCompareFunction, MTLIndexType, MTLLoadAction, MTLPixelFormat, MTLStoreAction,
    MTLTextureUsage,
};

use crate::backend::command::IndexType;
use crate::pipeline::CompareOp;
use crate::render_pass::{LoadOp, StoreOp};
use crate::texture::ImageFormat;

pub(crate) fn to_mtl_pixel_format(format: ImageFormat) -> MTLPixelFormat {
    match format {
        ImageFormat::Auto => MTLPixelFormat::BGRA8Unorm_sRGB,
        ImageFormat::R8G8B8A8Srgb => MTLPixelFormat::RGBA8Unorm_sRGB,
        ImageFormat::R8G8B8A8Unorm => MTLPixelFormat::RGBA8Unorm,
        ImageFormat::B8G8R8A8Srgb => MTLPixelFormat::BGRA8Unorm_sRGB,
        ImageFormat::R8Unorm => MTLPixelFormat::R8Unorm,
        ImageFormat::Rg8Unorm => MTLPixelFormat::RG8Unorm,
        ImageFormat::R32Sfloat => MTLPixelFormat::R32Float,
        ImageFormat::R32Uint => MTLPixelFormat::R32Uint,
        ImageFormat::R16G16B16A16Sfloat => MTLPixelFormat::RGBA16Float,
        ImageFormat::D32Sfloat => MTLPixelFormat::Depth32Float,
        ImageFormat::D32SfloatS8Uint => MTLPixelFormat::Depth32Float_Stencil8,
        ImageFormat::D24UnormS8Uint => MTLPixelFormat::Depth24Unorm_Stencil8,
    }
}

pub(crate) fn to_mtl_load_action(op: LoadOp) -> MTLLoadAction {
    match op {
        LoadOp::Clear => MTLLoadAction::Clear,
        LoadOp::Load => MTLLoadAction::Load,
        LoadOp::DontCare => MTLLoadAction::DontCare,
    }
}

pub(crate) fn to_mtl_store_action(op: StoreOp) -> MTLStoreAction {
    match op {
        StoreOp::Store => MTLStoreAction::Store,
        StoreOp::DontCare => MTLStoreAction::DontCare,
    }
}

pub(crate) fn to_mtl_compare_func(op: CompareOp) -> MTLCompareFunction {
    match op {
        CompareOp::Never => MTLCompareFunction::Never,
        CompareOp::Less => MTLCompareFunction::Less,
        CompareOp::Equal => MTLCompareFunction::Equal,
        CompareOp::LessOrEqual => MTLCompareFunction::LessEqual,
        CompareOp::Greater => MTLCompareFunction::Greater,
        CompareOp::NotEqual => MTLCompareFunction::NotEqual,
        CompareOp::GreaterOrEqual => MTLCompareFunction::GreaterEqual,
        CompareOp::Always => MTLCompareFunction::Always,
    }
}

pub(crate) fn to_mtl_index_type(ty: IndexType) -> MTLIndexType {
    match ty {
        IndexType::Uint16 => MTLIndexType::UInt16,
        IndexType::Uint32 => MTLIndexType::UInt32,
        IndexType::Uint8 => MTLIndexType::UInt16,
    }
}

pub(crate) fn to_mtl_texture_usage(usage: crate::texture::TextureUsage) -> MTLTextureUsage {
    let mut result = MTLTextureUsage::empty();
    if usage.contains(crate::texture::TextureUsage::SAMPLED) {
        result |= MTLTextureUsage::ShaderRead;
    }
    if usage.contains(crate::texture::TextureUsage::STORAGE) {
        result |= MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite;
    }
    if usage.contains(crate::texture::TextureUsage::COLOR_ATTACHMENT) {
        result |= MTLTextureUsage::RenderTarget;
    }
    if usage.contains(crate::texture::TextureUsage::DEPTH_STENCIL_ATTACHMENT) {
        result |= MTLTextureUsage::RenderTarget;
    }
    if usage.contains(crate::texture::TextureUsage::COPY_DST) {
        result |= MTLTextureUsage::ShaderWrite;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format_all_variants() {
        let formats = [
            ImageFormat::Auto,
            ImageFormat::R8G8B8A8Srgb,
            ImageFormat::R8G8B8A8Unorm,
            ImageFormat::B8G8R8A8Srgb,
            ImageFormat::R8Unorm,
            ImageFormat::Rg8Unorm,
            ImageFormat::R32Sfloat,
            ImageFormat::R32Uint,
            ImageFormat::R16G16B16A16Sfloat,
            ImageFormat::D32Sfloat,
            ImageFormat::D32SfloatS8Uint,
            ImageFormat::D24UnormS8Uint,
        ];
        for fmt in &formats {
            let mtl = to_mtl_pixel_format(*fmt);
            assert_ne!(
                mtl,
                MTLPixelFormat::Invalid,
                "Format {:?} mapped to Invalid",
                fmt
            );
        }
    }

    #[test]
    fn test_pixel_format_color_formats() {
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::R8G8B8A8Srgb),
            MTLPixelFormat::RGBA8Unorm_sRGB
        );
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::R8G8B8A8Unorm),
            MTLPixelFormat::RGBA8Unorm
        );
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::B8G8R8A8Srgb),
            MTLPixelFormat::BGRA8Unorm_sRGB
        );
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::Auto),
            MTLPixelFormat::BGRA8Unorm_sRGB
        );
    }

    #[test]
    fn test_pixel_format_depth_formats() {
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::D32Sfloat),
            MTLPixelFormat::Depth32Float
        );
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::D32SfloatS8Uint),
            MTLPixelFormat::Depth32Float_Stencil8
        );
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::D24UnormS8Uint),
            MTLPixelFormat::Depth24Unorm_Stencil8
        );
    }

    #[test]
    fn test_pixel_format_single_channel() {
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::R8Unorm),
            MTLPixelFormat::R8Unorm
        );
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::R32Sfloat),
            MTLPixelFormat::R32Float
        );
        assert_eq!(
            to_mtl_pixel_format(ImageFormat::R32Uint),
            MTLPixelFormat::R32Uint
        );
    }

    #[test]
    fn test_load_action_roundtrip() {
        assert_eq!(to_mtl_load_action(LoadOp::Clear), MTLLoadAction::Clear);
        assert_eq!(to_mtl_load_action(LoadOp::Load), MTLLoadAction::Load);
        assert_eq!(
            to_mtl_load_action(LoadOp::DontCare),
            MTLLoadAction::DontCare
        );
    }

    #[test]
    fn test_store_action_roundtrip() {
        assert_eq!(to_mtl_store_action(StoreOp::Store), MTLStoreAction::Store);
        assert_eq!(
            to_mtl_store_action(StoreOp::DontCare),
            MTLStoreAction::DontCare
        );
    }

    #[test]
    fn test_compare_func_all_variants() {
        assert_eq!(
            to_mtl_compare_func(CompareOp::Never),
            MTLCompareFunction::Never
        );
        assert_eq!(
            to_mtl_compare_func(CompareOp::Less),
            MTLCompareFunction::Less
        );
        assert_eq!(
            to_mtl_compare_func(CompareOp::Equal),
            MTLCompareFunction::Equal
        );
        assert_eq!(
            to_mtl_compare_func(CompareOp::LessOrEqual),
            MTLCompareFunction::LessEqual
        );
        assert_eq!(
            to_mtl_compare_func(CompareOp::Greater),
            MTLCompareFunction::Greater
        );
        assert_eq!(
            to_mtl_compare_func(CompareOp::NotEqual),
            MTLCompareFunction::NotEqual
        );
        assert_eq!(
            to_mtl_compare_func(CompareOp::GreaterOrEqual),
            MTLCompareFunction::GreaterEqual
        );
        assert_eq!(
            to_mtl_compare_func(CompareOp::Always),
            MTLCompareFunction::Always
        );
    }

    #[test]
    fn test_index_type_mapping() {
        assert_eq!(to_mtl_index_type(IndexType::Uint16), MTLIndexType::UInt16);
        assert_eq!(to_mtl_index_type(IndexType::Uint32), MTLIndexType::UInt32);
        assert_eq!(to_mtl_index_type(IndexType::Uint8), MTLIndexType::UInt16);
    }

    #[test]
    fn test_texture_usage_flags() {
        use crate::texture::TextureUsage;

        let sampled = to_mtl_texture_usage(TextureUsage::SAMPLED);
        assert!(sampled.contains(MTLTextureUsage::ShaderRead));
        assert!(!sampled.contains(MTLTextureUsage::ShaderWrite));
        assert!(!sampled.contains(MTLTextureUsage::RenderTarget));

        let storage = to_mtl_texture_usage(TextureUsage::STORAGE);
        assert!(storage.contains(MTLTextureUsage::ShaderRead));
        assert!(storage.contains(MTLTextureUsage::ShaderWrite));

        let color = to_mtl_texture_usage(TextureUsage::COLOR_ATTACHMENT);
        assert!(color.contains(MTLTextureUsage::RenderTarget));

        let depth = to_mtl_texture_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
        assert!(depth.contains(MTLTextureUsage::RenderTarget));

        let combined = to_mtl_texture_usage(TextureUsage::SAMPLED | TextureUsage::COLOR_ATTACHMENT);
        assert!(combined.contains(MTLTextureUsage::ShaderRead));
        assert!(combined.contains(MTLTextureUsage::RenderTarget));
    }
}
