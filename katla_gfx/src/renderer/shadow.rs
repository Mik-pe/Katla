use super::FRAMES_IN_FLIGHT;
use crate::RendererError;
use crate::handle::PipelineHandle;
use ash::vk;
use log::info;

/// Bundles all shadow-related state from VulkanRenderer.
pub(crate) struct ShadowState {
    /// CSM cascade computation
    pub csm: Option<crate::shadow::CascadeShadowMap>,
    /// GPU buffers (shadow data storage buffer, atlas view, sampler)
    pub buffers: Option<crate::shadow::ShadowBuffers>,
    /// Descriptor set layout (Set 4, owned by renderer)
    pub descriptor_layout: Option<vk::DescriptorSetLayout>,
    /// Comparison sampler for depth comparison
    pub sampler: Option<vk::Sampler>,
    /// Descriptor pool for per-frame shadow descriptor sets
    pub descriptor_pool: Option<vk::DescriptorPool>,
    /// Per-frame descriptor sets (Set 4)
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    /// Depth-only pipeline for shadow map rendering
    pub pipeline: Option<PipelineHandle>,
    /// Depth-only pipeline for skinned mesh shadow rendering
    pub pipeline_skinned: Option<PipelineHandle>,
    /// Empty descriptor set layout (Set 1 placeholder for shadow pipeline)
    pub empty_descriptor_layout: Option<vk::DescriptorSetLayout>,
    /// Cascade descriptor set layout (Set 2 for shadow depth shader)
    pub cascade_descriptor_layout: Option<vk::DescriptorSetLayout>,
    /// Per-frame cascade descriptor sets (Set 2)
    pub cascade_descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per-frame cascade storage buffers (CPU→GPU, contains ShadowCascadeGPU array)
    pub cascade_buffers: Vec<vk::Buffer>,
    /// Per-frame cascade buffer allocations
    pub cascade_allocations: Vec<gpu_allocator::vulkan::Allocation>,
    /// Per-frame cascade buffer mapped pointers
    pub cascade_mapped_ptrs: Vec<*mut u8>,
    /// Cascade descriptor pool
    pub cascade_descriptor_pool: Option<vk::DescriptorPool>,
}

impl Default for ShadowState {
    fn default() -> Self {
        Self {
            csm: None,
            buffers: None,
            descriptor_layout: None,
            sampler: None,
            descriptor_pool: None,
            descriptor_sets: Vec::new(),
            pipeline: None,
            pipeline_skinned: None,
            empty_descriptor_layout: None,
            cascade_descriptor_layout: None,
            cascade_descriptor_sets: Vec::new(),
            cascade_buffers: Vec::new(),
            cascade_allocations: Vec::new(),
            cascade_mapped_ptrs: Vec::new(),
            cascade_descriptor_pool: None,
        }
    }
}

impl super::VulkanRenderer {
    /// Initialize the shadow system for CSM (Cascaded Shadow Maps).
    ///
    /// Must be called after `init_light_culling()` and before compiling PBR materials,
    /// because PBR pipelines need Set 4 for shadow data in their layout.
    pub fn init_shadow_resources(
        &mut self,
        shadow_atlas_view: Option<vk::ImageView>,
        params: crate::shadow::CascadeParams,
    ) -> Result<(), RendererError> {
        info!("Initializing shadow resources...");

        let device = &self.context.device;

        // Create comparison sampler for shadow depth comparison
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .compare_enable(true)
            .compare_op(vk::CompareOp::LESS_OR_EQUAL)
            .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
            .min_lod(0.0)
            .max_lod(1.0);

        let shadow_sampler = unsafe {
            device.create_sampler(&sampler_info, None).map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create shadow sampler: {:?}",
                    e
                ))
            })?
        };

        // Create shadow descriptor set layout (Set 4):
        // Binding 0: storage buffer (ShadowFrameData)
        // Binding 1: sampled image (shadow atlas depth texture)
        // Binding 2: comparison sampler
        let shadow_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];

        let shadow_binding_flags = [
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
        ];

        let mut shadow_binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                .binding_flags(&shadow_binding_flags);

        let shadow_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&shadow_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut shadow_binding_flags_info);

        let shadow_descriptor_layout = unsafe {
            device
                .create_descriptor_set_layout(&shadow_layout_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create shadow descriptor layout: {:?}",
                        e
                    ))
                })?
        };

        // Set shadow descriptor layout in material compiler so PBR pipelines include Set 4
        self.material_compiler
            .set_shadow_descriptor_layout(shadow_descriptor_layout);

        // Create descriptor pool for per-frame shadow descriptor sets
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(FRAMES_IN_FLIGHT as u32),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(FRAMES_IN_FLIGHT as u32),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(FRAMES_IN_FLIGHT as u32),
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(FRAMES_IN_FLIGHT as u32)
            .pool_sizes(&pool_sizes)
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND);

        let shadow_descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create shadow descriptor pool: {:?}",
                        e
                    ))
                })?
        };

        // Allocate per-frame descriptor sets
        let layouts: Vec<_> = (0..FRAMES_IN_FLIGHT)
            .map(|_| shadow_descriptor_layout)
            .collect();
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(shadow_descriptor_pool)
            .set_layouts(&layouts);

        let shadow_descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to allocate shadow descriptor sets: {:?}",
                        e
                    ))
                })?
        };

        // Create CSM cascade computation
        let shadow_csm = crate::shadow::CascadeShadowMap::new(params);

        // Create shadow buffers (storage buffer for ShadowFrameData)
        let shadow_buffers = crate::shadow::ShadowBuffers::new(
            self.context.clone(),
            shadow_atlas_view,
            shadow_sampler,
        )
        .map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to create shadow buffers: {}", e))
        })?;

        self.shadow.csm = Some(shadow_csm);
        self.shadow.buffers = Some(shadow_buffers);
        self.shadow.descriptor_layout = Some(shadow_descriptor_layout);
        self.shadow.sampler = Some(shadow_sampler);
        self.shadow.descriptor_pool = Some(shadow_descriptor_pool);
        self.shadow.descriptor_sets = shadow_descriptor_sets;

        info!(
            "Shadow resources initialized (CSM, {} cascades)",
            crate::shadow::cascade::MAX_CASCADES
        );
        Ok(())
    }

    /// Update shadow cascades and upload GPU data for the current frame.
    ///
    /// Call this once per frame after setting frame uniforms but before rendering.
    pub fn update_shadows(&mut self, light_direction: [f32; 3]) {
        if let Some(ref mut csm) = self.shadow.csm {
            csm.update(
                light_direction,
                &self.frame_uniforms.view_matrix,
                &self.frame_uniforms.proj_matrix,
            );

            if let Some(ref mut buffers) = self.shadow.buffers {
                let gpu_data = csm.gpu_data();
                buffers.upload_shadow_data(&gpu_data);
            }
        }
    }

    /// Get the shadow descriptor set for the current frame.
    pub fn shadow_descriptor_set(&self) -> Option<vk::DescriptorSet> {
        let frame_idx = self.current_frame();
        self.shadow.descriptor_sets.get(frame_idx).copied()
    }

    /// Update the shadow atlas image view for a specific frame (call on resize/recreation).
    pub fn set_shadow_atlas_view(&mut self, frame_idx: usize, view: vk::ImageView) {
        if let Some(ref mut buffers) = self.shadow.buffers {
            buffers.set_shadow_atlas_view(frame_idx, view);
        }
    }

    /// Get the shadow comparison sampler.
    pub fn shadow_sampler(&self) -> Option<vk::Sampler> {
        self.shadow.sampler
    }

    /// Initialize the shadow depth pipeline and cascade descriptor infrastructure.
    ///
    /// Must be called after `init_shadow_resources()` and before frame graph execution.
    /// Creates:
    /// - A depth-only pipeline from `shadow_depth.wgsl` (front-face culled, depth bias)
    /// - Cascade descriptor set layout (Set 2) for the shadow depth shader
    /// - Per-frame cascade descriptor sets
    /// - Cascade storage buffer (CPU→GPU mapped)
    pub fn init_shadow_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::shadow::cascade::ShadowCascadeGPU;
        use crate::vulkan::material::builder::PipelineBuilder;
        use crate::vulkan::vertexbinding::VertexFormat;

        let device = &self.context.device;

        // Create cascade descriptor set layout (Set 2):
        // Binding 0: storage buffer (array<ShadowCascadeData, 4>)
        // Binding 1: storage buffer (ShadowParams)
        let cascade_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let cascade_binding_flags = [
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
        ];

        let mut cascade_binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                .binding_flags(&cascade_binding_flags);

        let cascade_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&cascade_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut cascade_binding_flags_info);

        let cascade_layout = unsafe {
            device
                .create_descriptor_set_layout(&cascade_layout_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create shadow cascade descriptor layout: {:?}",
                        e
                    ))
                })?
        };

        // Create descriptor pool for per-frame cascade descriptor sets (2 bindings per set)
        let cascade_pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(FRAMES_IN_FLIGHT as u32 * 2)];

        let cascade_pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(FRAMES_IN_FLIGHT as u32)
            .pool_sizes(&cascade_pool_sizes)
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND);

        let cascade_pool = unsafe {
            device
                .create_descriptor_pool(&cascade_pool_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create shadow cascade descriptor pool: {:?}",
                        e
                    ))
                })?
        };

        // Allocate per-frame cascade descriptor sets
        let cascade_layouts: Vec<_> = (0..FRAMES_IN_FLIGHT).map(|_| cascade_layout).collect();
        let cascade_allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(cascade_pool)
            .set_layouts(&cascade_layouts);

        let cascade_descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(&cascade_allocate_info)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to allocate shadow cascade descriptor sets: {:?}",
                        e
                    ))
                })?
        };

        // Create per-frame cascade storage buffers (4 x ShadowCascadeGPU + ShadowParams per frame)
        let cascade_data_size =
            std::mem::size_of::<ShadowCascadeGPU>() * crate::shadow::cascade::MAX_CASCADES;
        let params_size = 16u64; // ShadowParams: cascade_index(u32) + bias(f32) + pad(vec2f)
        let per_frame_buffer_size = cascade_data_size as u64 + params_size;

        let mut cascade_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut cascade_allocations = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut cascade_mapped_ptrs = Vec::with_capacity(FRAMES_IN_FLIGHT);

        for _frame in 0..FRAMES_IN_FLIGHT {
            let buffer_info = vk::BufferCreateInfo::default()
                .size(per_frame_buffer_size)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let (buffer, allocation) = self
                .context
                .allocate_buffer(&buffer_info, gpu_allocator::MemoryLocation::CpuToGpu);

            let mapped_ptr = self.context.map_buffer(&allocation);
            unsafe {
                std::ptr::write_bytes(mapped_ptr, 0, per_frame_buffer_size as usize);
            }

            cascade_buffers.push(buffer);
            cascade_allocations.push(allocation);
            cascade_mapped_ptrs.push(mapped_ptr);
        }

        // Write descriptor sets with two bindings per frame:
        // Binding 0: cascade array (4 x ShadowCascadeGPU)
        // Binding 1: shadow params (ShadowParams)
        for (frame_idx, &descriptor_set) in cascade_descriptor_sets.iter().enumerate() {
            let cascade_buffer_info = vk::DescriptorBufferInfo::default()
                .buffer(cascade_buffers[frame_idx])
                .offset(0)
                .range(cascade_data_size as u64);

            let params_buffer_info = vk::DescriptorBufferInfo::default()
                .buffer(cascade_buffers[frame_idx])
                .offset(cascade_data_size as u64)
                .range(params_size);

            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(std::slice::from_ref(&cascade_buffer_info)),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(std::slice::from_ref(&params_buffer_info)),
            ];

            unsafe {
                device.update_descriptor_sets(&writes, &[]);
            }
        }

        // Compile shadow depth shader
        let mut cache = self.material_compiler.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load shadow vertex shader: {:?}",
                    e
                ))
            })?;
        let frag_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load shadow fragment shader: {:?}",
                    e
                ))
            })?;
        drop(cache);

        // Build shadow depth pipeline:
        // - Set 0: storage uniforms (frame_data + objects)
        // - Set 1: empty placeholder (matches PBR pipeline layout numbering)
        // - Set 2: shadow cascades

        let empty_descriptor_layout = unsafe {
            device
                .create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default(), None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create empty descriptor layout for shadow pipeline: {:?}",
                        e
                    ))
                })?
        };
        self.shadow.empty_descriptor_layout = Some(empty_descriptor_layout);
        // - Front-face culling (reduces self-shadowing)
        // - Hardware depth bias (slope + constant)
        // - Depth-only output (D32Sfloat, no color)
        let storage_layout = self.storage_descriptor_sets[0].layout();

        let cascade_params = self
            .shadow
            .csm
            .as_ref()
            .map(|csm| csm.params().clone())
            .unwrap_or_default();

        let pipeline = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![
                storage_layout,
                empty_descriptor_layout,
                cascade_layout,
            ])
            .with_soa_attribute(0, VertexFormat::RGB32f) // position
            .with_depth_test(true, true, crate::pipeline::CompareOp::Less)
            .with_cull_mode(CullMode::Front, FrontFace::CounterClockwise)
            .with_depth_bias(
                cascade_params.depth_bias_constant,
                cascade_params.depth_bias_slope,
                0.0,
            )
            .with_rendering_formats(None, Some(crate::texture::ImageFormat::D32Sfloat))
            .build_dynamic()
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to build shadow pipeline: {:?}",
                    e
                ))
            })?;

        let pipeline_handle = self.asset_registry.register_pipeline(pipeline);

        self.shadow.pipeline = Some(pipeline_handle);
        self.shadow.cascade_descriptor_layout = Some(cascade_layout);
        self.shadow.cascade_descriptor_sets = cascade_descriptor_sets;
        self.shadow.cascade_buffers = cascade_buffers;
        self.shadow.cascade_allocations = cascade_allocations;
        self.shadow.cascade_mapped_ptrs = cascade_mapped_ptrs;
        self.shadow.cascade_descriptor_pool = Some(cascade_pool);

        info!(
            "Shadow depth pipeline initialized (4 cascades, front-face culled, hardware depth bias)"
        );
        Ok(())
    }

    /// Upload shadow cascade GPU data for the current frame.
    ///
    /// Call this after `update_shadows()` to upload cascade view_proj matrices
    /// to the cascade storage buffer for the shadow depth shader.
    pub fn upload_shadow_cascades(&mut self) {
        if let (Some(csm), frame_idx) = (&self.shadow.csm, self.current_frame()) {
            if frame_idx >= self.shadow.cascade_mapped_ptrs.len() {
                return;
            }
            let mapped_ptr = self.shadow.cascade_mapped_ptrs[frame_idx];
            let gpu_data = csm.gpu_data();
            let cascade_size = std::mem::size_of::<crate::shadow::cascade::ShadowCascadeGPU>();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    gpu_data.cascades.as_ptr() as *const u8,
                    mapped_ptr,
                    cascade_size * crate::shadow::cascade::MAX_CASCADES,
                );
            }
            if frame_idx < self.shadow.cascade_allocations.len() {
                self.context.flush_mapped_memory(
                    &self.shadow.cascade_allocations[frame_idx],
                    0,
                    (cascade_size * crate::shadow::cascade::MAX_CASCADES) as u64,
                );
            }
        }
    }

    /// Update shadow params for a specific cascade draw.
    ///
    /// Call this before each cascade draw to set the active cascade index and bias.
    pub fn set_shadow_cascade_params(&self, cascade_index: u32, bias: f32) {
        let frame_idx = self.current_frame();
        if frame_idx >= self.shadow.cascade_mapped_ptrs.len() {
            return;
        }
        let mapped_ptr = self.shadow.cascade_mapped_ptrs[frame_idx];
        let cascade_size = std::mem::size_of::<crate::shadow::cascade::ShadowCascadeGPU>()
            * crate::shadow::cascade::MAX_CASCADES;
        let params_offset = cascade_size;
        let params: [u32; 4] = [cascade_index, bias.to_bits(), 0, 0];
        unsafe {
            std::ptr::copy_nonoverlapping(
                params.as_ptr() as *const u8,
                mapped_ptr.add(params_offset),
                16,
            );
        }
        if frame_idx < self.shadow.cascade_allocations.len() {
            self.context.flush_mapped_memory(
                &self.shadow.cascade_allocations[frame_idx],
                params_offset as u64,
                16,
            );
        }
    }

    /// Get the shadow depth pipeline handle.
    pub fn shadow_pipeline(&self) -> Option<PipelineHandle> {
        self.shadow.pipeline
    }

    /// Get the skinned shadow depth pipeline handle.
    pub fn shadow_pipeline_skinned(&self) -> Option<PipelineHandle> {
        self.shadow.pipeline_skinned
    }

    /// Initialize the skinned shadow depth pipeline.
    ///
    /// Same as the regular shadow pipeline but uses the skinned vertex layout
    /// (includes joint indices/weights) and binds skeleton joint matrices at Set 3.
    /// Uses the same cascade descriptor sets as the regular shadow pipeline.
    pub fn init_shadow_pipeline_skinned(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::vulkan::material::builder::PipelineBuilder;
        use crate::vulkan::vertexbinding::VertexFormat;

        let storage_layout = self.storage_descriptor_sets[0].layout();
        let empty_descriptor_layout = self.shadow.empty_descriptor_layout.ok_or_else(|| {
            RendererError::InitializationFailed(
                "Shadow empty descriptor layout not initialized. Call init_shadow_pipeline() first."
                    .to_string(),
            )
        })?;
        let cascade_layout = self.shadow.cascade_descriptor_layout.ok_or_else(|| {
            RendererError::InitializationFailed(
                "Shadow cascade descriptor layout not initialized. Call init_shadow_pipeline() first."
                    .to_string(),
            )
        })?;
        let skeleton_layout = self.material_compiler.skeleton_descriptor_layout();

        let mut cache = self.material_compiler.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load skinned shadow vertex shader: {:?}",
                    e
                ))
            })?;
        let frag_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load skinned shadow fragment shader: {:?}",
                    e
                ))
            })?;
        drop(cache);

        let cascade_params = self
            .shadow
            .csm
            .as_ref()
            .map(|csm| csm.params().clone())
            .unwrap_or_default();

        let pipeline = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![
                storage_layout,
                empty_descriptor_layout,
                cascade_layout,
                skeleton_layout,
            ])
            .with_soa_attribute(0, VertexFormat::RGB32f) // position
            .with_soa_attribute(4, VertexFormat::RGBA16u) // joint_indices
            .with_soa_attribute(5, VertexFormat::RGBA32f) // joint_weights
            .with_depth_test(true, true, crate::pipeline::CompareOp::Less)
            .with_cull_mode(CullMode::Front, FrontFace::CounterClockwise)
            .with_depth_bias(
                cascade_params.depth_bias_constant,
                cascade_params.depth_bias_slope,
                0.0,
            )
            .with_rendering_formats(None, Some(crate::texture::ImageFormat::D32Sfloat))
            .build_dynamic()
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to build skinned shadow pipeline: {:?}",
                    e
                ))
            })?;

        let pipeline_handle = self.asset_registry.register_pipeline(pipeline);
        self.shadow.pipeline_skinned = Some(pipeline_handle);

        info!(
            "Skinned shadow depth pipeline initialized (4 cascades, front-face culled, depth bias)"
        );
        Ok(())
    }

    /// Get the cascade descriptor set for the current frame (Set 2).
    pub fn shadow_cascade_descriptor_set(&self) -> Option<vk::DescriptorSet> {
        let frame_idx = self.current_frame();
        self.shadow.cascade_descriptor_sets.get(frame_idx).copied()
    }

    /// Upload shadow data and bind shadow descriptors for the current frame.
    ///
    /// Call this inside the render graph execution (after binding pipeline, before draw calls).
    pub fn bind_shadow_descriptors(
        &self,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
    ) {
        let frame_idx = self.current_frame();

        if let (Some(buffers), Some(&descriptor_set)) = (
            &self.shadow.buffers,
            self.shadow.descriptor_sets.get(frame_idx),
        ) {
            if let Err(e) = buffers.update_and_bind_descriptors(
                cmd,
                &self.context.device,
                pipeline_layout,
                descriptor_set,
                frame_idx,
            ) {
                log::warn!("Failed to bind shadow descriptors: {}", e);
            }
        }
    }
}
