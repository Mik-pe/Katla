//! Frame graph execution types.
//!
//! This module provides the executable [`FrameGraph`] and [`ExecutionContext`]
//! types for render graph execution.

use std::collections::HashMap;

use super::builder::PassBuilder;
use super::error::RenderGraphError;
use super::pass::PassDesc;
use super::resource::GraphResourceHandle;
use crate::renderer::VulkanRenderer;
use crate::renderer::types::DrawList;

/// Executable render graph.
///
/// Built once from a [`FrameGraphBuilder`], executed many times per frame.
///
/// # Example
///
/// ```ignore
/// // Build once
/// let graph = FrameGraph::builder()
///     .add_pass(GeometryPass::new("geometry")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .build(&renderer)?;
///
/// // Execute every frame
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("geometry").draw_list(&draw_list);
/// })?;
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
    /// Create a builder for constructing a frame graph.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let graph = FrameGraph::builder()
    ///     .add_pass(GeometryPass::new("geometry")
    ///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
    ///         .write_depth("depth", ImageFormat::D32Sfloat))
    ///     .build()?;
    /// ```
    pub fn builder() -> FrameGraphBuilder {
        FrameGraphBuilder::new()
    }

    /// Create a new empty frame graph.
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            resource_names: HashMap::new(),
            pass_names: HashMap::new(),
            compiled: false,
        }
    }

    /// Add a pass to the graph.
    ///
    /// Passes are executed in the order they are added.
    pub fn add_pass(&mut self, pass: PassDesc) {
        let index = self.passes.len();
        self.pass_names.insert(pass.name.clone(), index);
        self.passes.push(pass);
        self.compiled = false;
    }

    /// Import a resource into the graph.
    ///
    /// Resources are referenced by name during execution.
    pub fn import_resource(&mut self, name: impl Into<String>, handle: GraphResourceHandle) {
        self.resource_names.insert(name.into(), handle);
        self.compiled = false;
    }

    /// Compile the graph for execution.
    ///
    /// This analyzes dependencies and computes the execution order.
    /// Currently a no-op placeholder - will add topological sort and
    /// barrier computation in the future.
    pub fn compile(&mut self) -> Result<(), RenderGraphError> {
        // TODO: Implement dependency analysis and topological sort
        // TODO: Compute resource barriers between passes
        self.compiled = true;
        Ok(())
    }

    /// Execute the graph with the given closure.
    ///
    /// The closure receives an [`ExecutionContext`] for configuring passes.
    ///
    /// # Arguments
    ///
    /// * `renderer` - VulkanRenderer for GPU access
    /// * `f` - Execution callback with ExecutionContext
    ///
    /// # Example
    ///
    /// ```ignore
    /// graph.execute(&renderer, |ctx| {
    ///     ctx.pass("geometry")
    ///         .draw_list(&opaque);
    ///     ctx.pass("transparent")
    ///         .draw_list(&transparent);
    /// })?;
    /// ```
    pub fn execute<F>(&mut self, renderer: &VulkanRenderer, f: F) -> Result<(), RenderGraphError>
    where
        F: FnOnce(&mut ExecutionContext),
    {
        if !self.compiled {
            self.compile()?;
        }

        // Create execution context
        let mut ctx = ExecutionContext::new(self, renderer);

        // User callback to configure passes
        f(&mut ctx);

        // Execute all passes in order
        ctx.execute_passes()?;

        Ok(())
    }

    /// Get a resource handle by name.
    pub fn resource_handle(&self, name: &str) -> Option<GraphResourceHandle> {
        self.resource_names.get(name).copied()
    }

    /// Get a pass index by name.
    pub fn pass_index(&self, name: &str) -> Option<usize> {
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
/// Provides a fluent API for adding passes and resources before building
/// the executable [`FrameGraph`].
///
/// # Example
///
/// ```ignore
/// let graph = FrameGraph::builder()
///     .add_pass(GeometryPass::new("geometry")
///         .write_color("color", ImageFormat::R16G16B16A16Sfloat)
///         .write_depth("depth", ImageFormat::D32Sfloat))
///     .add_pass(FullscreenPass::new("tonemap")
///         .read("color")
///         .write("output", ImageFormat::R8G8B8A8Srgb))
///     .build()?;
/// ```
pub struct FrameGraphBuilder {
    /// Internal pass builders from pass templates.
    pass_builders: Vec<super::builder::InternalPassBuilder>,
    /// Resource declarations (name -> handle mapping).
    resources: HashMap<String, GraphResourceHandle>,
}

impl FrameGraphBuilder {
    /// Create a new frame graph builder.
    pub fn new() -> Self {
        Self {
            pass_builders: Vec::new(),
            resources: HashMap::new(),
        }
    }

    /// Add a pass to the graph.
    ///
    /// Takes any type implementing the [`PassBuilder`] trait, such as
    /// [`GeometryPass`], [`FullscreenPass`], or [`ShadowPass`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// let builder = FrameGraph::builder()
    ///     .add_pass(GeometryPass::new("geometry")
    ///         .write_color("color", ImageFormat::R16G16B16A16Sfloat));
    /// ```
    pub fn add_pass(mut self, pass: impl PassBuilder + 'static) -> Self {
        self.pass_builders.push(pass.as_builder());
        self
    }

    /// Import an external resource into the graph.
    ///
    /// Resources are referenced by name during graph construction and
    /// resolved to handles at build time.
    ///
    /// # Arguments
    ///
    /// * `name` - Resource name for graph reference
    /// * `handle` - External resource handle
    pub fn import_resource(mut self, name: impl Into<String>, handle: GraphResourceHandle) -> Self {
        self.resources.insert(name.into(), handle);
        self
    }

    /// Build the frame graph.
    ///
    /// Resolves string resource names to handles and creates the
    /// executable [`FrameGraph`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// let graph = FrameGraph::builder()
    ///     .add_pass(GeometryPass::new("geometry")
    ///         .write_color("color", ImageFormat::R16G16B16A16Sfloat))
    ///     .build()?;
    /// ```
    pub fn build(self) -> Result<FrameGraph, RenderGraphError> {
        let mut graph = FrameGraph::new();

        // Import resources
        for (name, handle) in self.resources {
            graph.import_resource(name, handle);
        }

        // Build passes
        for pass_builder in self.pass_builders {
            // Resolve resource names to handles
            let mut resource_map = HashMap::new();

            // For now, use a dummy handle for each resource name
            // In a full implementation, this would allocate actual resources
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

            // Create PassDesc (simplified - in full implementation would use pass_data)
            let pass = PassDesc::new(pass_builder.name);

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

/// Execution context for graph passes.
///
/// Provides autocomplete-friendly access to passes by name during execution.
///
/// # Example
///
/// ```ignore
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("geometry")
///         .set_frame_uniforms(&uniforms)
///         .draw_list(&opaque_draw_list);
///
///     ctx.pass("lighting")
///         .push_uniform(&light_data)
///         .dispatch();
/// })?;
/// ```
pub struct ExecutionContext<'a> {
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

impl<'a> ExecutionContext<'a> {
    /// Create a new execution context.
    pub(crate) fn new(graph: &'a FrameGraph, renderer: &'a VulkanRenderer) -> Self {
        Self {
            graph,
            renderer,
            pending: HashMap::new(),
        }
    }

    /// Access a pass by name.
    ///
    /// Returns a [`PassHandle`] for configuring pass execution.
    ///
    /// # Panics
    ///
    /// Panics if the pass name doesn't exist in the graph.
    pub fn pass(&mut self, name: &str) -> PassHandle<'_> {
        let index = self
            .graph
            .pass_index(name)
            .unwrap_or_else(|| panic!("Pass '{}' not found in graph", name));

        PassHandle {
            index,
            pending: &mut self.pending,
        }
    }

    /// Try to access a pass by name (non-panicking).
    ///
    /// Returns `None` if the pass doesn't exist.
    pub fn try_pass(&mut self, name: &str) -> Option<PassHandle<'_>> {
        self.graph.pass_index(name).map(|index| PassHandle {
            index,
            pending: &mut self.pending,
        })
    }

    /// Execute all passes in order.
    ///
    /// This is called internally after the user callback completes.
    fn execute_passes(&mut self) -> Result<(), RenderGraphError> {
        // Execute passes in order
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

        // For now, this is a placeholder
        Ok(())
    }
}

/// Handle for configuring pass execution.
///
/// Returned by [`ExecutionContext::pass()`].
pub struct PassHandle<'a> {
    /// Pass index in the graph.
    index: usize,

    /// Reference to pending execution data.
    pending: &'a mut HashMap<usize, PassExecutionData>,
}

impl<'a> PassHandle<'a> {
    /// Submit a draw list for rendering in this pass.
    ///
    /// Can be called multiple times to submit multiple draw lists.
    pub fn draw_list(&mut self, draw_list: &DrawList) -> &mut Self {
        self.pending
            .entry(self.index)
            .or_insert_with(PassExecutionData::default)
            .draw_lists
            .push(draw_list.clone());
        self
    }

    /// Submit a UI draw list for rendering in this pass.
    ///
    /// Can be called multiple times to submit multiple UI draw lists.
    pub fn draw_ui(&mut self, ui_draw_list: &crate::renderer::types::UIDrawList) -> &mut Self {
        self.pending
            .entry(self.index)
            .or_insert_with(PassExecutionData::default)
            .ui_draw_lists
            .push(ui_draw_list.clone());
        self
    }

    /// Dispatch compute workgroups.
    ///
    /// Only valid for compute passes.
    pub fn dispatch(&mut self, x: u32, y: u32, z: u32) -> &mut Self {
        self.pending
            .entry(self.index)
            .or_insert_with(PassExecutionData::default)
            .dispatch = Some((x, y, z));
        self
    }

    /// Push uniform data for this pass.
    ///
    /// The data is copied into the pass's uniform buffer.
    pub fn push_uniform(&mut self, data: &[u8]) -> &mut Self {
        self.pending
            .entry(self.index)
            .or_insert_with(PassExecutionData::default)
            .uniform_data
            .extend_from_slice(data);
        self
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
    fn test_frame_graph_import_resource() {
        let mut graph = FrameGraph::new();
        let handle = GraphResourceHandle::new(0);
        graph.import_resource("color", handle);

        assert_eq!(graph.resource_handle("color"), Some(handle));
        assert_eq!(graph.resource_handle("depth"), None);
    }

    #[test]
    fn test_frame_graph_add_pass() {
        let mut graph = FrameGraph::new();
        let pass = PassDesc::new("geometry");
        graph.add_pass(pass);

        assert_eq!(graph.pass_count(), 1);
        assert_eq!(graph.pass_index("geometry"), Some(0));
    }

    #[test]
    fn test_frame_graph_compile() {
        let mut graph = FrameGraph::new();
        let result = graph.compile();
        assert!(result.is_ok());
        assert!(graph.compiled);
    }

    #[test]
    fn test_pass_execution_data_default() {
        let data = PassExecutionData::default();
        assert!(data.draw_lists.is_empty());
        assert!(data.dispatch.is_none());
        assert!(data.uniform_data.is_empty());
    }
}
