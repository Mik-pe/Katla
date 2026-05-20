//! Resource types for render graph.

use std::marker::PhantomData;

pub use crate::render_pass::ResourceState;
use crate::texture::ImageFormat;

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

/// Trait for transient texture state tracking.
///
/// Implemented by backend-specific transient texture types.
/// Provides a uniform interface for the render graph to track
/// and update resource states without knowing backend details.
pub trait TransientTextureOps {
    /// Get the current tracked resource state.
    fn state(&self) -> ResourceState;
    /// Update the tracked resource state after a transition.
    fn set_state(&self, state: ResourceState);
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
}
