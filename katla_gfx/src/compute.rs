//! Generic compute pass framework for issuing arbitrary compute dispatches.
//!
//! Provides a builder pattern for creating and dispatching compute pipelines
//! with typed buffer bindings. Designed for validation, debugging, and
//! utility compute work that doesn't need the full particle/render pipeline.
//!
//! # Example
//! ```ignore
//! let pass = ComputePass::builder(context)
//!     .add_storage_buffer(0, buffer, offset, size)
//!     .add_uniform_buffer(1, uniform_buf, 0, 64)
//!     .build(shader, &mut asset_registry)?;
//!
//! // Record dispatch into a command buffer (requires pipeline handles from registry)
//! pass.record_dispatch_with_handles(cmd, pipeline, layout, workgroups_x, 1, 1);
//!
//! // Add barrier after dispatch
//! pass.add_buffer_barrier(cmd, buffer, 0, vk::WHOLE_SIZE,
//!     crate::sync::PipelineStage2Flags::VERTEX_SHADER);
//! ```

use std::rc::Rc;

use ash::vk;
use log::info;

use crate::ComputePipelineBuilder;
use crate::renderer::{AssetRegistry, PipelineHandle};
use crate::sync::{VkBuffer, VkDescriptorSetLayout};
use crate::vulkan::context::VulkanContext;

/// A descriptor binding specification for a compute pass.
#[derive(Clone, Debug)]
pub struct BufferBinding {
    /// Binding index in the descriptor set layout.
    pub binding: u32,
    /// Descriptor type (STORAGE_BUFFER, UNIFORM_BUFFER).
    pub descriptor_type: vk::DescriptorType,
    /// Vulkan buffer handle.
    pub buffer: vk::Buffer,
    /// Byte offset into the buffer.
    pub offset: u64,
    /// Size in bytes (VK_WHOLE_SIZE for entire buffer).
    pub size: u64,
}

/// A compiled compute pass with pipeline, descriptor set, and layout.
///
/// Owns the pipeline (via AssetRegistry handle) and descriptor set.
/// Can record dispatches into any command buffer.
pub struct ComputePass {
    context: Rc<VulkanContext>,
    pipeline_handle: PipelineHandle,
    descriptor_set: vk::DescriptorSet,
    descriptor_pool: vk::DescriptorPool,
    descriptor_layout: vk::DescriptorSetLayout,
    push_descriptor_layout: Option<vk::DescriptorSetLayout>,
    bindings: Vec<BufferBinding>,
}

impl ComputePass {
    /// Create a compute pass builder with regular descriptor sets.
    pub fn builder(context: &Rc<VulkanContext>) -> ComputePassBuilder {
        ComputePassBuilder {
            context: context.clone(),
            bindings: Vec::new(),
            use_push_descriptors: false,
        }
    }

    /// Create a compute pass builder that uses push descriptors.
    ///
    /// Push descriptors are written directly into the command buffer during
    /// `record_dispatch_with_handles`. Use this for bindings that change
    /// every frame.
    pub fn with_push_descriptors(context: &Rc<VulkanContext>) -> ComputePassBuilder {
        ComputePassBuilder {
            context: context.clone(),
            bindings: Vec::new(),
            use_push_descriptors: true,
        }
    }

    /// Record a compute dispatch using pipeline handles from the registry.
    ///
    /// Look up the pipeline via `asset_registry.get_pipeline(pass.pipeline_handle())`
    /// to get `(pipeline, layout)`, then pass them here.
    pub fn record_dispatch_with_handles(
        &self,
        cmd: vk::CommandBuffer,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) {
        let device = &self.context.device;

        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
        }

        if self.push_descriptor_layout.is_some() {
            push_descriptors(&self.context, &self.bindings, cmd, layout);
        } else {
            unsafe {
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    layout,
                    0,
                    std::slice::from_ref(&self.descriptor_set),
                    &[],
                );
            }
        }

        unsafe {
            device.cmd_dispatch(cmd, workgroups_x, workgroups_y, workgroups_z);
        }
    }

    /// Add buffer memory barriers on all storage buffer bindings.
    ///
    /// Ensures compute writes are visible to the specified destination stage.
    pub fn add_storage_barrier(
        &self,
        cmd: vk::CommandBuffer,
        dst_stage: crate::sync::PipelineStage2Flags,
    ) {
        let device = &self.context.device;
        let mut dep_info = crate::sync::DependencyInfo::new();

        for binding in &self.bindings {
            if binding.descriptor_type == vk::DescriptorType::STORAGE_BUFFER {
                dep_info = dep_info.add_buffer_barrier2(crate::sync::BufferMemoryBarrier2 {
                    src_stage_mask: crate::sync::PipelineStage2Flags::COMPUTE_SHADER,
                    dst_stage_mask: dst_stage,
                    src_access_mask: crate::sync::AccessFlags2::SHADER_WRITE,
                    dst_access_mask: crate::sync::AccessFlags2::SHADER_READ,
                    src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                    dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                    buffer: VkBuffer::new(binding.buffer),
                    offset: binding.offset,
                    size: binding.size,
                });
            }
        }

        if !dep_info.buffer_barriers.is_empty() {
            dep_info.build(|dep_info| unsafe {
                device.cmd_pipeline_barrier2(cmd, dep_info);
            });
        }
    }

    /// Record a barrier for a specific buffer at a given offset/size.
    pub fn add_buffer_barrier(
        &self,
        cmd: vk::CommandBuffer,
        buffer: vk::Buffer,
        offset: u64,
        size: u64,
        dst_stage: crate::sync::PipelineStage2Flags,
    ) {
        let device = &self.context.device;

        let dep_info = crate::sync::DependencyInfo::new().add_buffer_barrier2(
            crate::sync::BufferMemoryBarrier2 {
                src_stage_mask: crate::sync::PipelineStage2Flags::COMPUTE_SHADER,
                dst_stage_mask: dst_stage,
                src_access_mask: crate::sync::AccessFlags2::SHADER_WRITE,
                dst_access_mask: crate::sync::AccessFlags2::SHADER_READ,
                src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
                buffer: VkBuffer::new(buffer),
                offset,
                size,
            },
        );

        dep_info.build(|dep_info| unsafe {
            device.cmd_pipeline_barrier2(cmd, dep_info);
        });
    }

    /// Update a single buffer binding in the descriptor set.
    ///
    /// Use this for per-frame buffer swaps (e.g., double-buffered counters).
    /// Only valid for regular (non-push) descriptor sets.
    pub fn update_binding(&self, binding_index: usize, buffer: vk::Buffer, offset: u64, size: u64) {
        if self.push_descriptor_layout.is_some() {
            log::warn!("Cannot update binding on push descriptor pass");
            return;
        }

        let binding = match self.bindings.get(binding_index) {
            Some(b) => b,
            None => {
                log::warn!(
                    "Binding index {} out of range ({} bindings)",
                    binding_index,
                    self.bindings.len()
                );
                return;
            }
        };

        let buffer_info = [vk::DescriptorBufferInfo {
            buffer,
            offset,
            range: size,
        }];

        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_set)
            .dst_binding(binding.binding)
            .descriptor_type(binding.descriptor_type)
            .descriptor_count(1)
            .buffer_info(&buffer_info);

        unsafe {
            self.context
                .device
                .update_descriptor_sets(std::slice::from_ref(&write), &[]);
        }
    }

    /// Get the pipeline handle for registry lookup.
    pub fn pipeline_handle(&self) -> PipelineHandle {
        self.pipeline_handle
    }

    /// Destroy GPU resources.
    pub fn destroy(&mut self) {
        let device = &self.context.device;

        unsafe {
            if self.descriptor_pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
            }
            if self.descriptor_layout != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.descriptor_layout, None);
                self.descriptor_layout = vk::DescriptorSetLayout::null();
            }
            if let Some(layout) = self.push_descriptor_layout.take() {
                device.destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}

impl Drop for ComputePass {
    fn drop(&mut self) {
        self.destroy();
    }
}

fn push_descriptors(
    context: &VulkanContext,
    bindings: &[BufferBinding],
    cmd: vk::CommandBuffer,
    layout: vk::PipelineLayout,
) {
    let buffer_infos: Vec<vk::DescriptorBufferInfo> = bindings
        .iter()
        .map(|b| vk::DescriptorBufferInfo {
            buffer: b.buffer,
            offset: b.offset,
            range: b.size,
        })
        .collect();

    let writes: Vec<vk::WriteDescriptorSet> = bindings
        .iter()
        .zip(buffer_infos.chunks(1))
        .map(|(b, info)| {
            vk::WriteDescriptorSet::default()
                .dst_binding(b.binding)
                .descriptor_type(b.descriptor_type)
                .descriptor_count(1)
                .buffer_info(info)
        })
        .collect();

    unsafe {
        if let Some(ref push_ext) = context.push_descriptor_khr {
            push_ext.cmd_push_descriptor_set(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                layout,
                0,
                &writes,
            );
        } else {
            log::error!("Push descriptor extension not available");
        }
    }
}

/// Builder for creating a `ComputePass`.
pub struct ComputePassBuilder {
    context: Rc<VulkanContext>,
    bindings: Vec<BufferBinding>,
    use_push_descriptors: bool,
}

impl ComputePassBuilder {
    /// Add a storage buffer binding.
    pub fn add_storage_buffer(
        mut self,
        binding: u32,
        buffer: vk::Buffer,
        offset: u64,
        size: u64,
    ) -> Self {
        self.bindings.push(BufferBinding {
            binding,
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
            buffer,
            offset,
            size,
        });
        self
    }

    /// Add a uniform buffer binding.
    pub fn add_uniform_buffer(
        mut self,
        binding: u32,
        buffer: vk::Buffer,
        offset: u64,
        size: u64,
    ) -> Self {
        self.bindings.push(BufferBinding {
            binding,
            descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
            buffer,
            offset,
            size,
        });
        self
    }

    /// Add a buffer binding with explicit descriptor type.
    pub fn add_buffer(
        mut self,
        binding: u32,
        descriptor_type: vk::DescriptorType,
        buffer: vk::Buffer,
        offset: u64,
        size: u64,
    ) -> Self {
        self.bindings.push(BufferBinding {
            binding,
            descriptor_type,
            buffer,
            offset,
            size,
        });
        self
    }

    /// Build the compute pass: create descriptor layout, pipeline, and descriptor set.
    pub fn build(
        self,
        shader: crate::sync::VkShaderModule,
        asset_registry: &mut AssetRegistry,
    ) -> Result<ComputePass, String> {
        let device = &self.context.device;

        let mut sorted_bindings = self.bindings.clone();
        sorted_bindings.sort_by_key(|b| b.binding);

        let layout_bindings: Vec<vk::DescriptorSetLayoutBinding> = sorted_bindings
            .iter()
            .map(|b| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(b.binding)
                    .descriptor_type(b.descriptor_type)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();

        let layout_create_flags = if self.use_push_descriptors {
            vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR
        } else {
            vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL
        };

        let binding_flags: Vec<vk::DescriptorBindingFlags> = if !self.use_push_descriptors {
            layout_bindings
                .iter()
                .map(|_| vk::DescriptorBindingFlags::UPDATE_AFTER_BIND)
                .collect()
        } else {
            Vec::new()
        };

        let mut flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let mut layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&layout_bindings)
            .flags(layout_create_flags);

        if !self.use_push_descriptors {
            layout_info = layout_info.push_next(&mut flags_info);
        }

        let descriptor_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .map_err(|e| format!("Failed to create compute pass descriptor layout: {:?}", e))?
        };

        let pipeline = ComputePipelineBuilder::new(self.context.clone())
            .with_shader(shader)
            .add_descriptor_layout(VkDescriptorSetLayout(descriptor_layout))
            .build()
            .map_err(|e| format!("Failed to build compute pipeline: {}", e))?;

        let pipeline_handle = asset_registry.register_compute_pipeline(pipeline);

        let (descriptor_set, descriptor_pool) = if self.use_push_descriptors {
            (vk::DescriptorSet::null(), vk::DescriptorPool::null())
        } else {
            let mut storage_count = 0u32;
            let mut uniform_count = 0u32;
            for b in &sorted_bindings {
                match b.descriptor_type {
                    vk::DescriptorType::STORAGE_BUFFER => storage_count += 1,
                    vk::DescriptorType::UNIFORM_BUFFER => uniform_count += 1,
                    _ => {}
                }
            }

            let mut pool_sizes = Vec::new();
            if storage_count > 0 {
                pool_sizes.push(
                    vk::DescriptorPoolSize::default()
                        .ty(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(storage_count),
                );
            }
            if uniform_count > 0 {
                pool_sizes.push(
                    vk::DescriptorPoolSize::default()
                        .ty(vk::DescriptorType::UNIFORM_BUFFER)
                        .descriptor_count(uniform_count),
                );
            }

            let pool = unsafe {
                device
                    .create_descriptor_pool(
                        &vk::DescriptorPoolCreateInfo::default()
                            .pool_sizes(&pool_sizes)
                            .max_sets(1)
                            .flags(
                                vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET
                                    | vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND,
                            ),
                        None,
                    )
                    .map_err(|e| format!("Failed to create descriptor pool: {:?}", e))?
            };

            let sets = unsafe {
                device
                    .allocate_descriptor_sets(
                        &vk::DescriptorSetAllocateInfo::default()
                            .descriptor_pool(pool)
                            .set_layouts(std::slice::from_ref(&descriptor_layout)),
                    )
                    .map_err(|e| format!("Failed to allocate descriptor set: {:?}", e))?
            };

            let ds = sets[0];

            let buffer_infos: Vec<vk::DescriptorBufferInfo> = sorted_bindings
                .iter()
                .map(|b| vk::DescriptorBufferInfo {
                    buffer: b.buffer,
                    offset: b.offset,
                    range: b.size,
                })
                .collect();

            let writes: Vec<vk::WriteDescriptorSet> = sorted_bindings
                .iter()
                .zip(buffer_infos.chunks(1))
                .map(|(b, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(ds)
                        .dst_binding(b.binding)
                        .descriptor_type(b.descriptor_type)
                        .descriptor_count(1)
                        .buffer_info(info)
                })
                .collect();

            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }

            (ds, pool)
        };

        info!(
            "Created compute pass: {} bindings{}",
            self.bindings.len(),
            if self.use_push_descriptors {
                " (push descriptors)"
            } else {
                ""
            }
        );

        Ok(ComputePass {
            context: self.context,
            pipeline_handle,
            descriptor_set,
            descriptor_pool,
            descriptor_layout,
            push_descriptor_layout: if self.use_push_descriptors {
                Some(descriptor_layout)
            } else {
                None
            },
            bindings: self.bindings,
        })
    }
}
