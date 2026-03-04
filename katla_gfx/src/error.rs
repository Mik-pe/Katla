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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_error_display() {
        let err = RendererError::VulkanError("ERROR_DEVICE_LOST".to_string());
        assert!(err.to_string().contains("Vulkan error"));

        let err = RendererError::NotFound("texture.png".to_string());
        assert!(err.to_string().contains("Not found"));
    }

    #[test]
    fn test_from_vk_result() {
        let err = RendererError::from(ash::vk::Result::ERROR_DEVICE_LOST);
        assert!(matches!(err, RendererError::VulkanError(_)));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err = RendererError::from(io_err);
        assert!(matches!(err, RendererError::IoError(_)));
    }
}
