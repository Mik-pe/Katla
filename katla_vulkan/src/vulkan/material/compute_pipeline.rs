//! Compute pipeline builder for Vulkan 1.3.
//!
//! Provides a builder pattern for creating compute pipelines, similar to
//! `PipelineBuilder` but for compute shaders. This is used for GPU compute
//! operations like particle simulation.
//!
//! # Example
//!
//! ```ignore
//! use katla_vulkan::vulkan::material::{ComputePipelineBuilder, ShaderModule};
//!
//! let compute_shader = ShaderModule::from_wgsl(&context, include_str!("particle_sim.wgsl"), "cs_main")?;
//!
//! let pipeline = ComputePipelineBuilder::new(context.clone())
//!     .with_shader(compute_shader.shader_module)
//!     .with_descriptor_layouts(vec![descriptor_set_layout])
//!     .add_push_constant_range(vk::ShaderStageFlags::COMPUTE, 0, std::mem::size_of::<ParticlePushConstants>() as u32)
//!     .build()?;
//! ```

use std::{ffi::CString, rc::Rc};

use ash::vk;

use crate::VulkanContext;

/// Builder for creating compute pipelines.
///
/// Provides a fluent API for configuring compute pipelines before creation.
/// Unlike graphics pipelines, compute pipelines only have a single shader stage.
pub struct ComputePipelineBuilder {
    context: Rc<VulkanContext>,
    compute_shader: Option<vk::ShaderModule>,
    entry_point: CString,
    descriptor_layouts: Vec<vk::DescriptorSetLayout>,
    push_constant_ranges: Vec<vk::PushConstantRange>,
}

impl ComputePipelineBuilder {
    /// Create a new compute pipeline builder.
    pub fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            compute_shader: None,
            entry_point: CString::new("cs_main").unwrap(),
            descriptor_layouts: Vec::new(),
            push_constant_ranges: Vec::new(),
        }
    }

    /// Set the compute shader module.
    pub fn with_shader(mut self, shader: vk::ShaderModule) -> Self {
        self.compute_shader = Some(shader);
        self
    }

    /// Set a custom entry point name (default: "cs_main").
    pub fn with_entry_point(mut self, entry_point: CString) -> Self {
        self.entry_point = entry_point;
        self
    }

    /// Set the descriptor set layouts.
    pub fn with_descriptor_layouts(mut self, layouts: Vec<vk::DescriptorSetLayout>) -> Self {
        self.descriptor_layouts = layouts;
        self
    }

    /// Add a descriptor set layout.
    pub fn add_descriptor_layout(mut self, layout: vk::DescriptorSetLayout) -> Self {
        self.descriptor_layouts.push(layout);
        self
    }

    /// Set the push constant ranges.
    pub fn with_push_constants(mut self, ranges: Vec<vk::PushConstantRange>) -> Self {
        self.push_constant_ranges = ranges;
        self
    }

    /// Add a push constant range.
    pub fn add_push_constant_range(
        mut self,
        stages: vk::ShaderStageFlags,
        offset: u32,
        size: u32,
    ) -> Self {
        self.push_constant_ranges.push(
            vk::PushConstantRange::default()
                .stage_flags(stages)
                .offset(offset)
                .size(size),
        );
        self
    }

    /// Build the compute pipeline.
    ///
    /// # Returns
    /// A `ComputePipeline` on success, or an error if creation fails.
    pub fn build(self) -> Result<ComputePipeline, ComputePipelineError> {
        let compute_shader = self
            .compute_shader
            .ok_or(ComputePipelineError::MissingComputeShader)?;

        // Create shader stage
        let shader_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(compute_shader)
            .name(&self.entry_point);

        // Create pipeline layout
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&self.descriptor_layouts)
            .push_constant_ranges(&self.push_constant_ranges);

        let pipeline_layout = unsafe {
            self.context
                .device
                .create_pipeline_layout(&pipeline_layout_info, None)
        }
        .map_err(ComputePipelineError::LayoutCreationFailed)?;

        // Create compute pipeline
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(shader_stage)
            .layout(pipeline_layout);

        let pipeline = unsafe {
            self.context.device.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[create_info],
                None,
            )
        }
        .map_err(|e| ComputePipelineError::CreationFailed(e.1))?[0];

        Ok(ComputePipeline {
            handle: pipeline,
            layout: pipeline_layout,
            device: self.context.device.clone(),
        })
    }
}

/// A Vulkan compute pipeline.
///
/// Contains the pipeline handle and layout for dispatching compute operations.
pub struct ComputePipeline {
    /// The Vulkan pipeline handle.
    pub handle: vk::Pipeline,
    /// The pipeline layout.
    pub layout: vk::PipelineLayout,
    device: ash::Device,
}

impl ComputePipeline {
    /// Get the pipeline handle.
    pub fn vk_pipeline(&self) -> vk::Pipeline {
        self.handle
    }

    /// Get the pipeline layout.
    pub fn vk_layout(&self) -> vk::PipelineLayout {
        self.layout
    }

    /// Destroy the pipeline resources.
    pub fn destroy(&self) {
        unsafe {
            self.device.destroy_pipeline(self.handle, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        self.destroy();
    }
}

/// Errors that can occur when creating a compute pipeline.
#[derive(Debug)]
pub enum ComputePipelineError {
    /// No compute shader was provided.
    MissingComputeShader,
    /// Failed to create the pipeline layout.
    LayoutCreationFailed(vk::Result),
    /// Failed to create the compute pipeline.
    CreationFailed(vk::Result),
}

impl std::fmt::Display for ComputePipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingComputeShader => write!(f, "Compute shader not provided"),
            Self::LayoutCreationFailed(e) => {
                write!(f, "Failed to create pipeline layout: {:?}", e)
            }
            Self::CreationFailed(e) => write!(f, "Failed to create compute pipeline: {:?}", e),
        }
    }
}

impl std::error::Error for ComputePipelineError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_pipeline_builder_creation() {
        // Test that the builder can be created (without Vulkan context)
        // The actual build requires a valid context
    }

    #[test]
    fn test_compute_pipeline_error_display() {
        let err = ComputePipelineError::MissingComputeShader;
        assert_eq!(format!("{}", err), "Compute shader not provided");
    }
}
