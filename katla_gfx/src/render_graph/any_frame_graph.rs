//! Enum-based frame graph dispatch for dynamic backend selection.

use super::backend::RenderGraphBackend;
use super::diagnostics::RenderGraphDiagnostics;
use super::error::RenderGraphError;
use super::frame_graph::FrameGraph;
use super::handles::{PassId, ResourceId};
use super::pass::PassDesc;
use super::trace::{ResourceExecutionTrace as RenderExecutionTrace, TraceDivergence};

#[cfg(target_os = "macos")]
use crate::metal::metal_renderer::MetalRenderer;
use crate::renderer::VulkanRenderer;

/// Frame graph that wraps both Vulkan and Metal backends behind a single type.
pub enum AnyFrameGraph {
    Vulkan(FrameGraph<VulkanRenderer>),
    #[cfg(target_os = "macos")]
    Metal(FrameGraph<MetalRenderer>),
}

impl AnyFrameGraph {
    pub fn from_vulkan(fg: FrameGraph<VulkanRenderer>) -> Self {
        AnyFrameGraph::Vulkan(fg)
    }

    #[cfg(target_os = "macos")]
    pub fn from_metal(fg: FrameGraph<MetalRenderer>) -> Self {
        AnyFrameGraph::Metal(fg)
    }

    pub fn new() -> Self {
        AnyFrameGraph::Vulkan(FrameGraph::new())
    }

    /// Capture compiled logical resources and observed native execution.
    pub fn capture(&self) -> Result<super::capture::RenderGraphCapture, RenderGraphError> {
        match self {
            Self::Vulkan(graph) => graph.capture(),
            #[cfg(target_os = "macos")]
            Self::Metal(graph) => graph.capture(),
        }
    }

    pub fn add_pass(&mut self, pass: PassDesc) -> Result<PassId, RenderGraphError> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.add_pass(pass),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.add_pass(pass),
        }
    }

    pub fn insert_pass(
        &mut self,
        index: usize,
        pass: PassDesc,
    ) -> Result<PassId, RenderGraphError> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.insert_pass(index, pass),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.insert_pass(index, pass),
        }
    }

    /// Import an ordinary application-owned buffer.
    pub fn import_buffer(
        &mut self,
        name: impl Into<String>,
        handle: crate::handle::BufferHandle,
        desc: super::BufferDesc,
    ) -> Result<ResourceId, RenderGraphError> {
        match self {
            Self::Vulkan(graph) => graph.import_buffer(name, handle, desc),
            #[cfg(target_os = "macos")]
            Self::Metal(graph) => graph.import_buffer(name, handle, desc),
        }
    }

    /// Select the acquired slot's buffer without changing its graph contract.
    pub fn rebind_imported_buffer(
        &mut self,
        resource: ResourceId,
        handle: crate::handle::BufferHandle,
    ) -> Result<(), RenderGraphError> {
        match self {
            Self::Vulkan(graph) => graph.rebind_imported_buffer(resource, handle),
            #[cfg(target_os = "macos")]
            Self::Metal(graph) => graph.rebind_imported_buffer(resource, handle),
        }
    }

    /// Revalidate a resource contract after an imported buffer changes capacity.
    pub fn redefine_imported_buffer(
        &mut self,
        resource: ResourceId,
        handle: crate::handle::BufferHandle,
        desc: super::BufferDesc,
    ) -> Result<(), RenderGraphError> {
        match self {
            Self::Vulkan(graph) => graph.redefine_imported_buffer(resource, handle, desc),
            #[cfg(target_os = "macos")]
            Self::Metal(graph) => graph.redefine_imported_buffer(resource, handle, desc),
        }
    }

    /// Retire an imported allocation after all pass references have been removed.
    pub fn remove_imported_buffer(&mut self, resource: ResourceId) -> Result<(), RenderGraphError> {
        match self {
            Self::Vulkan(graph) => graph.remove_imported_buffer(resource),
            #[cfg(target_os = "macos")]
            Self::Metal(graph) => graph.remove_imported_buffer(resource),
        }
    }

    /// Replace a pass's explicit compute/transfer workload and accesses.
    pub fn set_pass_commands(
        &mut self,
        pass: PassId,
        commands: Vec<super::ComputeCommand>,
        accesses: Vec<super::BufferAccess>,
    ) -> Result<(), RenderGraphError> {
        match self {
            Self::Vulkan(graph) => graph.set_pass_commands(pass, commands, accesses),
            #[cfg(target_os = "macos")]
            Self::Metal(graph) => graph.set_pass_commands(pass, commands, accesses),
        }
    }

    /// Bind application-owned graphics resources and pipelines to a pass.
    pub fn set_pass_bindings(
        &mut self,
        pass: PassId,
        bindings: crate::renderer::frame_bindings::PassBindings,
    ) -> Result<(), RenderGraphError> {
        match self {
            Self::Vulkan(graph) => graph.set_pass_bindings(pass, bindings),
            #[cfg(target_os = "macos")]
            Self::Metal(graph) => graph.set_pass_bindings(pass, bindings),
        }
    }

    /// Declare real buffer dependencies on an existing graphics pass.
    pub fn extend_pass_buffer_accesses(
        &mut self,
        name: &str,
        accesses: Vec<super::access::BufferAccess>,
    ) -> Result<(), RenderGraphError> {
        match self {
            Self::Vulkan(graph) => graph.extend_pass_buffer_accesses(name, accesses),
            #[cfg(target_os = "macos")]
            Self::Metal(graph) => graph.extend_pass_buffer_accesses(name, accesses),
        }
    }

    pub fn pass_id(&self, name: &str) -> Option<PassId> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.pass_id(name),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.pass_id(name),
        }
    }

    /// Resolve a named graph resource without exposing backend-specific graph types.
    pub fn resource_id(&self, name: &str) -> Option<ResourceId> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.resource_id(name),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.resource_id(name),
        }
    }

    /// Prepare one explicit compute descriptor without compiling graph topology.
    pub fn prepare_compute_pipeline(
        &self,
        renderer: &mut crate::AnyRenderer,
        descriptor: &super::ComputePipelineDesc,
    ) -> Result<(), RenderGraphError> {
        match (self, renderer) {
            (Self::Vulkan(_), crate::AnyRenderer::Vulkan(renderer)) => {
                RenderGraphBackend::prepare_compute_pipeline(renderer, descriptor)
            }
            #[cfg(target_os = "macos")]
            (Self::Metal(_), crate::AnyRenderer::Metal(renderer)) => {
                RenderGraphBackend::prepare_compute_pipeline(renderer, descriptor)
            }
            #[cfg(target_os = "macos")]
            _ => Err(RenderGraphError::InvalidConfiguration(
                "Graph backend differs from renderer".into(),
            )),
        }
    }

    /// Warm reflected compute pipelines before the first frame is acquired.
    pub fn initialize_compute_pipelines(
        &mut self,
        renderer: &mut crate::AnyRenderer,
    ) -> Result<(), RenderGraphError> {
        match (self, renderer) {
            (Self::Vulkan(graph), crate::AnyRenderer::Vulkan(renderer)) => {
                graph.initialize_compute_pipelines(renderer)
            }
            #[cfg(target_os = "macos")]
            (Self::Metal(graph), crate::AnyRenderer::Metal(renderer)) => {
                graph.initialize_compute_pipelines(renderer)
            }
            #[cfg(target_os = "macos")]
            _ => Err(RenderGraphError::InvalidConfiguration(
                "Graph backend differs from renderer".into(),
            )),
        }
    }

    pub fn cleanup(&mut self) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.cleanup(),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.cleanup(),
        }
    }

    // --- Backend-specific accessors ---

    /// Access the Vulkan frame graph mutably.
    pub fn as_vulkan_mut(&mut self) -> &mut FrameGraph<VulkanRenderer> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg,
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(_) => panic!("Expected Vulkan frame graph"),
        }
    }

    /// Access the Vulkan frame graph (const).
    pub fn as_vulkan(&self) -> &FrameGraph<VulkanRenderer> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg,
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(_) => panic!("Expected Vulkan frame graph"),
        }
    }

    /// Access the Metal frame graph mutably.
    #[cfg(target_os = "macos")]
    pub fn as_metal_mut(&mut self) -> &mut FrameGraph<MetalRenderer> {
        match self {
            AnyFrameGraph::Vulkan(_) => panic!("Expected Metal frame graph"),
            AnyFrameGraph::Metal(fg) => fg,
        }
    }

    /// Access the Metal frame graph (const).
    #[cfg(target_os = "macos")]
    pub fn as_metal(&self) -> &FrameGraph<MetalRenderer> {
        match self {
            AnyFrameGraph::Vulkan(_) => panic!("Expected Metal frame graph"),
            AnyFrameGraph::Metal(fg) => fg,
        }
    }

    /// Enable or disable recording of the emitted encoder trace.
    pub fn set_execution_trace(&mut self, enabled: bool) {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.set_execution_trace(enabled),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.set_execution_trace(enabled),
        }
    }

    /// Whether execution currently records an emitted encoder trace.
    pub fn execution_trace_enabled(&self) -> bool {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.execution_trace_enabled(),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.execution_trace_enabled(),
        }
    }

    /// Encoders emitted by the most recent execution.
    pub fn last_execution_trace(&self) -> &RenderExecutionTrace {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.last_execution_trace(),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.last_execution_trace(),
        }
    }

    /// Compare the most recent execution's emitted trace against the compiled
    /// plan, returning every divergence found.
    pub fn compare_execution_trace(&self) -> Vec<TraceDivergence> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.compare_execution_trace(),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.compare_execution_trace(),
        }
    }

    /// Build a deterministic diagnostics snapshot of the declared graph.
    ///
    /// The compiler is pure, so this works before any GPU resource exists and
    /// is independent of the selected backend.
    pub fn diagnostics(&self) -> Result<RenderGraphDiagnostics, RenderGraphError> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.diagnostics(),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.diagnostics(),
        }
    }

    /// Get the transient texture bindless slot for a named texture.
    /// Returns None if the texture doesn't exist or has no bindless slot.
    pub fn transient_texture_bindless_slot(&self, name: &str, frame_idx: usize) -> Option<u32> {
        match self {
            AnyFrameGraph::Vulkan(fg) => fg.transient_texture(name, frame_idx).and_then(|t| {
                <VulkanRenderer as RenderGraphBackend>::transient_texture_bindless_slot(t)
            }),
            #[cfg(target_os = "macos")]
            AnyFrameGraph::Metal(fg) => fg.transient_texture(name, frame_idx).and_then(|t| {
                <MetalRenderer as RenderGraphBackend>::transient_texture_bindless_slot(t)
            }),
        }
    }

    /// Recreate transient textures with new dimensions.
    /// Returns (texture_name, bindless_slot) pairs for all recreated textures.
    pub fn recreate_transient_textures(
        &mut self,
        renderer: &mut crate::renderer::any_renderer::AnyRenderer,
        width: u32,
        height: u32,
    ) -> Result<Vec<(String, u32)>, RenderGraphError> {
        match (self, renderer) {
            (AnyFrameGraph::Vulkan(fg), crate::renderer::any_renderer::AnyRenderer::Vulkan(r)) => {
                fg.recreate_transient_textures(r, width, height)
            }
            #[cfg(target_os = "macos")]
            (AnyFrameGraph::Metal(fg), crate::renderer::any_renderer::AnyRenderer::Metal(r)) => {
                fg.recreate_transient_textures(r, width, height)
            }
            #[cfg(target_os = "macos")]
            _ => Err(RenderGraphError::BackendError(
                "Backend mismatch between frame graph and renderer".into(),
            )),
        }
    }

    /// Initialize transient textures using the renderer.
    pub fn initialize_transient_textures(
        &mut self,
        renderer: &mut crate::renderer::any_renderer::AnyRenderer,
    ) -> Result<(), RenderGraphError> {
        match (self, renderer) {
            (AnyFrameGraph::Vulkan(fg), crate::renderer::any_renderer::AnyRenderer::Vulkan(r)) => {
                fg.initialize_transient_textures(r)
            }
            #[cfg(target_os = "macos")]
            (AnyFrameGraph::Metal(fg), crate::renderer::any_renderer::AnyRenderer::Metal(r)) => {
                fg.initialize_transient_textures(r)
            }
            #[cfg(target_os = "macos")]
            _ => Err(RenderGraphError::BackendError(
                "Backend mismatch between frame graph and renderer".into(),
            )),
        }
    }

    /// Initialize graph-owned buffers for every frame slot.
    pub fn initialize_transient_buffers(
        &mut self,
        renderer: &mut crate::renderer::any_renderer::AnyRenderer,
    ) -> Result<(), RenderGraphError> {
        match (self, renderer) {
            (AnyFrameGraph::Vulkan(fg), crate::renderer::any_renderer::AnyRenderer::Vulkan(r)) => {
                fg.initialize_transient_buffers(r)
            }
            #[cfg(target_os = "macos")]
            (AnyFrameGraph::Metal(fg), crate::renderer::any_renderer::AnyRenderer::Metal(r)) => {
                fg.initialize_transient_buffers(r)
            }
            #[cfg(target_os = "macos")]
            _ => Err(RenderGraphError::BackendError(
                "Backend mismatch between frame graph and renderer".into(),
            )),
        }
    }

    /// Register a transient texture with the bindless system.
    pub fn register_transient_texture_bindless(
        &mut self,
        renderer: &mut crate::renderer::any_renderer::AnyRenderer,
        name: &str,
    ) -> Result<u32, RenderGraphError> {
        match (self, renderer) {
            (AnyFrameGraph::Vulkan(fg), crate::renderer::any_renderer::AnyRenderer::Vulkan(r)) => {
                fg.register_transient_texture_bindless(r, name)
            }
            #[cfg(target_os = "macos")]
            (AnyFrameGraph::Metal(fg), crate::renderer::any_renderer::AnyRenderer::Metal(r)) => {
                fg.register_transient_texture_bindless(r, name)
            }
            #[cfg(target_os = "macos")]
            _ => Err(RenderGraphError::BackendError(
                "Backend mismatch between frame graph and renderer".into(),
            )),
        }
    }

    // --- Metal-specific methods ---

    #[cfg(target_os = "macos")]
    pub fn transient_image_view_metal(
        &self,
        name: &str,
        frame_idx: usize,
    ) -> Option<crate::metal::texture::MetalTextureView> {
        match self {
            AnyFrameGraph::Vulkan(_) => None,
            AnyFrameGraph::Metal(fg) => fg.transient_image_view(name, frame_idx),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn transient_texture_metal(
        &self,
        name: &str,
        frame_idx: usize,
    ) -> Option<&<MetalRenderer as RenderGraphBackend>::TransientTexture> {
        match self {
            AnyFrameGraph::Vulkan(_) => None,
            AnyFrameGraph::Metal(fg) => fg.transient_texture(name, frame_idx),
        }
    }
}

impl Default for AnyFrameGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{
        FrameGraphBuilder, GraphResourceDesc, GraphResourceType, PassType, SimplePass,
    };
    use crate::texture::ImageFormat;

    #[test]
    fn test_diagnostics_export_through_any_frame_graph() {
        let graph = FrameGraphBuilder::new()
            .create_resource(GraphResourceDesc {
                name: "color".to_string(),
                resource_type: GraphResourceType::ColorAttachment { clear_value: None },
                format: ImageFormat::R8G8B8A8Unorm,
                width: 64,
                height: 64,
                tracks_swapchain_size: false,
            })
            .add_pass(SimplePass::new("geometry", PassType::Graphics).write("color"))
            .build::<crate::renderer::VulkanRenderer>()
            .expect("build graph");
        let graph = AnyFrameGraph::from_vulkan(graph);

        let diagnostics = graph.diagnostics().expect("diagnostics");
        let json = diagnostics.to_json_pretty().expect("json export");
        assert_eq!(diagnostics.summary.declared_passes, 1);
        assert!(json.contains("\"transient_slots\""));
    }
}
