use ash::vk;

/// Errors that can occur during render graph construction, compilation, and execution.
#[derive(Debug)]
pub enum RenderGraphError {
    /// A resource with the given ID was not found
    ResourceNotFound(u32),

    /// Invalid resource usage was specified
    InvalidResourceUsage(String),

    /// A Vulkan operation failed
    VulkanError(vk::Result),

    /// An error occurred during graph compilation
    CompilationError(String),

    /// No frame data is available for rendering
    NoFrameData,

    /// No render graph has been set on the renderer
    NoRenderGraph,

    /// The swapchain is out of date and needs to be recreated
    SwapchainOutOfDate,
}

impl std::fmt::Display for RenderGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderGraphError::ResourceNotFound(id) => {
                write!(f, "Resource not found: {}", id)
            }
            RenderGraphError::InvalidResourceUsage(msg) => {
                write!(f, "Invalid resource usage: {}", msg)
            }
            RenderGraphError::VulkanError(err) => {
                write!(f, "Vulkan error: {:?}", err)
            }
            RenderGraphError::CompilationError(msg) => {
                write!(f, "Compilation error: {}", msg)
            }
            RenderGraphError::NoFrameData => {
                write!(f, "No frame data available")
            }
            RenderGraphError::NoRenderGraph => {
                write!(f, "No render graph set")
            }
            RenderGraphError::SwapchainOutOfDate => {
                write!(f, "Swapchain is out of date")
            }
        }
    }
}

impl std::error::Error for RenderGraphError {}

impl From<vk::Result> for RenderGraphError {
    fn from(err: vk::Result) -> Self {
        RenderGraphError::VulkanError(err)
    }
}
