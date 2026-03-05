//! Frame graph execution types.
//!
//! This module provides the executable [`FrameGraph`] and [`Frame`]
//! types for render graph execution.

use std::collections::HashMap;

use super::builder::{InternalPassBuilder, PassBuilder};
use super::compiler::{ExecutionPlan, GraphCompiler};
use super::error::RenderGraphError;
use super::pass::PassDesc;
use super::resource::{GraphResourceHandle, ResourceState};
use crate::renderer::VulkanRenderer;
use crate::renderer::types::DrawList;
use ash::vk;
use bytemuck::{Pod, Zeroable};

/// Executable render graph.
///
/// Built once from a [`FrameGraphBuilder`], executed many times per frame.
pub struct FrameGraph {
    /// Pass descriptors in execution order.
    passes: Vec<PassDesc>,

    /// String -> handle mapping for resources.
    resource_names: HashMap<String, GraphResourceHandle>,

    /// Pass name -> index mapping for execution context.
    pass_names: HashMap<String, usize>,

    /// Compiled execution plan (sorted passes, barriers).
    execution_plan: Option<ExecutionPlan>,

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
            execution_plan: None,
            compiled: false,
        }
    }

    /// Add a pass to the graph.
    pub(crate) fn add_pass(&mut self, pass: PassDesc) {
        let index = self.passes.len();
        self.pass_names.insert(pass.name.clone(), index);
        self.passes.push(pass);
        self.compiled = false;
        self.execution_plan = None;
    }

    /// Import a resource into the graph.
    pub(crate) fn import_resource(&mut self, name: impl Into<String>, handle: GraphResourceHandle) {
        self.resource_names.insert(name.into(), handle);
        self.compiled = false;
        self.execution_plan = None;
    }

    /// Compile the graph for execution.
    pub(crate) fn compile(&mut self) -> Result<(), RenderGraphError> {
        if self.compiled {
            return Ok(());
        }

        // Build initial resource states (all undefined initially)
        let mut initial_states = HashMap::new();
        for handle in self.resource_names.values() {
            initial_states.insert(*handle, ResourceState::Undefined);
        }

        // Use the graph compiler to analyze dependencies and compute barriers
        let compiler = GraphCompiler::from_pass_descs(&self.passes);
        let execution_plan = compiler.compile(&initial_states)?;

        self.execution_plan = Some(execution_plan);
        self.compiled = true;
        Ok(())
    }

    /// Execute the graph with the given frame context.
    ///
    /// Called internally by `VulkanRenderer::render()`.
    pub(crate) fn execute(
        &mut self,
        renderer: &VulkanRenderer,
        image_index: u32,
        f: impl FnOnce(&mut Frame),
    ) -> Result<(), RenderGraphError> {
        if !self.compiled {
            self.compile()?;
        }

        let mut frame = Frame::new(self, renderer, image_index);
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

    /// Get the execution plan (after compilation).
    pub(crate) fn execution_plan(&self) -> Option<&ExecutionPlan> {
        self.execution_plan.as_ref()
    }

    /// Get a pass by index.
    pub(crate) fn pass(&self, index: usize) -> Option<&PassDesc> {
        self.passes.get(index)
    }

    /// Get all passes.
    pub(crate) fn passes(&self) -> &[PassDesc] {
        &self.passes
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
    /// Takes any type implementing the [`PassBuilder`] trait.
    pub fn add_pass(mut self, pass: impl PassBuilder + 'static) -> Self {
        self.pass_builders.push(pass.as_builder());
        self
    }

    /// Declare that this graph writes to the backbuffer (swapchain).
    pub(crate) fn writes_backbuffer(mut self) -> Self {
        self.writes_backbuffer = true;
        self
    }

    /// Import an external resource into the graph.
    pub fn import_resource(mut self, name: impl Into<String>, handle: GraphResourceHandle) -> Self {
        self.resources.insert(name.into(), handle);
        self
    }

    /// Build the frame graph.
    pub fn build(self) -> Result<FrameGraph, RenderGraphError> {
        let mut graph = FrameGraph::new();

        // Import resources
        for (name, handle) in self.resources {
            graph.import_resource(name, handle);
        }

        // Build passes
        for pass_builder in self.pass_builders {
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
pub struct Frame<'a> {
    /// Reference to the frame graph.
    graph: &'a FrameGraph,

    /// Reference to the Vulkan renderer.
    renderer: &'a VulkanRenderer,

    /// Current swapchain image index being rendered to.
    image_index: u32,

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
    pub(crate) fn new(graph: &'a FrameGraph, renderer: &'a VulkanRenderer, image_index: u32) -> Self {
        Self {
            graph,
            renderer,
            image_index,
            pending: HashMap::new(),
        }
    }

    /// Submit a draw list to a pass.
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
        let frame_idx = self.renderer.swap_data.current_frame();
        let cmd = &self.renderer.frame_context.command_buffers[frame_idx];

        for (index, pass) in self.graph.passes.iter().enumerate() {
            let data = self.pending.remove(&index).unwrap_or_default();

            // Insert pre-pass barriers
            self.insert_barriers(cmd, index)?;

            // Execute pass based on type
            match pass.pass_type {
                super::pass::PassType::Graphics => {
                    self.execute_graphics_pass(cmd, pass, data)?;
                }
                super::pass::PassType::Compute => {
                    self.execute_compute_pass(cmd, pass, data)?;
                }
                super::pass::PassType::Transfer => {}
            }
        }

        Ok(())
    }

    /// Insert barriers for a pass.
    fn insert_barriers(
        &mut self,
        _cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        let execution_plan = self.graph.execution_plan()
            .ok_or(RenderGraphError::NotCompiled)?;

        let barriers = execution_plan.barriers_for_pass(pass_index);
        if barriers.is_none() || barriers.unwrap().is_empty() {
            return Ok(());
        }

        // TODO: Convert ResourceState to vk types and insert barriers
        // For now, we'll skip barrier insertion since we don't have transient resources yet

        Ok(())
    }

    /// Execute a graphics pass with dynamic rendering.
    fn execute_graphics_pass(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pass: &PassDesc,
        data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        // For MVP: render directly to swapchain
        let swapchain_view = self.renderer.frame_context.swapchain_image_views[self.image_index as usize].vk();

        let extent = self.renderer.frame_context.swapchain.get_extent();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        // Set up color attachment
        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(swapchain_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.1, 0.1, 0.1, 1.0] },
            });

        // Begin dynamic rendering
        cmd.begin_rendering(
            &[color_attachment],
            None, // depth for later
            None,
            render_area,
            1,
        );

        // Set viewport and scissor
        cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
            0.0,
            0.0,
            extent.width as f32,
            extent.height as f32,
        )]);
        cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
            extent.width,
            extent.height,
        )]);

        // Execute draw lists
        for draw_list in &data.draw_lists {
            self.execute_draw_list(cmd, draw_list)?;
        }

        // End rendering
        cmd.end_rendering();

        Ok(())
    }

    /// Execute a compute pass.
    fn execute_compute_pass(
        &self,
        _cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        _pass: &PassDesc,
        _data: PassExecutionData,
    ) -> Result<(), RenderGraphError> {
        // TODO: Implement compute pass execution
        Ok(())
    }

    /// Execute a draw list.
    fn execute_draw_list(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        draw_list: &DrawList,
    ) -> Result<(), RenderGraphError> {
        for draw_call in &draw_list.draws {
            self.execute_draw_call(cmd, draw_call)?;
        }
        Ok(())
    }

    /// Execute a single draw call.
    fn execute_draw_call(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        draw_call: &crate::renderer::types::DrawCall,
    ) -> Result<(), RenderGraphError> {
        // Get mesh from registry
        let mesh = self.renderer.asset_registry
            .get_mesh(draw_call.mesh)
            .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

        // Get material from registry
        let material = self.renderer.asset_registry
            .get_material(draw_call.material)
            .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

        // Get pipeline handles from registry
        let (pipeline, layout) = self.renderer.asset_registry
            .get_pipeline_vk_handles(material.pipeline)
            .ok_or_else(|| RenderGraphError::InvalidPipelineHandle(material.pipeline))?;

        // Bind graphics pipeline
        unsafe {
            self.renderer.context.device.cmd_bind_pipeline(
                cmd.vk_command_buffer(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline,
            );
        }

        // Bind vertex buffer
        if let Some(vb) = &mesh.vertex_buffer {
            cmd.bind_vertex_buffer(vb.object(), 0);
        }

        // Bind index buffer
        if let Some(ib) = &mesh.index_buffer {
            cmd.bind_index_buffer(ib.object(), 0, vk::IndexType::UINT32);
        }

        // Bind descriptor sets
        self.bind_descriptor_sets(cmd, layout, material)?;

        // Push constants (object transform index)
        self.push_object_constants(cmd, layout, draw_call)?;

        // Draw indexed
        let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);
        cmd.draw_indexed(index_count, 1, 0, 0, 0);

        Ok(())
    }

    /// Bind descriptor sets for a draw call.
    fn bind_descriptor_sets(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        material: &crate::renderer::registry::MaterialAsset,
    ) -> Result<(), RenderGraphError> {
        // Set 0: Frame uniforms (from StorageUniformManager)
        // TODO: Wire up frame descriptor set from storage manager
        // For now, we skip set 0 binding

        // Set 1: Object uniforms (transforms, material params)
        // TODO: Wire up object descriptor set from storage manager
        // For now, we skip set 1 binding

        // Set 2: Bindless textures
        if material.uses_bindless {
            let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();
            cmd.bind_descriptor_sets(pipeline_layout, 2, &[bindless_ds], &[]);
        }

        Ok(())
    }

    /// Push object constants for a draw call.
    fn push_object_constants(
        &mut self,
        cmd: &crate::vulkan::commandbuffer::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
        draw_call: &crate::renderer::types::DrawCall,
    ) -> Result<(), RenderGraphError> {
        #[repr(C)]
        #[derive(Copy, Clone, Pod, Zeroable)]
        struct PushConstants {
            object_index: u32,
            material_index: u32,
        }

        let constants = PushConstants {
            object_index: 0, // TODO: Use actual object index from draw call
            material_index: draw_call.material.index() as u32,
        };

        cmd.push_constants(
            pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &constants,
        );

        Ok(())
    }

    /// Get the object descriptor set for instance data.
    fn get_object_descriptor_set(&self) -> Option<vk::DescriptorSet> {
        // TODO: Create or get object descriptor set from storage uniform manager
        None
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
