#![allow(dead_code)]
//! Compute pipeline builder for Vulkan 1.3.
//!
//! Provides a builder pattern for creating compute pipelines, similar to
//! `PipelineBuilder` but for compute shaders. This is used for GPU compute
//! operations like particle simulation.

use ash::vk;
use std::{ffi::CString, rc::Rc};

use super::super::context::VulkanContext;
use crate::sync::{VkDescriptorSetLayout, VkPipeline, VkPipelineLayout};
use crate::vulkan::pipeline_state::ShaderStages;

/// Builder for creating compute pipelines.
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

    pub fn with_shader(mut self, shader: crate::sync::VkShaderModule) -> Self {
        self.compute_shader = Some(shader.vk());
        self
    }

    /// Set a custom entry point name (default: "cs_main").
    pub fn with_entry_point(mut self, entry_point: CString) -> Self {
        self.entry_point = entry_point;
        self
    }

    /// Set the descriptor set layouts using wrapper types.
    pub fn with_descriptor_layouts(mut self, layouts: Vec<VkDescriptorSetLayout>) -> Self {
        self.descriptor_layouts = layouts.into_iter().map(|l| l.into()).collect();
        self
    }

    /// Add a descriptor set layout using wrapper type.
    pub fn add_descriptor_layout(mut self, layout: VkDescriptorSetLayout) -> Self {
        self.descriptor_layouts.push(layout.into());
        self
    }

    /// Set the push constant ranges.
    pub fn with_push_constants(mut self, ranges: Vec<vk::PushConstantRange>) -> Self {
        self.push_constant_ranges = ranges;
        self
    }

    /// Add a push constant range.
    pub fn add_push_constant_range(mut self, stages: ShaderStages, offset: u32, size: u32) -> Self {
        let vk_stages: vk::ShaderStageFlags = stages.into();
        self.push_constant_ranges.push(
            vk::PushConstantRange::default()
                .stage_flags(vk_stages)
                .offset(offset)
                .size(size),
        );
        self
    }

    /// Build the compute pipeline.
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
            handle: Some(pipeline),
            layout: Some(pipeline_layout),
            descriptor_layouts: self.descriptor_layouts,
            device: self.context.device.clone(),
        })
    }
}

/// A Vulkan compute pipeline.
pub struct ComputePipeline {
    /// The Vulkan pipeline handle.
    handle: Option<vk::Pipeline>,
    /// The pipeline layout.
    layout: Option<vk::PipelineLayout>,
    /// Descriptor set layouts (owned, for cleanup).
    descriptor_layouts: Vec<vk::DescriptorSetLayout>,
    device: ash::Device,
}

impl ComputePipeline {
    /// Get the pipeline handle as a wrapper type.
    pub fn pipeline(&self) -> VkPipeline {
        VkPipeline::new(self.handle.unwrap_or(vk::Pipeline::null()))
    }

    /// Get the pipeline layout as a wrapper type.
    pub fn pipeline_layout(&self) -> VkPipelineLayout {
        VkPipelineLayout::new(self.layout.unwrap_or(vk::PipelineLayout::null()))
    }

    /// Get the descriptor set layouts used when creating this pipeline.
    pub fn descriptor_set_layouts(&self) -> &[vk::DescriptorSetLayout] {
        &self.descriptor_layouts
    }

    /// Destroy the pipeline resources.
    ///
    /// Uses `take()` to prevent double-free when called explicitly
    /// before Drop runs. Safe to call multiple times.
    pub fn destroy(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe {
                self.device.destroy_pipeline(handle, None);
            }
        }
        if let Some(layout) = self.layout.take() {
            unsafe {
                self.device.destroy_pipeline_layout(layout, None);
            }
        }
        // Note: Descriptor set layouts are NOT destroyed here since they may be shared
        // between multiple pipelines (e.g., particle emit and simulate pipelines share layouts).
        // The caller who created the layouts is responsible for their cleanup.
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
