//! Particle system manager for GPU-based particle effects.
//!
//! Simplified API with 3 core operations:
//! - `new()` - Zero-config initialization (shader paths are constants)
//! - `create_emitter()` - Create particle emitter with config
//! - `destroy_emitter()` - Free emitter resources

use std::collections::HashMap;
use std::rc::Rc;

use ash::vk;

use crate::handle::{EmitterHandle, PipelineHandle};
use crate::renderer::VulkanRenderer;
use crate::vulkan::particle_buffer::{EmitterConfig, FrameData, ParticleBuffer};

/// Default particle shader paths
const PARTICLE_SIM_SHADER: &str = "particles/particle_sim.wgsl";
const PARTICLE_RENDER_SHADER: &str = "particles/particle_render.wgsl";

/// Default maximum particles per emitter
const DEFAULT_MAX_PARTICLES: u32 = 65536;

/// Particle system manager.
///
/// Manages GPU resources for particle simulation and rendering.
/// Each particle emitter gets its own particle buffer.
pub struct ParticleSystem {
    /// Particle emitters indexed by handle
    emitters: HashMap<u32, ParticleEmitter>,
    /// Compute pipeline for particle simulation
    compute_pipeline: Option<PipelineHandle>,
    /// Graphics pipeline for rendering particles as billboards
    render_pipeline: Option<PipelineHandle>,
    /// Descriptor set layouts (owned by pipelines, stored for allocation)
    compute_descriptor_layout_set0: Option<vk::DescriptorSetLayout>,
    compute_descriptor_layout_set1: Option<vk::DescriptorSetLayout>,
    render_descriptor_layout_set1: Option<vk::DescriptorSetLayout>,
    render_descriptor_layout_set0: Option<vk::DescriptorSetLayout>,
    /// Frame uniform buffer for frame data (delta_time, emit_count, etc.)
    frame_uniform_buffer: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,
    /// Descriptor pool for allocating descriptor sets
    descriptor_pool: Option<vk::DescriptorPool>,
    /// Next emitter handle index
    next_handle: u32,
}

/// Internal emitter data
struct ParticleEmitter {
    /// GPU particle buffer
    buffer: ParticleBuffer,
    /// Compute descriptor set (Set 0: particle buffer + frame data)
    compute_descriptor_set: vk::DescriptorSet,
    /// Render descriptor set (Set 1: particle buffer)
    render_descriptor_set: vk::DescriptorSet,
    /// Emitter configuration
    config: EmitterConfig,
}

impl ParticleSystem {
    /// Create a new particle system with default shader paths.
    ///
    /// This provides zero-config initialization - shader paths are built-in constants.
    pub fn new(renderer: &mut VulkanRenderer) -> Result<Self, String> {
        Self::with_shaders(renderer, PARTICLE_SIM_SHADER, PARTICLE_RENDER_SHADER)
    }

    /// Create particle system with custom shaders (for advanced use).
    ///
    /// Most code should use `new()` instead.
    fn with_shaders(
        renderer: &mut VulkanRenderer,
        compute_shader: &str,
        render_shader: &str,
    ) -> Result<Self, String> {
        let resources = std::path::Path::new("resources/shaders");
        let compute_shader_path = resources.join(compute_shader);
        let render_shader_path = resources.join(render_shader);

        let mut system = Self {
            emitters: HashMap::new(),
            compute_pipeline: None,
            render_pipeline: None,
            compute_descriptor_layout_set0: None,
            compute_descriptor_layout_set1: None,
            render_descriptor_layout_set1: None,
            render_descriptor_layout_set0: None,
            frame_uniform_buffer: None,
            descriptor_pool: None,
            next_handle: 0,
        };

        system.init(renderer, compute_shader_path, render_shader_path)?;
        Ok(system)
    }

    /// Create a new particle emitter.
    ///
    /// # Arguments
    /// * `renderer` - Vulkan renderer for resource creation
    /// * `config` - Emitter configuration (position, emit rate, etc.)
    ///
    /// # Returns
    /// Handle to the emitter for use with ECS components
    pub fn create_emitter(
        &mut self,
        renderer: &mut VulkanRenderer,
        config: EmitterConfig,
    ) -> Result<EmitterHandle, String> {
        self.create_emitter_with_capacity(renderer, config, DEFAULT_MAX_PARTICLES)
    }

    /// Create emitter with custom particle capacity.
    ///
    /// Use this for memory-constrained scenarios or high-particle-count effects.
    pub fn create_emitter_with_capacity(
        &mut self,
        renderer: &mut VulkanRenderer,
        config: EmitterConfig,
        max_particles: u32,
    ) -> Result<EmitterHandle, String> {
        let handle = EmitterHandle::new(self.next_handle);
        self.next_handle += 1;

        let context = renderer.context();
        let buffer = ParticleBuffer::new(context.clone(), max_particles);

        let compute_layout_set0 = self
            .compute_descriptor_layout_set0
            .ok_or("Compute Set 0 layout not created")?;
        let render_layout_set1 = self
            .render_descriptor_layout_set1
            .ok_or("Render Set 1 layout not created")?;
        let descriptor_pool = self.descriptor_pool.ok_or("Descriptor pool not created")?;

        let compute_descriptor_set =
            self.allocate_descriptor_set(context, descriptor_pool, compute_layout_set0)?;

        let particle_buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(buffer.buffer())
            .offset(0)
            .range(buffer.size() as u64)];

        let frame_buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(
                self.frame_uniform_buffer
                    .as_ref()
                    .ok_or("Frame uniform buffer not created")?
                    .0,
            )
            .offset(0)
            .range(std::mem::size_of::<FrameData>() as u64)];

        let compute_writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(compute_descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&particle_buffer_info),
            vk::WriteDescriptorSet::default()
                .dst_set(compute_descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(&frame_buffer_info),
        ];

        unsafe {
            context.device.update_descriptor_sets(&compute_writes, &[]);
        }

        let render_descriptor_set =
            self.allocate_descriptor_set(context, descriptor_pool, render_layout_set1)?;

        let render_write = vk::WriteDescriptorSet::default()
            .dst_set(render_descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&particle_buffer_info);

        unsafe {
            context.device.update_descriptor_sets(&[render_write], &[]);
        }

        self.emitters.insert(
            handle.index(),
            ParticleEmitter {
                buffer,
                compute_descriptor_set,
                render_descriptor_set,
                config,
            },
        );

        log::info!(
            "Created particle emitter {:?} ({} particles)",
            handle,
            max_particles
        );
        Ok(handle)
    }

    /// Update emitter configuration.
    ///
    /// Call this to change emit rate, color, position, etc.
    pub fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        if let Some(emitter) = self.emitters.get_mut(&handle.index()) {
            emitter.config = config;
        }
    }

    /// Destroy an emitter and free its GPU resources.
    pub fn destroy_emitter(
        &mut self,
        context: &Rc<crate::vulkan::context::VulkanContext>,
        handle: EmitterHandle,
    ) {
        if let Some(emitter) = self.emitters.remove(&handle.index()) {
            let descriptor_pool = self.descriptor_pool.unwrap();
            unsafe {
                let _ = context
                    .device
                    .free_descriptor_sets(descriptor_pool, &[emitter.compute_descriptor_set]);
                let _ = context
                    .device
                    .free_descriptor_sets(descriptor_pool, &[emitter.render_descriptor_set]);
            }
            log::info!("Destroyed particle emitter {:?}", handle);
        }
    }

    /// Update frame data for simulation (called once per frame by renderer).
    pub fn update_frame_data(&self, frame_data: &FrameData) -> Result<(), String> {
        if let Some((_buffer, allocation)) = &self.frame_uniform_buffer {
            if let Some(mapped_ptr) = allocation.mapped_ptr() {
                let dst = mapped_ptr.as_ptr() as *mut u8;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        frame_data as *const FrameData as *const u8,
                        dst,
                        std::mem::size_of::<FrameData>(),
                    );
                }
            }
        }
        Ok(())
    }

    /// Get compute dispatch data for an emitter (internal, used by renderer).
    pub(crate) fn get_compute_dispatch(
        &self,
        handle: EmitterHandle,
        asset_registry: &crate::renderer::registry::AssetRegistry,
    ) -> Option<(
        PipelineHandle,
        vk::PipelineLayout,
        vk::DescriptorSet,
        EmitterConfig,
        u32,
    )> {
        let emitter = self.emitters.get(&handle.index())?;
        let pipeline = self.compute_pipeline?;
        let pipeline_layout = asset_registry.get_pipeline(pipeline)?.vk_layout();
        let workgroup_count = (emitter.buffer.max_particles() + 255) / 256;

        Some((
            pipeline,
            pipeline_layout,
            emitter.compute_descriptor_set,
            emitter.config,
            workgroup_count,
        ))
    }

    /// Get render data for an emitter (internal, used by renderer).
    pub(crate) fn get_render_data(
        &self,
        handle: EmitterHandle,
        frame_descriptor_set: vk::DescriptorSet,
        asset_registry: &crate::renderer::registry::AssetRegistry,
    ) -> Option<(
        PipelineHandle,
        vk::PipelineLayout,
        vk::DescriptorSet,
        vk::DescriptorSet,
        u32,
    )> {
        let emitter = self.emitters.get(&handle.index())?;
        let pipeline = self.render_pipeline?;
        let pipeline_layout = asset_registry.get_pipeline(pipeline)?.vk_layout();
        let particle_count = emitter.buffer.max_particles();

        Some((
            pipeline,
            pipeline_layout,
            frame_descriptor_set,
            emitter.render_descriptor_set,
            particle_count,
        ))
    }

    /// Initialize the particle system (internal).
    fn init(
        &mut self,
        renderer: &mut VulkanRenderer,
        compute_shader_path: std::path::PathBuf,
        render_shader_path: std::path::PathBuf,
    ) -> Result<(), String> {
        use crate::vulkan::material::compute_pipeline::ComputePipelineBuilder;
        use ash::vk;

        let shader_cache = renderer.material_compiler.shader_cache.clone();

        self.create_descriptor_set_layouts(renderer)?;

        let mut cache = shader_cache.borrow_mut();
        let shader_module = cache
            .load_shader(&compute_shader_path, vk::ShaderStageFlags::COMPUTE)
            .map_err(|e| format!("Failed to load compute shader: {:?}", e))?;
        drop(cache);

        let compute_module = crate::sync::VkShaderModule(shader_module);

        let compute_layout_set0 = self
            .compute_descriptor_layout_set0
            .ok_or("Compute Set 0 layout not created")?;
        let compute_layout_set1 = self
            .compute_descriptor_layout_set1
            .ok_or("Compute Set 1 layout not created")?;

        let compute_pipeline = ComputePipelineBuilder::new(renderer.context().clone())
            .with_shader(compute_module)
            .with_descriptor_layouts(vec![
                crate::sync::VkDescriptorSetLayout(compute_layout_set0),
                crate::sync::VkDescriptorSetLayout(compute_layout_set1),
            ])
            .build()
            .map_err(|e| format!("Failed to build compute pipeline: {}", e))?;

        let compute_pipeline_handle = renderer
            .asset_registry
            .register_compute_pipeline(compute_pipeline);
        self.compute_pipeline = Some(compute_pipeline_handle);

        let mut cache = shader_cache.borrow_mut();
        let render_vert_module = cache
            .load_shader(&render_shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| format!("Failed to load render vertex shader: {:?}", e))?;
        let render_frag_module = cache
            .load_shader(&render_shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| format!("Failed to load render fragment shader: {:?}", e))?;
        drop(cache);

        use crate::vulkan::material::builder::PipelineBuilder;

        let storage_bindings = [
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

        let storage_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&storage_bindings);
        let storage_layout = unsafe {
            renderer
                .context
                .device
                .create_descriptor_set_layout(&storage_layout_info, None)
                .map_err(|e| format!("Failed to create storage descriptor layout: {:?}", e))?
        };

        let render_layout_set1 = self
            .render_descriptor_layout_set1
            .ok_or("Render descriptor layout not created")?;

        let render_pipeline = PipelineBuilder::new(renderer.context().clone())
            .with_shaders(render_vert_module, render_frag_module)
            .with_descriptor_layouts(vec![storage_layout, render_layout_set1])
            .with_alpha_blending()
            .with_depth_test(true, false, crate::pipeline::CompareOp::GreaterOrEqual)
            .with_cull_mode(
                crate::pipeline::CullMode::None,
                crate::pipeline::FrontFace::CounterClockwise,
            )
            .with_rendering_formats(
                Some(crate::texture::ImageFormat::R16G16B16A16Sfloat),
                Some(crate::texture::ImageFormat::D32SfloatS8Uint),
            )
            .build_dynamic()
            .map_err(|e| format!("Failed to build particle render pipeline: {}", e))?;

        self.render_descriptor_layout_set0 = Some(storage_layout);

        let render_pipeline_handle = renderer.asset_registry.register_pipeline(render_pipeline);
        self.render_pipeline = Some(render_pipeline_handle);

        log::info!("Particle render pipeline created successfully");

        let buffer_size = std::mem::size_of::<FrameData>() as u64;
        let context = renderer.context();

        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            context
                .device
                .create_buffer(&buffer_info, None)
                .map_err(|e| format!("Failed to create frame uniform buffer: {:?}", e))?
        };

        let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

        let allocation = context
            .allocator
            .borrow_mut()
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "particle_frame_uniform",
                requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate frame uniform memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|e| format!("Failed to bind frame uniform memory: {:?}", e))?
        };

        self.frame_uniform_buffer = Some((buffer, allocation));

        log::info!("Particle system initialized successfully");
        Ok(())
    }

    fn create_descriptor_set_layouts(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<(), String> {
        let context = renderer.context();

        let compute_bindings_set0 = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];

        let compute_layout_info_set0 =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&compute_bindings_set0);
        let compute_layout_set0 = unsafe {
            context
                .device
                .create_descriptor_set_layout(&compute_layout_info_set0, None)
                .map_err(|e| format!("Failed to create compute Set 0 layout: {:?}", e))?
        };
        self.compute_descriptor_layout_set0 = Some(compute_layout_set0);

        let compute_bindings_set1 = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];

        let compute_layout_info_set1 =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&compute_bindings_set1);
        let compute_layout_set1 = unsafe {
            context
                .device
                .create_descriptor_set_layout(&compute_layout_info_set1, None)
                .map_err(|e| format!("Failed to create compute Set 1 layout: {:?}", e))?
        };
        self.compute_descriptor_layout_set1 = Some(compute_layout_set1);

        let render_bindings_set1 = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];

        let render_layout_info_set1 =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&render_bindings_set1);
        let render_layout_set1 = unsafe {
            context
                .device
                .create_descriptor_set_layout(&render_layout_info_set1, None)
                .map_err(|e| format!("Failed to create render Set 1 layout: {:?}", e))?
        };
        self.render_descriptor_layout_set1 = Some(render_layout_set1);

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 256,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 256,
            },
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(512)
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

        let descriptor_pool = unsafe {
            context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| format!("Failed to create descriptor pool: {:?}", e))?
        };
        self.descriptor_pool = Some(descriptor_pool);

        log::info!("Created particle descriptor set layouts");
        Ok(())
    }

    fn allocate_descriptor_set(
        &self,
        context: &Rc<crate::vulkan::context::VulkanContext>,
        pool: vk::DescriptorPool,
        layout: vk::DescriptorSetLayout,
    ) -> Result<vk::DescriptorSet, String> {
        let layouts = [layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);

        unsafe {
            context
                .device
                .allocate_descriptor_sets(&alloc_info)
                .map(|sets| sets[0])
                .map_err(|e| format!("Failed to allocate descriptor set: {:?}", e))
        }
    }

    /// Destroy all particle system resources.
    pub fn destroy(&mut self, context: &Rc<crate::vulkan::context::VulkanContext>) {
        for handle_index in self.emitters.keys().copied().collect::<Vec<_>>() {
            let handle = EmitterHandle::new(handle_index);
            self.destroy_emitter(context, handle);
        }

        if let Some(pool) = self.descriptor_pool.take() {
            unsafe {
                context.device.destroy_descriptor_pool(pool, None);
            }
            log::debug!("Destroyed particle descriptor pool");
        }

        if let Some(layout) = self.render_descriptor_layout_set1.take() {
            unsafe {
                context.device.destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.render_descriptor_layout_set0.take() {
            unsafe {
                context.device.destroy_descriptor_set_layout(layout, None);
            }
        }

        if let Some((buffer, mut allocation)) = self.frame_uniform_buffer.take() {
            unsafe {
                context.device.destroy_buffer(buffer, None);
                context.allocator.borrow_mut().free(allocation).ok();
            }
            log::debug!("Destroyed particle frame uniform buffer");
        }

        log::info!("Particle system destroyed");
    }
}

impl Drop for ParticleSystem {
    fn drop(&mut self) {}
}
