//! Error types for render graph.

use std::fmt;

use super::resource::ResourceState;

/// Render graph errors.
#[derive(Debug)]
pub enum RenderGraphError {
    /// Resource not found.
    ResourceNotFound(String),
    /// Pass not found.
    PassNotFound(String),
    /// Cycle detected in dependency graph.
    DependencyCycle(String),
    /// Invalid resource state transition.
    InvalidStateTransition {
        from: ResourceState,
        to: ResourceState,
        resource: String,
    },
    /// Invalid configuration.
    InvalidConfiguration(String),
    /// Allocation failed.
    AllocationFailed(usize),
    /// Pipeline not set.
    PipelineNotSet(String),
    /// Vulkan error.
    VulkanError(String),
    /// Graph not compiled.
    NotCompiled,
    /// Invalid mesh handle.
    InvalidMeshHandle(crate::handle::MeshHandle),
    /// Invalid material handle.
    InvalidMaterialHandle(crate::handle::MaterialHandle),
    /// Invalid pipeline handle.
    InvalidPipelineHandle(crate::handle::PipelineHandle),
    /// Invalid skeleton handle.
    InvalidSkeletonHandle(crate::handle::SkeletonHandle),
}

impl fmt::Display for RenderGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceNotFound(name) => write!(f, "Resource '{}' not found", name),
            Self::PassNotFound(name) => write!(f, "Pass '{}' not found", name),
            Self::DependencyCycle(msg) => write!(f, "Cycle detected in dependency graph: {}", msg),
            Self::InvalidStateTransition { from, to, resource } => {
                write!(
                    f,
                    "Invalid state transition: {:?} -> {:?} for resource '{}'",
                    from, to, resource
                )
            }
            Self::InvalidConfiguration(msg) => {
                write!(f, "Invalid configuration: {}", msg)
            }
            Self::AllocationFailed(size) => {
                write!(
                    f,
                    "Failed to allocate {} bytes from transient allocator",
                    size
                )
            }
            Self::PipelineNotSet(name) => write!(f, "Pipeline not set for pass '{}'", name),
            Self::VulkanError(msg) => write!(f, "Vulkan error: {}", msg),
            Self::NotCompiled => write!(f, "Render graph has not been compiled"),
            Self::InvalidMeshHandle(handle) => write!(f, "Invalid mesh handle: {}", handle.index()),
            Self::InvalidMaterialHandle(handle) => {
                write!(f, "Invalid material handle: {}", handle.index())
            }
            Self::InvalidPipelineHandle(handle) => {
                write!(f, "Invalid pipeline handle: {}", handle.index())
            }
            Self::InvalidSkeletonHandle(handle) => {
                write!(f, "Invalid skeleton handle: {}", handle.index())
            }
        }
    }
}

impl std::error::Error for RenderGraphError {}

impl From<ash::vk::Result> for RenderGraphError {
    fn from(result: ash::vk::Result) -> Self {
        Self::VulkanError(format!("{:?}", result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_resource_not_found() {
        let err = RenderGraphError::ResourceNotFound("color_buffer".to_string());
        assert!(err.to_string().contains("color_buffer"));
    }

    #[test]
    fn test_error_display_pass_not_found() {
        let err = RenderGraphError::PassNotFound("geometry".to_string());
        assert!(err.to_string().contains("geometry"));
    }

    #[test]
    fn test_error_display_dependency_cycle() {
        let err = RenderGraphError::DependencyCycle("A -> B -> A".to_string());
        assert!(err.to_string().contains("Cycle detected"));
    }

    #[test]
    fn test_error_display_invalid_state_transition() {
        let err = RenderGraphError::InvalidStateTransition {
            from: ResourceState::Undefined,
            to: ResourceState::ColorAttachment,
            resource: "color".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Invalid state transition"));
        assert!(msg.contains("color"));
    }

    #[test]
    fn test_error_display_allocation_failed() {
        let err = RenderGraphError::AllocationFailed(1024);
        assert!(err.to_string().contains("1024"));
    }

    #[test]
    fn test_error_display_pipeline_not_set() {
        let err = RenderGraphError::PipelineNotSet("post_process".to_string());
        assert!(err.to_string().contains("Pipeline not set"));
    }

    #[test]
    fn test_error_display_vulkan_error() {
        let err = RenderGraphError::VulkanError("ERROR_DEVICE_LOST".to_string());
        assert!(err.to_string().contains("Vulkan error"));
    }

    #[test]
    fn test_from_vk_result() {
        let err = RenderGraphError::from(ash::vk::Result::ERROR_DEVICE_LOST);
        assert!(matches!(err, RenderGraphError::VulkanError(_)));
    }
}
