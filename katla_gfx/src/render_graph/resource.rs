//! Resource types for render graph.

use std::marker::PhantomData;

use crate::texture::ImageFormat;
use ash::vk;

/// Transient resource types for render graph.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphResourceType {
    /// Color attachment for rendering (supports HDR formats).
    ColorAttachment {
        /// Clear value as [R, G, B, A]. None = don't clear.
        clear_value: Option<[f32; 4]>,
    },
    /// Depth-stencil attachment.
    DepthAttachment {
        /// Clear value (0.0 = far, 1.0 = near for reverse Z).
        clear_value: f32,
        /// Whether this depth texture will also be sampled by shaders.
        /// When true, VK_IMAGE_USAGE_SAMPLED_BIT is added to the image usage.
        sampled: bool,
    },
    /// Sampled image for shader reading (textures).
    SampledImage,
}

/// Descriptor for creating a transient resource in the render graph.
#[derive(Clone, Debug)]
pub struct GraphResourceDesc {
    /// Resource name (used for pass read/write declarations).
    pub name: String,
    /// Resource type and parameters.
    pub resource_type: GraphResourceType,
    /// Image format (for texture resources).
    pub format: ImageFormat,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Whether this resource should be resized when the swapchain is recreated.
    /// Fixed-size resources (e.g., shadow atlas at 4096x4096) should set this to false.
    /// Default is true for backwards compatibility.
    pub tracks_swapchain_size: bool,
}

/// Resource state for barrier tracking.
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

impl ResourceState {
    /// Convert to Vulkan pipeline stage flags.
    pub fn to_vk_stage_flags(self) -> vk::PipelineStageFlags {
        match self {
            Self::Undefined => vk::PipelineStageFlags::TOP_OF_PIPE,
            Self::ColorAttachment => vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            Self::DepthStencilAttachment => {
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
            }
            Self::ShaderRead | Self::ShaderWrite => {
                vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER
            }
            Self::TransferSrc | Self::TransferDst => vk::PipelineStageFlags::TRANSFER,
            Self::PresentSrc => vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        }
    }

    /// Convert to Vulkan access flags.
    pub fn to_vk_access_flags(self) -> vk::AccessFlags {
        match self {
            Self::Undefined => vk::AccessFlags::empty(),
            Self::ColorAttachment => vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            Self::DepthStencilAttachment => {
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            }
            Self::ShaderRead => vk::AccessFlags::SHADER_READ,
            Self::ShaderWrite => vk::AccessFlags::SHADER_WRITE,
            Self::TransferSrc => vk::AccessFlags::TRANSFER_READ,
            Self::TransferDst => vk::AccessFlags::TRANSFER_WRITE,
            Self::PresentSrc => vk::AccessFlags::NONE,
        }
    }
}

/// Opaque handle for graph resources (internal use only).
///
/// Generated from string names at build time.
/// Prevents mixing graph resources with external handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphResourceHandle {
    index: u32,
    _marker: PhantomData<*const ()>, // !Send + !Sync
}

impl GraphResourceHandle {
    /// Handle representing no resource.
    pub const NONE: Self = Self {
        index: u32::MAX,
        _marker: PhantomData,
    };

    /// Create a new resource handle.
    pub(crate) fn new(index: u32) -> Self {
        Self {
            index,
            _marker: PhantomData,
        }
    }

    /// Get the underlying index.
    pub fn index(self) -> u32 {
        self.index
    }

    /// Check if this is the NONE handle.
    pub fn is_none(self) -> bool {
        self.index == u32::MAX
    }

    /// Check if this is a valid handle.
    pub fn is_some(self) -> bool {
        self.index != u32::MAX
    }
}

impl Default for GraphResourceHandle {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_none() {
        let handle = GraphResourceHandle::NONE;
        assert!(handle.is_none());
        assert!(!handle.is_some());
        assert_eq!(handle.index(), u32::MAX);
    }

    #[test]
    fn test_handle_creation() {
        let handle = GraphResourceHandle::new(42);
        assert!(!handle.is_none());
        assert!(handle.is_some());
        assert_eq!(handle.index(), 42);
    }

    #[test]
    fn test_handle_copy_clone() {
        let handle = GraphResourceHandle::new(10);
        let copied = handle;
        let cloned = handle;
        assert_eq!(handle, copied);
        assert_eq!(handle, cloned);
    }

    #[test]
    fn test_handle_equality() {
        let h1 = GraphResourceHandle::new(1);
        let h2 = GraphResourceHandle::new(1);
        let h3 = GraphResourceHandle::new(2);

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_handle_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let h1 = GraphResourceHandle::new(1);
        let h2 = GraphResourceHandle::new(1);
        let h3 = GraphResourceHandle::new(2);

        set.insert(h1);
        assert!(set.contains(&h2));
        assert!(!set.contains(&h3));
    }

    #[test]
    fn test_resource_state_default() {
        assert_eq!(ResourceState::default(), ResourceState::Undefined);
    }

    #[test]
    fn test_resource_state_undefined() {
        let state = ResourceState::Undefined;
        assert_eq!(
            state.to_vk_stage_flags(),
            vk::PipelineStageFlags::TOP_OF_PIPE
        );
        assert_eq!(state.to_vk_access_flags(), vk::AccessFlags::empty());
    }

    #[test]
    fn test_resource_state_color_attachment() {
        let state = ResourceState::ColorAttachment;
        assert_eq!(
            state.to_vk_stage_flags(),
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
        );
        assert_eq!(
            state.to_vk_access_flags(),
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
        );
    }

    #[test]
    fn test_resource_state_depth_stencil() {
        let state = ResourceState::DepthStencilAttachment;
        let stages = state.to_vk_stage_flags();
        assert!(stages.contains(vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS));
        assert!(stages.contains(vk::PipelineStageFlags::LATE_FRAGMENT_TESTS));

        let access = state.to_vk_access_flags();
        assert!(access.contains(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ));
        assert!(access.contains(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE));
    }

    #[test]
    fn test_resource_state_shader_read() {
        let state = ResourceState::ShaderRead;
        let stages = state.to_vk_stage_flags();
        assert!(stages.contains(vk::PipelineStageFlags::VERTEX_SHADER));
        assert!(stages.contains(vk::PipelineStageFlags::FRAGMENT_SHADER));
        assert!(stages.contains(vk::PipelineStageFlags::COMPUTE_SHADER));
        assert_eq!(state.to_vk_access_flags(), vk::AccessFlags::SHADER_READ);
    }

    #[test]
    fn test_resource_state_shader_write() {
        let state = ResourceState::ShaderWrite;
        let stages = state.to_vk_stage_flags();
        assert!(stages.contains(vk::PipelineStageFlags::VERTEX_SHADER));
        assert!(stages.contains(vk::PipelineStageFlags::FRAGMENT_SHADER));
        assert!(stages.contains(vk::PipelineStageFlags::COMPUTE_SHADER));
        assert_eq!(state.to_vk_access_flags(), vk::AccessFlags::SHADER_WRITE);
    }

    #[test]
    fn test_resource_state_transfer() {
        let state = ResourceState::TransferSrc;
        assert_eq!(state.to_vk_stage_flags(), vk::PipelineStageFlags::TRANSFER);
        assert_eq!(state.to_vk_access_flags(), vk::AccessFlags::TRANSFER_READ);

        let state = ResourceState::TransferDst;
        assert_eq!(state.to_vk_stage_flags(), vk::PipelineStageFlags::TRANSFER);
        assert_eq!(state.to_vk_access_flags(), vk::AccessFlags::TRANSFER_WRITE);
    }

    #[test]
    fn test_resource_state_present() {
        let state = ResourceState::PresentSrc;
        assert_eq!(
            state.to_vk_stage_flags(),
            vk::PipelineStageFlags::BOTTOM_OF_PIPE
        );
        assert_eq!(state.to_vk_access_flags(), vk::AccessFlags::empty());
    }
}
