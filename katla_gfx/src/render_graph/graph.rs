//! Frame graph execution types.
//!
//! This module provides the executable [`FrameGraph`] and [`Frame`]
//! types for render graph execution.

use std::collections::HashMap;

use super::builder::{InternalPassBuilder, PassBuilder};
use super::error::RenderGraphError;
use super::pass::PassDesc;
use super::resource::GraphResourceHandle;
use crate::renderer::types::DrawList;
use crate::renderer::VulkanRenderer;

/// Executable render graph.
///
/// Built once from a [`FrameGraphBuilder`], executed many times per frame.
///
/// # Example
///
/// ```ignore
/// // Build once at startup
/// let frame_graph = renderer.create_frame_graph()
///     .add_pass(GeometryPass::new("geometry")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .add_pass(FullscreenPass::new("tonemap")
///         .read("color")
///         .write_backbuffer())
///     .build()?;
///
/// // Execute every frame
/// renderer.render(&frame_graph, |frame| {
///     frame.submit("geometry", &draw_list);
/// });
/// ```
pub struct FrameGraph {
    /// Pass descriptors in execution order.
    passes: Vec<PassDesc>,

    /// String -> handle mapping for resources.
    resource_names: HashMap<String, GraphResourceHandle>,

    /// Pass name -> index mapping for execution context.
    pass_names: HashMap<String, usize>,

    /// Whether the graph has been compiled.
    compiled: bool,
}

impl FrameGraph {
    /// Create a new empty frame graph.
    pub(crate) fn new() -> Self {
        Self {
            passes: Vec::new(),
            resource_names: HashMap::new(),
            pass_names: HashMap::new(),
            compiled: false,
        }
    }

    /// Add a pass to the graph.
    pub(crate) fn add_pass(&mut self, pass: PassDesc) {
        let index = self.passes.len();
        self.pass_names.insert(pass.name.clone(), index);
        self.passes.push(pass);
        self.compiled = false;
    }

    /// Import a resource into the graph.
    pub(crate) fn import_resource(&mut self, name: impl Into<String>, handle: GraphResourceHandle) {
        self.resource_names.insert(name.into(), handle);
        self.compiled = false;
    }

    /// Compile the graph for execution.
    pub(crate) fn compile(&mut self) -> Result<(), RenderGraphError> {
        // TODO: Implement dependency analysis and topological sort
        // TODO: Compute resource barriers between passes
        self.compiled = true;
        Ok(())
    }

    /// Execute the graph with the given frame context.
    ///
    /// Called internally by `VulkanRenderer::render()`.
    pub(crate) fn execute(
        &mut self,
        renderer: &VulkanRenderer,
        f: impl FnOnce(&mut Frame),
    ) -> Result<(), RenderGraphError> {
        if !self.compiled {
            self.compile()?;
        }

        let mut frame = Frame::new(self, renderer);
        f(&mut frame);
        frame.execute_passes()?;

        Ok(())
    }

    /// Get a resource handle by name.
    pub(crate) fn resource_handle(&self, name: &str) -> Option<GraphResourceHandle> {
        self.resource_names.get(name).copied()
    }

    /// Get a pass index by name.
    pub(crate) fn pass_index(&self, name: &str) -> Option<usize> {
        self.pass_names.get(name).copied()
    }

    /// Get the number of passes in the graph.
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }
}

impl Default for FrameGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing a frame graph.
///
/// Created by [`VulkanRenderer::create_frame_graph()`].
/// Provides a fluent API for adding passes before building the executable [`FrameGraph`].
///
/// # Example
///
/// ```ignore
/// let frame_graph = renderer.create_frame_graph()
///     .add_pass(GeometryPass::new("geometry")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .add_pass(FullscreenPass::new("tonemap")
///         .read("color")
///         .write_backbuffer())
///     .build()?;
/// ```
pub struct FrameGraphBuilder {
    /// Internal pass builders from pass templates.
    pass_builders: Vec<InternalPassBuilder>,

    /// Resource declarations (name -> handle mapping).
    resources: HashMap<String, GraphResourceHandle>,

    /// Whether this builder writes to the backbuffer.
    writes_backbuffer: bool,
}

impl FrameGraphBuilder {
    /// Create a new frame graph builder.
    pub(crate) fn new() -> Self {
        Self {
            pass_builders: Vec::new(),
            resources: HashMap::new(),
            writes_backbuffer: false,
        }
    }

    /// Add a pass to the graph.
    ///
    /// Takes any type implementing the [`PassBuilder`] trait, such as
    /// [`GeometryPass`], [`FullscreenPass`], or [`ShadowPass`].
    pub fn add_pass(mut self, pass: impl PassBuilder + 'static) -> Self {
        self.pass_builders.push(pass.as_builder());
        self
    }

    /// Declare that this graph writes to the backbuffer (swapchain).
    ///
    /// This is typically called by pass templates that output to the screen,
    /// such as a final tonemap pass.
    pub(crate) fn writes_backbuffer(mut self) -> Self {
        self.writes_backbuffer = true;
        self
    }

    /// Import an external resource into the graph.
    ///
    /// Resources are referenced by name during graph construction and
    /// resolved to handles at build time.
    pub fn import_resource(mut self, name: impl Into<String>, handle: GraphResourceHandle) -> Self {
        self.resources.insert(name.into(), handle);
        self
    }

    /// Build the frame graph.
    ///
    /// Validates pass dependencies, allocates transient resources,
    /// and creates the executable [`FrameGraph`].
    pub fn build(self) -> Result<FrameGraph, RenderGraphError> {
        let mut graph = FrameGraph::new();

        // Import resources
        for (name, handle) in self.resources {
            graph.import_resource(name, handle);
        }

        // Build passes
        for pass_builder in self.pass_builders {
            // TODO: Resolve resource names to handles
            // TODO: Allocate transient resources
            // TODO: Validate dependencies

            let mut resource_map = HashMap::new();
            for read_name in &pass_builder.reads {
                if !resource_map.contains_key(read_name) {
                    resource_map.insert(
                        read_name.clone(),
                        GraphResourceHandle::new(resource_map.len() as u32),
                    );
                }
            }
            for write_name in &pass_builder.writes {
                if !resource_map.contains_key(write_name) {
                    resource_map.insert(
                        write_name.clone(),
                        GraphResourceHandle::new(resource_map.len() as u32),
                    );
                }
            }

            // Call the build function
            let _pass_data = (pass_builder.build_fn)(&resource_map)?;

            // Create PassDesc
            let reads: Vec<_> = pass_builder
                .reads
                .iter()
                .filter_map(|n| resource_map.get(n).copied())
                .collect();
            let writes: Vec<_> = pass_builder
                .writes
                .iter()
                .filter_map(|n| resource_map.get(n).copied())
                .collect();

            let pass = PassDesc::new(
                pass_builder.name,
                pass_builder.pass_type,
                reads,
                writes,
                Box::new(|_ctx| Ok(())),
            );

            graph.add_pass(pass);
        }

        graph.compile()?;
        Ok(graph)
    }
}

impl Default for FrameGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame context for submitting work to passes.
///
/// Passed to the closure in [`VulkanRenderer::render()`]. Provides a simple
/// API for submitting draw lists to named passes.
///
/// # Example
///
/// ```ignore
/// renderer.render(&frame_graph, |frame| {
///     frame.submit("geometry", &opaque_draw_list);
///     frame.submit("geometry", &transparent_draw_list);
///     frame.submit("shadows", &shadow_draw_list);
///     // Passes without draw lists (like tonemap) run automatically
/// });
/// ```
pub struct Frame<'a> {
    /// Reference to the frame graph.
    graph: &'a FrameGraph,

    /// Reference to the Vulkan renderer.
    renderer: &'a VulkanRenderer,

    /// Pending pass execution data.
    pending: HashMap<usize, PassExecutionData>,
}

/// Data for a single pass execution.
#[derive(Default, Clone)]
struct PassExecutionData {
    /// Draw lists to render in this pass.
    draw_lists: Vec<DrawList>,

    /// UI draw lists to render in this pass.
    ui_draw_lists: Vec<crate::renderer::types::UIDrawList>,

    /// Whether dispatch was requested.
    dispatch: Option<(u32, u32, u32)>,

    /// Custom uniform data.
    uniform_data: Vec<u8>,
}

impl<'a> Frame<'a> {
    /// Create a new frame context.
    pub(crate) fn new(graph: &'a FrameGraph, renderer: &'a VulkanRenderer) -> Self {
        Self {
            graph,
            renderer,
            pending: HashMap::new(),
        }
    }

    /// Submit a draw list to a pass.
    ///
    /// Can be called multiple times for the same pass to submit multiple draw lists.
    ///
    /// # Panics
    ///
    /// Panics if the pass name doesn't exist in the graph.
    pub fn submit(&mut self, pass: &str, draw_list: &DrawList) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending
            .entry(index)
            .or_insert_with(PassExecutionData::default)
            .draw_lists
            .push(draw_list.clone());
        self
    }

    /// Submit a UI draw list to a pass.
    ///
    /// Can be called multiple times for the same pass to submit multiple UI draw lists.
    pub fn submit_ui(
        &mut self,
        pass: &str,
        ui_draw_list: &crate::renderer::types::UIDrawList,
    ) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending
            .entry(index)
            .or_insert_with(PassExecutionData::default)
            .ui_draw_lists
            .push(ui_draw_list.clone());
        self
    }

    /// Dispatch compute workgroups for a pass.
    ///
    /// Only valid for compute passes.
    pub fn dispatch(&mut self, pass: &str, x: u32, y: u32, z: u32) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending
            .entry(index)
            .or_insert_with(PassExecutionData::default)
            .dispatch = Some((x, y, z));
        self
    }

    /// Push uniform data for a pass.
    ///
    /// The data is copied into the pass's uniform buffer.
    pub fn push_uniform(&mut self, pass: &str, data: &[u8]) -> &mut Self {
        let index = self
            .graph
            .pass_index(pass)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", pass));

        self.pending
            .entry(index)
            .or_insert_with(PassExecutionData::default)
            .uniform_data
            .extend_from_slice(data);
        self
    }

    /// Execute all passes in order.
    fn execute_passes(&mut self) -> Result<(), RenderGraphError> {
        for (index, _pass) in self.graph.passes.iter().enumerate() {
            if let Some(data) = self.pending.remove(&index) {
                self.execute_pass(index, data)?;
            }
        }

        Ok(())
    }

    /// Execute a single pass with the given data.
    fn execute_pass(
        &self,
        _index: usize,
        _data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        // TODO: Implement actual pass execution
        // This would:
        // 1. Get the command buffer from renderer
        // 2. Set up render pass / compute pass
        // 3. Execute draw calls or dispatch
        // 4. Insert barriers between passes
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_graph_new() {
        let graph = FrameGraph::new();
        assert_eq!(graph.pass_count(), 0);
        assert!(!graph.compiled);
    }

    #[test]
    fn test_frame_graph_default() {
        let graph = FrameGraph::default();
        assert_eq!(graph.pass_count(), 0);
    }

    #[test]
    fn test_frame_graph_builder_new() {
        let builder = FrameGraphBuilder::new();
        assert!(builder.pass_builders.is_empty());
        assert!(!builder.writes_backbuffer);
    }

    #[test]
    fn test_frame_graph_builder_default() {
        let builder = FrameGraphBuilder::default();
        assert!(builder.pass_builders.is_empty());
    }

    #[test]
    fn test_pass_execution_data_default() {
        let data = PassExecutionData::default();
        assert!(data.draw_lists.is_empty());
        assert!(data.dispatch.is_none());
        assert!(data.uniform_data.is_empty());
    }
}
