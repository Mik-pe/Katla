//! Error types for katla_gfx.
//!
//! This module provides unified error handling for the Vulkan renderer,
//! wrapping `vk::Result` and other error types to avoid exposing raw
//! Vulkan types in the public API.

use std::fmt;
use std::io;

use ash::vk;

use crate::render_graph::RenderGraphError;
use crate::vulkan::material::compiler::MaterialError;

/// Unified error type for the Vulkan renderer.
///
/// This enum wraps various error types that can occur during rendering,
/// providing a clean public API without exposing raw `vk::Result`.
#[derive(Debug)]
pub enum RendererError {
    /// Vulkan API error with context message and source result.
    VulkanError(String, vk::Result),

    /// IO error (file loading, etc.).
    IoError(io::Error),

    /// Resource not found.
    NotFound(String),

    /// Invalid operation or state.
    InvalidOperation(String),

    /// Initialization failed.
    InitializationFailed(String),

    /// Swapchain error (acquire, present, etc.).
    SwapchainError(String),

    /// Swapchain is out of date or suboptimal and needs recreation.
    /// The caller should recreate the swapchain and retry.
    SwapchainOutOfDate,

    /// Render graph error.
    RenderGraphError(RenderGraphError),

    /// Material compilation error.
    MaterialError(MaterialError),

    /// Exceeded maximum objects per frame limit.
    ObjectLimitExceeded { index: usize, limit: usize },
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::VulkanError(msg, _) => write!(f, "Vulkan error: {}", msg),
            RendererError::IoError(err) => write!(f, "IO error: {}", err),
            RendererError::NotFound(msg) => write!(f, "Not found: {}", msg),
            RendererError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
            RendererError::InitializationFailed(msg) => {
                write!(f, "Initialization failed: {}", msg)
            }
            RendererError::SwapchainError(msg) => write!(f, "Swapchain error: {}", msg),
            RendererError::SwapchainOutOfDate => write!(f, "Swapchain out of date"),
            RendererError::RenderGraphError(err) => write!(f, "Render graph error: {}", err),
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
            RendererError::VulkanError(_, err) => Some(err),
            RendererError::IoError(err) => Some(err),
            RendererError::RenderGraphError(err) => Some(err),
            RendererError::MaterialError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<vk::Result> for RendererError {
    fn from(result: vk::Result) -> Self {
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

impl From<MaterialError> for RendererError {
    fn from(error: MaterialError) -> Self {
        RendererError::MaterialError(error)
    }
}

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
