//! Vulkan translation of the canonical reflected compute interface.

use super::{BufferUsage, ComputePipelineDesc, RenderGraphError};
use crate::vulkan::context::VulkanContext;
use ash::vk;
use std::{ffi::CString, rc::Rc};

pub(crate) struct VulkanGraphComputePipeline {
    context: Rc<VulkanContext>,
    pub(crate) pipeline: vk::Pipeline,
    pub(crate) layout: vk::PipelineLayout,
    pub(crate) interface: super::ComputeInterface,
    descriptor_layout: vk::DescriptorSetLayout,
}

impl VulkanGraphComputePipeline {
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        descriptor: &ComputePipelineDesc,
    ) -> Result<Self, RenderGraphError> {
        let fail = |reason: String| RenderGraphError::BackendError(reason);
        let interface = descriptor.interface().map_err(fail)?;
        let entry =
            CString::new(descriptor.entry.as_str()).map_err(|error| fail(error.to_string()))?;
        if context.push_descriptor_khr.is_none() {
            return Err(fail(
                "Compute commands require Vulkan push descriptors".into(),
            ));
        }
        let mut module = naga::front::wgsl::parse_str(&descriptor.wgsl)
            .map_err(|error| fail(error.to_string()))?;
        for (_, variable) in module.global_variables.iter_mut() {
            if let Some(binding) = &mut variable.binding {
                let slot = interface
                    .bindings
                    .iter()
                    .position(|slot| slot.group == binding.group && slot.binding == binding.binding)
                    .ok_or_else(|| fail("Reflected compute binding is absent".into()))?;
                binding.group = 0;
                binding.binding = slot as u32;
            }
        }
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|error| fail(error.to_string()))?;
        let spirv = naga::back::spv::write_vec(
            &module,
            &info,
            &naga::back::spv::Options::default(),
            Some(&naga::back::spv::PipelineOptions {
                shader_stage: naga::ShaderStage::Compute,
                entry_point: descriptor.entry.clone(),
            }),
        )
        .map_err(|error| fail(error.to_string()))?;
        let bindings = interface
            .bindings
            .iter()
            .enumerate()
            .map(|(index, binding)| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(index as u32)
                    .descriptor_type(if binding.usage == BufferUsage::Uniform {
                        vk::DescriptorType::UNIFORM_BUFFER
                    } else {
                        vk::DescriptorType::STORAGE_BUFFER
                    })
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect::<Vec<_>>();
        let device = &context.device;
        let descriptor_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default()
                    .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR)
                    .bindings(&bindings),
                None,
            )
        }
        .map_err(|error| fail(error.to_string()))?;
        let mut native = Self {
            context: context.clone(),
            pipeline: vk::Pipeline::null(),
            layout: vk::PipelineLayout::null(),
            descriptor_layout,
            interface,
        };
        native.layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&[descriptor_layout]),
                None,
            )
        }
        .map_err(|error| fail(error.to_string()))?;
        let shader = unsafe {
            device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&spirv), None)
        }
        .map_err(|error| fail(error.to_string()))?;
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader)
            .name(&entry);
        let result = unsafe {
            device.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(stage)
                    .layout(native.layout)],
                None,
            )
        };
        unsafe {
            device.destroy_shader_module(shader, None);
        }
        native.pipeline = result.map_err(|(_, error)| fail(error.to_string()))?[0];
        Ok(native)
    }
}

impl Drop for VulkanGraphComputePipeline {
    fn drop(&mut self) {
        unsafe {
            self.context.device.destroy_pipeline(self.pipeline, None);
            self.context
                .device
                .destroy_pipeline_layout(self.layout, None);
            self.context
                .device
                .destroy_descriptor_set_layout(self.descriptor_layout, None);
        }
    }
}
