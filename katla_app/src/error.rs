//! Error types for katla_app.

use std::fmt;

/// Application-level error type.
#[derive(Debug)]
pub enum AppError {
    /// Failed to find resources directory
    ResourcesNotFound { path: String },

    /// Failed to load GLTF model
    ModelLoadFailed { path: String, reason: String },

    /// Failed to load material
    MaterialLoadFailed { name: String, reason: String },

    /// Failed to initialize renderer
    RendererInitFailed { reason: String },

    /// Graphics/rendering error
    Graphics { source: katla_gfx::RendererError },

    /// IO error
    Io { source: std::io::Error },

    /// Failed to compile shader for GLTF model
    ShaderCompileFailed { path: String, reason: String },

    /// Failed to create GPU skeleton for skinned model
    SkeletonCreateFailed { path: String, reason: String },

    /// Other error with message
    Other { message: String },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourcesNotFound { path } => {
                write!(f, "Resources directory not found (searched: {})", path)
            }
            Self::ModelLoadFailed { path, reason } => {
                write!(f, "Failed to load model '{}': {}", path, reason)
            }
            Self::MaterialLoadFailed { name, reason } => {
                write!(f, "Failed to load material '{}': {}", name, reason)
            }
            Self::RendererInitFailed { reason } => {
                write!(f, "Failed to initialize renderer: {}", reason)
            }
            Self::Graphics { source } => {
                write!(f, "Graphics error: {}", source)
            }
            Self::Io { source } => {
                write!(f, "IO error: {}", source)
            }
            Self::ShaderCompileFailed { path, reason } => {
                write!(f, "Failed to compile shader for '{}': {}", path, reason)
            }
            Self::SkeletonCreateFailed { path, reason } => {
                write!(f, "Failed to create skeleton for '{}': {}", path, reason)
            }
            Self::Other { message } => {
                write!(f, "{}", message)
            }
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source } => Some(source),
            Self::Graphics { source } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(source: std::io::Error) -> Self {
        Self::Io { source }
    }
}

/// Result type for app operations.
pub type AppResult<T> = Result<T, AppError>;
