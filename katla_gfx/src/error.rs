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

use crate::vulkan::material::compiler::MaterialError;

/// Native details captured when a submitted GPU command buffer fails.
#[derive(Debug)]
pub struct GpuExecutionFailure {
    pub backend: &'static str,
    pub label: String,
    pub status: String,
    pub code: Option<i64>,
    pub domain: Option<String>,
    pub description: Option<String>,
    /// Per-encoder execution status recorded by Metal when the command buffer
    /// was created with encoder-execution diagnostics. Empty when the feature
    /// was off or Metal recorded no encoder info.
    pub encoders: Vec<GpuEncoderDiagnostic>,
}

/// Snapshot of one encoder's terminal state inside a failed command buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuEncoderDiagnostic {
    pub label: String,
    pub error_state: String,
    pub debug_signposts: Vec<String>,
}

impl GpuEncoderDiagnostic {
    /// True when Metal marked this encoder as faulted.
    pub fn is_faulted(&self) -> bool {
        self.error_state == "Faulted"
    }
}

/// Unified error type for the renderer.
#[derive(Debug)]
pub enum RendererError {
    VulkanError(String, ash::vk::Result),

    /// IO error (file loading, etc.).
    IoError(io::Error),

    /// Resource not found.
    NotFound(String),

    /// Invalid operation or state.
    InvalidOperation(String),

    /// A required GPU/backend feature is not available in the current environment.
    UnsupportedFeature(String),

    /// Initialization failed.
    InitializationFailed(String),

    SwapchainError(String),

    SwapchainOutOfDate,

    /// Resource creation failed.
    ResourceCreationFailed(String),

    /// Render graph error.
    RenderGraphError(RenderGraphError),

    MaterialError(MaterialError),

    /// A submitted GPU command buffer reached a terminal failure state.
    GpuExecutionFailed(Box<GpuExecutionFailure>),

    /// Exceeded maximum objects per frame limit.
    ObjectLimitExceeded {
        index: usize,
        limit: usize,
    },
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::VulkanError(msg, _) => write!(f, "Vulkan error: {}", msg),
            RendererError::IoError(err) => write!(f, "IO error: {}", err),
            RendererError::NotFound(msg) => write!(f, "Not found: {}", msg),
            RendererError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
            RendererError::UnsupportedFeature(msg) => {
                write!(f, "Unsupported GPU feature: {}", msg)
            }
            RendererError::InitializationFailed(msg) => {
                write!(f, "Initialization failed: {}", msg)
            }
            RendererError::SwapchainError(msg) => write!(f, "Swapchain error: {}", msg),
            RendererError::SwapchainOutOfDate => write!(f, "Swapchain out of date"),
            RendererError::ResourceCreationFailed(msg) => {
                write!(f, "Resource creation failed: {}", msg)
            }
            RendererError::RenderGraphError(err) => write!(f, "Render graph error: {}", err),
            RendererError::MaterialError(err) => write!(f, "Material error: {}", err),
            RendererError::GpuExecutionFailed(details) => {
                let GpuExecutionFailure {
                    backend,
                    label,
                    status,
                    code,
                    domain,
                    description,
                    encoders,
                } = details.as_ref();
                write!(
                    f,
                    "{} GPU execution failed for command buffer '{}' (status={}",
                    backend, label, status
                )?;
                if let Some(code) = code {
                    write!(f, ", code={code}")?;
                }
                if let Some(domain) = domain {
                    write!(f, ", domain={domain}")?;
                }
                write!(f, ")")?;
                if let Some(description) = description {
                    write!(f, ": {description}")?;
                }
                for encoder in encoders {
                    write!(
                        f,
                        "\n  encoder '{}' (state={})",
                        encoder.label, encoder.error_state
                    )?;
                    for signpost in &encoder.debug_signposts {
                        write!(f, "\n    signpost: {signpost}")?;
                    }
                }
                Ok(())
            }
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

impl From<MaterialError> for RendererError {
    fn from(error: MaterialError) -> Self {
        RendererError::MaterialError(error)
    }
}

impl From<String> for RendererError {
    fn from(msg: String) -> Self {
        RendererError::InvalidOperation(msg)
    }
}

impl From<&str> for RendererError {
    fn from(msg: &str) -> Self {
        RendererError::InvalidOperation(msg.to_string())
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
