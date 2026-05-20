//! Error types for katla_gfx.
//!
//! This module provides unified error handling for the Vulkan renderer,
//! wrapping `vk::Result` and other error types to avoid exposing raw
//! Vulkan types in the public API.

use std::fmt;
use std::io;

use crate::render_graph::RenderGraphError;

/// Validation mode controls the level of GPU validation enabled.
///
/// Each mode includes all features from previous modes plus additional checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationMode {
    /// No validation layers enabled.
    #[default]
    Disabled,
    /// Standard validation with synchronization checks.
    Enabled,
    /// GPU-assisted validation in addition to standard validation.
    GpuAssisted,
}

impl ValidationMode {
    /// Returns true if any validation is enabled.
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled | Self::GpuAssisted)
    }

    /// Returns true if GPU-assisted validation is enabled.
    pub fn is_gpu_assisted(&self) -> bool {
        matches!(self, Self::GpuAssisted)
    }
}

#[cfg(feature = "vulkan")]
use crate::vulkan::material::compiler::MaterialError;

/// Unified error type for the renderer.
#[derive(Debug)]
pub enum RendererError {
    #[cfg(feature = "vulkan")]
    VulkanError(String, ash::vk::Result),

    /// IO error (file loading, etc.).
    IoError(io::Error),

    /// Resource not found.
    NotFound(String),

    /// Invalid operation or state.
    InvalidOperation(String),

    /// Initialization failed.
    InitializationFailed(String),

    #[cfg(feature = "vulkan")]
    SwapchainError(String),

    #[cfg(feature = "vulkan")]
    SwapchainOutOfDate,

    /// Resource creation failed.
    ResourceCreationFailed(String),

    /// Render graph error.
    RenderGraphError(RenderGraphError),

    #[cfg(feature = "vulkan")]
    MaterialError(MaterialError),

    /// Exceeded maximum objects per frame limit.
    ObjectLimitExceeded { index: usize, limit: usize },
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "vulkan")]
            RendererError::VulkanError(msg, _) => write!(f, "Vulkan error: {}", msg),
            RendererError::IoError(err) => write!(f, "IO error: {}", err),
            RendererError::NotFound(msg) => write!(f, "Not found: {}", msg),
            RendererError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
            RendererError::InitializationFailed(msg) => {
                write!(f, "Initialization failed: {}", msg)
            }
            #[cfg(feature = "vulkan")]
            RendererError::SwapchainError(msg) => write!(f, "Swapchain error: {}", msg),
            #[cfg(feature = "vulkan")]
            RendererError::SwapchainOutOfDate => write!(f, "Swapchain out of date"),
            RendererError::ResourceCreationFailed(msg) => {
                write!(f, "Resource creation failed: {}", msg)
            }
            RendererError::RenderGraphError(err) => write!(f, "Render graph error: {}", err),
            #[cfg(feature = "vulkan")]
            RendererError::MaterialError(err) => write!(f, "Material error: {}", err),
            RendererError::ObjectLimitExceeded { index, limit } => {
                write!(
                    f,
                    "Instance index {} exceeds MAX_OBJECTS_PER_FRAME ({}). Increase the limit or reduce draw calls per frame.",
                    index, limit
                )
            }
        }
    }
}

impl std::error::Error for RendererError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            #[cfg(feature = "vulkan")]
            RendererError::VulkanError(_, err) => Some(err),
            RendererError::IoError(err) => Some(err),
            RendererError::RenderGraphError(err) => Some(err),
            #[cfg(feature = "vulkan")]
            RendererError::MaterialError(err) => Some(err),
            _ => None,
        }
    }
}

#[cfg(feature = "vulkan")]
impl From<ash::vk::Result> for RendererError {
    fn from(result: ash::vk::Result) -> Self {
        RendererError::VulkanError(format!("{:?}", result), result)
    }
}

impl From<io::Error> for RendererError {
    fn from(error: io::Error) -> Self {
        RendererError::IoError(error)
    }
}

impl From<RenderGraphError> for RendererError {
    fn from(error: RenderGraphError) -> Self {
        RendererError::RenderGraphError(error)
    }
}

#[cfg(feature = "vulkan")]
impl From<MaterialError> for RendererError {
    fn from(error: MaterialError) -> Self {
        RendererError::MaterialError(error)
    }
}

#[cfg(feature = "vulkan")]
impl RendererError {
    pub(crate) fn from_allocation_error(
        resource: &str,
        error: gpu_allocator::AllocationError,
    ) -> Self {
        RendererError::InitializationFailed(format!(
            "Failed to allocate {} memory: {:?}",
            resource, error
        ))
    }
}
