//! Core types for render pass configuration.
//!
//! This module provides Katla-native types for describing render pass attachments
//! and their load/store operations.

use crate::texture::ImageFormat;

/// Describes how an attachment is loaded at the beginning of a render pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadOp {
    /// Clear the attachment to a specified value.
    Clear,
    /// Load the previous contents of the attachment.
    Load,
    /// Don't care about the previous contents; contents will be undefined.
    DontCare,
}

/// Describes how an attachment is stored at the end of a render pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreOp {
    /// Store the contents of the attachment.
    Store,
    /// Don't care about the contents; they may be discarded.
    DontCare,
}

#[cfg(feature = "vulkan")]
impl From<LoadOp> for ash::vk::AttachmentLoadOp {
    #[inline]
    fn from(op: LoadOp) -> Self {
        match op {
            LoadOp::Clear => ash::vk::AttachmentLoadOp::CLEAR,
            LoadOp::Load => ash::vk::AttachmentLoadOp::LOAD,
            LoadOp::DontCare => ash::vk::AttachmentLoadOp::DONT_CARE,
        }
    }
}

#[cfg(feature = "vulkan")]
impl From<StoreOp> for ash::vk::AttachmentStoreOp {
    #[inline]
    fn from(op: StoreOp) -> Self {
        match op {
            StoreOp::Store => ash::vk::AttachmentStoreOp::STORE,
            StoreOp::DontCare => ash::vk::AttachmentStoreOp::DONT_CARE,
        }
    }
}

/// Clear value for an attachment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClearValue {
    /// Color clear value (RGBA).
    Color([f32; 4]),
    /// Depth-stencil clear value.
    DepthStencil {
        /// Depth clear value (0.0 - 1.0).
        depth: f32,
        /// Stencil clear value.
        stencil: u32,
    },
}

impl ClearValue {
    /// Create a color clear value with opaque black (0, 0, 0, 1).
    pub const TRANSPARENT_BLACK: Self = ClearValue::Color([0.0, 0.0, 0.0, 0.0]);

    /// Create a color clear value with opaque black (0, 0, 0, 1).
    pub const OPAQUE_BLACK: Self = ClearValue::Color([0.0, 0.0, 0.0, 1.0]);

    /// Create a color clear value with opaque white (1, 1, 1, 1).
    pub const OPAQUE_WHITE: Self = ClearValue::Color([1.0, 1.0, 1.0, 1.0]);

    /// Create a depth clear value with depth=1.0 (far plane), stencil=0.
    pub const DEFAULT_DEPTH: Self = ClearValue::DepthStencil {
        depth: 1.0,
        stencil: 0,
    };

    /// Create a new color clear value.
    pub fn color(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::Color([r, g, b, a])
    }

    /// Create a new depth-stencil clear value.
    pub fn depth_stencil(depth: f32, stencil: u32) -> Self {
        Self::DepthStencil { depth, stencil }
    }
}

#[cfg(feature = "vulkan")]
impl From<ClearValue> for ash::vk::ClearValue {
    #[inline]
    fn from(value: ClearValue) -> Self {
        match value {
            ClearValue::Color([r, g, b, a]) => ash::vk::ClearValue {
                color: ash::vk::ClearColorValue {
                    float32: [r, g, b, a],
                },
            },
            ClearValue::DepthStencil { depth, stencil } => ash::vk::ClearValue {
                depth_stencil: ash::vk::ClearDepthStencilValue { depth, stencil },
            },
        }
    }
}

/// Describes an attachment for a render pass.
#[derive(Debug, Clone)]
pub struct AttachmentInfo {
    /// The format of the attachment.
    pub format: ImageFormat,
    /// How the attachment is loaded at the beginning of the render pass.
    pub load_op: LoadOp,
    /// How the attachment is stored at the end of the render pass.
    pub store_op: StoreOp,
    /// The clear value to use if load_op is LoadOp::Clear.
    pub clear_value: ClearValue,
}

impl AttachmentInfo {
    /// Create a new attachment info.
    pub fn new(
        format: ImageFormat,
        load_op: LoadOp,
        store_op: StoreOp,
        clear_value: ClearValue,
    ) -> Self {
        Self {
            format,
            load_op,
            store_op,
            clear_value,
        }
    }

    /// Create a color attachment that is cleared to a specific color.
    pub fn color_clear(format: ImageFormat, clear_color: [f32; 4]) -> Self {
        Self {
            format,
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::Color(clear_color),
        }
    }

    /// Create a color attachment that loads previous contents.
    pub fn color_load(format: ImageFormat) -> Self {
        Self {
            format,
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
            clear_value: ClearValue::OPAQUE_BLACK,
        }
    }

    /// Create a depth attachment that is cleared to default depth (1.0).
    pub fn depth_clear(format: ImageFormat) -> Self {
        Self {
            format,
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::DEFAULT_DEPTH,
        }
    }

    /// Create a depth attachment that loads previous contents.
    pub fn depth_load(format: ImageFormat) -> Self {
        Self {
            format,
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
            clear_value: ClearValue::DEFAULT_DEPTH,
        }
    }
}

/// Describes the type of memory barrier needed after a render pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BarrierKind {
    /// No barrier needed.
    #[default]
    None,
    /// Fragment shader read barrier (common for textures rendered in this pass).
    FragmentShaderRead,
    /// Color attachment output barrier (for subsequent color blending).
    ColorAttachmentOutput,
    /// Compute shader read/write barrier.
    ComputeReadWrite,
    /// Transfer read barrier (for copying to textures).
    TransferRead,
    /// General barrier for all graphics stages.
    AllGraphics,
}

#[cfg(all(test, feature = "vulkan"))]
mod tests {
    use super::*;

    #[test]
    fn test_load_op_conversion() {
        assert_eq!(
            ash::vk::AttachmentLoadOp::from(LoadOp::Clear),
            ash::vk::AttachmentLoadOp::CLEAR
        );
        assert_eq!(
            ash::vk::AttachmentLoadOp::from(LoadOp::Load),
            ash::vk::AttachmentLoadOp::LOAD
        );
        assert_eq!(
            ash::vk::AttachmentLoadOp::from(LoadOp::DontCare),
            ash::vk::AttachmentLoadOp::DONT_CARE
        );
    }

    #[test]
    fn test_store_op_conversion() {
        assert_eq!(
            ash::vk::AttachmentStoreOp::from(StoreOp::Store),
            ash::vk::AttachmentStoreOp::STORE
        );
        assert_eq!(
            ash::vk::AttachmentStoreOp::from(StoreOp::DontCare),
            ash::vk::AttachmentStoreOp::DONT_CARE
        );
    }

    #[test]
    fn test_clear_value_color() {
        let clear = ClearValue::color(0.5, 0.25, 0.75, 1.0);
        let vk_clear: ash::vk::ClearValue = clear.into();
        unsafe {
            assert_eq!(vk_clear.color.float32, [0.5, 0.25, 0.75, 1.0]);
        }
    }

    #[test]
    fn test_clear_value_depth_stencil() {
        let clear = ClearValue::depth_stencil(0.5, 42);
        let vk_clear: ash::vk::ClearValue = clear.into();
        unsafe {
            assert_eq!(vk_clear.depth_stencil.depth, 0.5);
            assert_eq!(vk_clear.depth_stencil.stencil, 42);
        }
    }

    #[test]
    fn test_clear_value_defaults() {
        match ClearValue::TRANSPARENT_BLACK {
            ClearValue::Color([r, g, b, a]) => {
                assert_eq!([r, g, b, a], [0.0, 0.0, 0.0, 0.0]);
            }
            _ => panic!("Expected Color variant"),
        }

        match ClearValue::OPAQUE_BLACK {
            ClearValue::Color([r, g, b, a]) => {
                assert_eq!([r, g, b, a], [0.0, 0.0, 0.0, 1.0]);
            }
            _ => panic!("Expected Color variant"),
        }

        match ClearValue::OPAQUE_WHITE {
            ClearValue::Color([r, g, b, a]) => {
                assert_eq!([r, g, b, a], [1.0, 1.0, 1.0, 1.0]);
            }
            _ => panic!("Expected Color variant"),
        }

        match ClearValue::DEFAULT_DEPTH {
            ClearValue::DepthStencil { depth, stencil } => {
                assert_eq!(depth, 1.0);
                assert_eq!(stencil, 0);
            }
            _ => panic!("Expected DepthStencil variant"),
        }
    }

    #[test]
    fn test_attachment_info_constructors() {
        let color_clear =
            AttachmentInfo::color_clear(ImageFormat::R8G8B8A8Srgb, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(color_clear.format, ImageFormat::R8G8B8A8Srgb);
        assert_eq!(color_clear.load_op, LoadOp::Clear);
        assert_eq!(color_clear.store_op, StoreOp::Store);

        let color_load = AttachmentInfo::color_load(ImageFormat::R8G8B8A8Srgb);
        assert_eq!(color_load.load_op, LoadOp::Load);
        assert_eq!(color_load.store_op, StoreOp::Store);

        let depth_clear = AttachmentInfo::depth_clear(ImageFormat::D32Sfloat);
        assert_eq!(depth_clear.format, ImageFormat::D32Sfloat);
        assert_eq!(depth_clear.load_op, LoadOp::Clear);

        let depth_load = AttachmentInfo::depth_load(ImageFormat::D32Sfloat);
        assert_eq!(depth_load.load_op, LoadOp::Load);
    }
}

/// Tracks the current usage state of a GPU resource.
///
/// Used by both the Vulkan render graph and Metal frame graph for
/// resource state tracking and barrier generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ResourceState {
    /// Undefined (don't care about contents).
    #[default]
    Undefined,
    /// Color attachment output (render target).
    ColorAttachment,
    /// Depth-stencil read/write.
    DepthStencilAttachment,
    /// Shader read (sampled image or uniform buffer).
    ShaderRead,
    /// Shader write (storage image or storage buffer).
    ShaderWrite,
    /// Transfer source (copy from).
    TransferSrc,
    /// Transfer destination (copy to).
    TransferDst,
    /// Present source (swapchain image).
    PresentSrc,
}
