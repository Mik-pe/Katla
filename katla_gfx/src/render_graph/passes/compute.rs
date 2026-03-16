//! Compute pass template.
//!
//! General-purpose compute dispatch for particle simulation, physics, etc.

use crate::handle::PipelineHandle;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::error::RenderGraphError;
use super::super::pass::PassType;
use ash::vk;
use std::collections::HashMap;

/// Compute pass template.
///
/// For particle simulation, physics, and other GPU compute work.
///
/// # Example
///
/// ```ignore
/// let particle_update = ComputePass::new("particle_update")
///     .pipeline(compute_pipeline)
///     .workgroup_count(64);
///
/// let graph = FrameGraph::builder()
///     .add_pass(particle_update)
///     .build(&renderer)?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("particle_update").dispatch();
/// })?;
/// ```
pub struct ComputePass {
    name: String,
    pipeline: Option<PipelineHandle>,
    workgroup_count: u32,
}

impl ComputePass {
    /// Create a new compute pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            pipeline: None,
            workgroup_count: 1,
        }
    }

    /// Set the compute pipeline for this pass.
    pub fn pipeline(mut self, pipeline: PipelineHandle) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    /// Set the workgroup count for dispatch.
    ///
    /// This is the X dimension of dispatch (Y and Z are always 1).
    pub fn workgroup_count(mut self, count: u32) -> Self {
        self.workgroup_count = count;
        self
    }
}

impl PassBuilder for ComputePass {
    fn as_builder(self) -> InternalPassBuilder {
        InternalPassBuilder {
            name: self.name.clone(),
            pass_type: PassType::Compute,
            reads: Vec::new(),
            writes: Vec::new(),
            pipeline: self.pipeline,
            tonemap_params: None,
            material: None,
            output_format: None,
            build_fn: Box::new(
                move |_resource_map: &HashMap<
                    String,
                    crate::render_graph::resource::GraphResourceHandle,
                >| {
                    Ok(Box::new(ComputePassData {
                        name: self.name.clone(),
                        pipeline: self.pipeline.ok_or_else(|| {
                            RenderGraphError::PipelineNotSet(
                                "Compute pass missing pipeline".to_string(),
                            )
                        })?,
                        workgroup_count: self.workgroup_count,
                    }))
                },
            ),
            uses_depth: false,
        }
    }
}

/// Compiled compute pass data.
pub struct ComputePassData {
    name: String,
    pipeline: PipelineHandle,
    workgroup_count: u32,
}

impl ComputePassData {
    /// Execute the compute pass.
    pub fn execute(
        &self,
        renderer: &crate::renderer::VulkanRenderer,
        command_buffer: vk::CommandBuffer,
    ) -> Result<(), String> {
        let device = &renderer.context.device;

        // Get compute pipeline from registry
        let compute_pipeline = renderer
            .asset_registry
            .get_pipeline(self.pipeline)
            .ok_or("Failed to get compute pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        // Bind compute pipeline
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Dispatch compute shader
        unsafe {
            device.cmd_dispatch(command_buffer, self.workgroup_count, 1, 1);
        }

        log::debug!(
            "Executed compute pass '{}' with {} workgroups",
            self.name,
            self.workgroup_count
        );

        Ok(())
    }
}
