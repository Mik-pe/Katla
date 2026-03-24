//! Error types for katla_gfx.
//!
//! This module provides unified error handling for the Vulkan renderer,
//! wrapping `vk::Result` and other error types to avoid exposing raw
//! Vulkan types in the public API.

use std::fmt;
use std::io;

use crate::render_graph::RenderGraphError;

/// Unified error type for the Vulkan renderer.
///
/// This enum wraps various error types that can occur during rendering,
/// providing a clean public API without exposing raw `vk::Result`.
#[derive(Debug)]
pub enum RendererError {
    /// Vulkan API error.
    VulkanError(String),

    /// IO error (file loading, etc.).
    IoError(String),

    /// Resource not found.
    NotFound(String),

    /// Invalid operation or state.
    InvalidOperation(String),

    /// Initialization failed.
    InitializationFailed(String),

    /// Swapchain error (acquire, present, etc.).
    SwapchainError(String),

    /// Render graph error.
    RenderGraphError(String),

    /// Exceeded maximum objects per frame limit.
    ObjectLimitExceeded { index: usize, limit: usize },
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::VulkanError(msg) => write!(f, "Vulkan error: {}", msg),
            RendererError::IoError(msg) => write!(f, "IO error: {}", msg),
            RendererError::NotFound(msg) => write!(f, "Not found: {}", msg),
            RendererError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
            RendererError::InitializationFailed(msg) => write!(f, "Initialization failed: {}", msg),
            RendererError::SwapchainError(msg) => write!(f, "Swapchain error: {}", msg),
            RendererError::RenderGraphError(msg) => write!(f, "Render graph error: {}", msg),
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
        None
    }
}

impl From<ash::vk::Result> for RendererError {
    fn from(result: ash::vk::Result) -> Self {
        RendererError::VulkanError(format!("{:?}", result))
    }
}

impl From<io::Error> for RendererError {
    fn from(error: io::Error) -> Self {
        RendererError::IoError(error.to_string())
    }
}

impl From<RenderGraphError> for RendererError {
    fn from(error: RenderGraphError) -> Self {
        RendererError::RenderGraphError(error.to_string())
    }
}
