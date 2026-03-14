//! Modern GPU-driven particle system using single global buffer.
//!
//! This module implements a 2025-vulkan particle system with:
//! - Single global particle pool for all emitters
//! - GPU-driven lifecycle via atomic counters
//! - Index list management for efficient particle tracking
//! - Bindless/BDA resource access
//! - Indirect drawing for optimal GPU utilization
//!
//! # Architecture
//!
//! ## Data Structures
//!
//! - `GlobalParticleSystem`: Main particle system manager
//! - `GlobalParticleBuffer`: Single buffer for all particles + index lists
//! - `ParticleSystemConfig`: Configuration for particle behavior
//!
//! ## Pipeline Flow
//!
//! 1. **Compute Pass**: Emit new particles → Simulate alive particles → Update index lists
//! 2. **Render Pass**: Indirect draw using alive list (only alive particles rendered)
//!
//! # Example
//!
//! ```ignore
//! let mut system = GlobalParticleSystem::new(renderer, MAX_PARTICLES)?;
//! let emitter = system.create_emitter(config)?;
//! system.update(delta_time)?;
//! system.render(render_pass)?;
//! ```

pub mod buffer;

pub use buffer::{FrameData, GlobalParticleBuffer, ParticleCounters};

use std::rc::Rc;

use ash::vk;
use log::{info, warn};

use crate::handle::PipelineHandle;
use crate::renderer::registry::AssetRegistry;
use crate::sync::VkShaderModule;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::compute_pipeline::ComputePipelineBuilder;

/// Default maximum particles across all emitters
pub const DEFAULT_MAX_PARTICLES: u32 = 1_048_576; // 1M particles (48MB)

/// Maximum emitters in system
pub const MAX_EMITTERS: u32 = 1024;

/// Workgroup size for particle compute shader
pub const PARTICLE_WORKGROUP_SIZE: u32 = 256;

/// Per-emitter configuration for GPU.
///
/// This is uploaded to a storage buffer that the compute shader
/// accesses when spawning particles.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EmitterConfig {
    /// World position of emitter
    pub position: [f32; 3],
    pub _pad0: f32,

    /// Particles to emit per second
    pub emit_rate: f32,

    /// Base lifetime for new particles (seconds)
    pub base_lifetime: f32,

    /// Random variation in lifetime (±percentage)
    pub lifetime_variation: f32,

    /// Base velocity direction (normalized)
    pub velocity_direction: [f32; 3],
    pub _pad1: f32,

    /// Velocity magnitude
    pub velocity_magnitude: f32,

    /// Velocity spread cone angle (0 = straight, PI/2 = hemisphere)
    pub velocity_cone_angle: f32,

    /// Base scale for new particles
    pub base_scale: f32,

    /// Scale variation (±percentage)
    pub scale_variation: f32,

    /// Color for new particles (RGBA)
    pub color: [f32; 4],

    /// Color variation (±percentage per channel)
    pub color_variation: f32,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            _pad0: 0.0,
            emit_rate: 50.0,
            base_lifetime: 5.0,
            lifetime_variation: 0.2,
            velocity_direction: [0.0, 1.0, 0.0],
            _pad1: 0.0,
            velocity_magnitude: 1.0,
            velocity_cone_angle: 0.5,
            base_scale: 0.1,
            scale_variation: 0.5,
            color: [1.0, 1.0, 1.0, 1.0],
            color_variation: 0.1,
        }
    }
}

/// Handle to an emitter in the global particle system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EmitterHandle {
    index: u32,
}

impl EmitterHandle {
    /// Invalid emitter handle
    pub const NONE: Self = Self { index: u32::MAX };

    /// Create a new emitter handle from index
    pub fn new(index: u32) -> Self {
        Self { index }
    }

    /// Get the emitter index
    pub fn index(&self) -> u32 {
        self.index
    }
}

/// Modern GPU-driven particle system.
///
/// Manages all particle effects using a single global buffer pool.
/// Each emitter is just configuration data - no per-emitter GPU resources.
pub struct GlobalParticleSystem {
    /// Global particle buffer (particles + index lists + counters)
    buffer: GlobalParticleBuffer,

    /// Compute pipeline for particle simulation
    compute_pipeline: Option<PipelineHandle>,

    /// Compute pipeline layout
    compute_pipeline_layout: Option<vk::PipelineLayout>,

    /// Graphics pipeline for rendering particles
    render_pipeline: Option<PipelineHandle>,

    /// Render pipeline layout
    render_pipeline_layout: Option<vk::PipelineLayout>,

    /// Descriptor set layout for compute (Set 0: static buffers)
    compute_descriptor_layout: Option<vk::DescriptorSetLayout>,

    /// Descriptor set layout for render (Set 0: static buffers)
    render_descriptor_layout: Option<vk::DescriptorSetLayout>,

    /// Per-emitter configurations (CPU-side, uploaded to GPU each frame)
    emitters: Vec<EmitterConfig>,

    /// Next free emitter slot
    next_emitter_slot: u32,

    /// Vulkan context for resource creation
    context: Rc<VulkanContext>,

    /// Frame counter for emission timing
    frame_count: u32,

    /// Frame data buffer for push descriptor updates
    frame_data_buffer: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,

    /// Emitter configs buffer for push descriptor updates
    emitter_configs_buffer: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,
}

impl GlobalParticleSystem {
    /// Create a new global particle system.
    ///
    /// # Arguments
    /// * `renderer` - Vulkan renderer (borrowed for initialization only)
    /// * `max_particles` - Maximum particles across all emitters (default: 1M)
    ///
    /// # Returns
    /// Initialized particle system ready for emitter creation
    pub fn new(context: &Rc<VulkanContext>, max_particles: u32) -> Result<Self, String> {
        info!(
            "Initializing modern particle system (max particles: {})",
            max_particles
        );

        let buffer = GlobalParticleBuffer::new(context.clone(), max_particles)
            .map_err(|e| format!("Failed to create particle buffer: {}", e))?;

        let mut system = Self {
            buffer,
            compute_pipeline: None,
            compute_pipeline_layout: None,
            render_pipeline: None,
            render_pipeline_layout: None,
            compute_descriptor_layout: None,
            render_descriptor_layout: None,
            emitters: Vec::with_capacity(MAX_EMITTERS as usize),
            next_emitter_slot: 0,
            context: context.clone(),
            frame_count: 0,
            frame_data_buffer: None,
            emitter_configs_buffer: None,
        };

        // Initialize index lists (all particles start dead)
        system.buffer.initialize_dead_list()?;

        // Create descriptor set layouts
        system.create_descriptor_layouts(context)?;

        // Create push descriptor buffers
        system.create_push_descriptor_buffers(context)?;

        info!("Modern particle system initialized successfully");
        Ok(system)
    }

    /// Create a new particle emitter.
    ///
    /// This is lightweight - just allocates a config slot.
    /// No GPU resources are created per-emitter.
    ///
    /// # Arguments
    /// * `config` - Emitter configuration (position, emit rate, color, etc.)
    ///
    /// # Returns
    /// Handle to the emitter for use with update/destroy
    pub fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        if self.emitters.len() >= MAX_EMITTERS as usize {
            return Err("Maximum emitter count reached".to_string());
        }

        let index = self.next_emitter_slot;
        self.next_emitter_slot += 1;

        // Ensure vector has space
        if self.emitters.len() <= index as usize {
            self.emitters
                .resize(index as usize + 1, EmitterConfig::default());
        }

        self.emitters[index as usize] = config;

        info!(
            "Created particle emitter {} at position {:?}",
            index, config.position
        );

        Ok(EmitterHandle::new(index))
    }

    /// Update emitter configuration.
    ///
    /// Call this to change emit rate, position, color, etc.
    pub fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        if handle.index() < self.emitters.len() as u32 {
            self.emitters[handle.index() as usize] = config;
        } else {
            warn!("Invalid emitter handle: {:?}", handle);
        }
    }

    /// Destroy an emitter.
    ///
    /// Frees the config slot for reuse.
    pub fn destroy_emitter(&mut self, handle: EmitterHandle) {
        if handle.index() < self.emitters.len() as u32 {
            self.emitters[handle.index() as usize] = EmitterConfig::default();
            info!("Destroyed particle emitter {}", handle.index());
        }
    }

    /// Update particle simulation and emit new particles.
    ///
    /// Call this once per frame before rendering.
    ///
    /// # Arguments
    /// * `delta_time` - Frame time in seconds
    ///
    /// # Returns
    /// Number of active particles this frame
    pub fn update(&mut self, delta_time: f32) -> Result<u32, String> {
        self.frame_count += 1;

        // Upload emitter configs to GPU buffer
        self.upload_emitter_configs()?;

        // Calculate total particles to emit this frame
        let total_emit_count: u32 = self
            .emitters
            .iter()
            .map(|config| (config.emit_rate * delta_time) as u32)
            .sum();

        // Update frame data buffer
        self.update_frame_data(delta_time, total_emit_count)?;

        // Dispatch compute shader
        self.buffer
            .dispatch_compute(total_emit_count, delta_time, self.frame_count)?;

        // Get alive particle count
        let alive_count = self.buffer.get_alive_count()?;

        Ok(alive_count)
    }

    /// Upload emitter configurations to GPU buffer.
    fn upload_emitter_configs(&self) -> Result<(), String> {
        if let Some((_buffer, allocation)) = &self.emitter_configs_buffer {
            if let Some(mapped) = allocation.mapped_ptr() {
                let dst = mapped.as_ptr() as *mut EmitterConfig;
                unsafe {
                    std::ptr::copy_nonoverlapping(self.emitters.as_ptr(), dst, self.emitters.len());
                }
                self.context.flush_mapped_memory(
                    allocation,
                    0,
                    (self.emitters.len() * std::mem::size_of::<EmitterConfig>()) as u64,
                );
            }
        }
        Ok(())
    }

    /// Update frame data for push descriptor.
    fn update_frame_data(&self, delta_time: f32, emit_count: u32) -> Result<(), String> {
        if let Some((_buffer, allocation)) = &self.frame_data_buffer {
            if let Some(mapped) = allocation.mapped_ptr() {
                let frame_data = FrameData {
                    delta_time,
                    total_emit_count: emit_count,
                    random_seed: self.frame_count,
                    _pad: 0,
                };
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        &frame_data as *const FrameData as *const u8,
                        mapped.as_ptr() as *mut u8,
                        std::mem::size_of::<FrameData>(),
                    );
                }
                self.context.flush_mapped_memory(
                    allocation,
                    0,
                    std::mem::size_of::<FrameData>() as u64,
                );
            }
        }
        Ok(())
    }

    /// Render particles.
    ///
    /// Uses indirect drawing - GPU determines draw count from alive list.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record draw calls into
    /// * `render_pass` - Current render pass
    pub fn render(
        &mut self,
        command_buffer: vk::CommandBuffer,
        render_pass: vk::RenderPass,
    ) -> Result<(), String> {
        self.buffer
            .dispatch_draw_indirect(command_buffer, render_pass)
    }

    /// Get current alive particle count.
    pub fn alive_count(&self) -> u32 {
        self.buffer.get_alive_count().unwrap_or(0)
    }

    /// Destroy all particle system resources.
    pub fn destroy(&mut self) {
        info!("Destroying particle system");
        self.buffer.destroy();

        // Destroy descriptor layouts
        if let Some(layout) = self.compute_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.render_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }

        // Destroy push descriptor buffers
        if let Some((buffer, allocation)) = self.frame_data_buffer.take() {
            unsafe {
                self.context.device.destroy_buffer(buffer, None);
                self.context.allocator.borrow_mut().free(allocation).ok();
            }
        }
        if let Some((buffer, allocation)) = self.emitter_configs_buffer.take() {
            unsafe {
                self.context.device.destroy_buffer(buffer, None);
                self.context.allocator.borrow_mut().free(allocation).ok();
            }
        }

        self.emitters.clear();
        self.next_emitter_slot = 0;
    }

    /// Create descriptor set layouts for particle system.
    fn create_descriptor_layouts(&mut self, context: &Rc<VulkanContext>) -> Result<(), String> {
        // Compute layout (Set 0: static buffers)
        let compute_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];

        let compute_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&compute_bindings);

        let compute_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&compute_layout_info, None)
                .map_err(|e| format!("Failed to create compute descriptor layout: {:?}", e))?
        };

        self.compute_descriptor_layout = Some(compute_layout);

        // Render layout (Set 0: particle data + alive list)
        let render_bindings = [
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

        let render_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&render_bindings);

        let render_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&render_layout_info, None)
                .map_err(|e| format!("Failed to create render descriptor layout: {:?}", e))?
        };

        self.render_descriptor_layout = Some(render_layout);

        info!("Created particle system descriptor layouts");
        Ok(())
    }

    /// Create buffers for push descriptor updates.
    fn create_push_descriptor_buffers(
        &mut self,
        context: &Rc<VulkanContext>,
    ) -> Result<(), String> {
        // Frame data buffer (uniform, CPU-visible)
        let frame_data_size = std::mem::size_of::<FrameData>() as u64;
        let frame_buffer_info = vk::BufferCreateInfo::default()
            .size(frame_data_size)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let frame_buffer = unsafe {
            context
                .device
                .create_buffer(&frame_buffer_info, None)
                .map_err(|e| format!("Failed to create frame data buffer: {:?}", e))?
        };

        let frame_requirements =
            unsafe { context.device.get_buffer_memory_requirements(frame_buffer) };

        let frame_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "particle_frame_data",
                requirements: frame_requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate frame data memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    frame_buffer,
                    frame_allocation.memory(),
                    frame_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind frame data memory: {:?}", e))?
        }

        self.frame_data_buffer = Some((frame_buffer, frame_allocation));

        // Emitter configs buffer (storage, CPU-visible)
        let emitter_size = (MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64;
        let emitter_buffer_info = vk::BufferCreateInfo::default()
            .size(emitter_size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let emitter_buffer = unsafe {
            context
                .device
                .create_buffer(&emitter_buffer_info, None)
                .map_err(|e| format!("Failed to create emitter configs buffer: {:?}", e))?
        };

        let emitter_requirements = unsafe {
            context
                .device
                .get_buffer_memory_requirements(emitter_buffer)
        };

        let emitter_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "particle_emitter_configs",
                requirements: emitter_requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate emitter configs memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    emitter_buffer,
                    emitter_allocation.memory(),
                    emitter_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind emitter configs memory: {:?}", e))?
        }

        self.emitter_configs_buffer = Some((emitter_buffer, emitter_allocation));

        info!("Created particle system push descriptor buffers");
        Ok(())
    }

    /// Create compute pipeline for particle simulation.
    pub fn create_compute_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        shader_module: VkShaderModule,
    ) -> Result<(), String> {
        let compute_layout = self
            .compute_descriptor_layout
            .ok_or("Compute descriptor layout not created")?;

        let compute_pipeline = ComputePipelineBuilder::new(self.context.clone())
            .with_shader(shader_module)
            .with_descriptor_layouts(vec![crate::sync::VkDescriptorSetLayout(compute_layout)])
            .build()
            .map_err(|e| format!("Failed to build compute pipeline: {}", e))?;

        let pipeline_handle = asset_registry.register_compute_pipeline(compute_pipeline);
        self.compute_pipeline = Some(pipeline_handle);

        info!("Created particle compute pipeline");
        Ok(())
    }

    /// Record compute dispatch commands.
    pub fn record_compute_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        total_workgroups: u32,
    ) -> Result<(), String> {
        let pipeline = self
            .compute_pipeline
            .ok_or("Compute pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get compute pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        let device = &self.context.device;

        // Bind compute pipeline
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Bind static descriptor set (Set 0)
        // TODO: Create descriptor set for static buffers
        // For now, this is a stub that will be completed when we integrate with the buffer

        // Update push descriptors (Set 1)
        if let Some((frame_buffer, _)) = &self.frame_data_buffer {
            if let Some((emitter_buffer, _)) = &self.emitter_configs_buffer {
                let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                    .buffer(*frame_buffer)
                    .offset(0)
                    .range(std::mem::size_of::<FrameData>() as u64)];

                let emitter_buffer_info = [vk::DescriptorBufferInfo::default()
                    .buffer(*emitter_buffer)
                    .offset(0)
                    .range((self.emitters.len() * std::mem::size_of::<EmitterConfig>()) as u64)];

                let push_descriptor_writes = [
                    vk::WriteDescriptorSet::default()
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(&frame_buffer_info),
                    vk::WriteDescriptorSet::default()
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&emitter_buffer_info),
                ];

                unsafe {
                    // Use push descriptors (no allocation, writes directly to command buffer)
                    let push_descriptor = self
                        .context
                        .push_descriptor_khr
                        .as_ref()
                        .ok_or("Push descriptor extension not available")?;

                    push_descriptor.cmd_push_descriptor_set(
                        command_buffer,
                        vk::PipelineBindPoint::COMPUTE,
                        vk_layout,
                        1, // Set 1
                        &push_descriptor_writes,
                    );
                }
            }
        }

        // Dispatch compute shader
        unsafe {
            device.cmd_dispatch(command_buffer, total_workgroups, 1, 1);
        }

        Ok(())
    }
}

impl Drop for GlobalParticleSystem {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emitter_config_size() {
        // Ensure config fits in uniform buffer and is properly aligned
        assert_eq!(std::mem::size_of::<EmitterConfig>(), 80);
    }

    #[test]
    fn test_emitter_handle() {
        let handle = EmitterHandle::new(42);
        assert_eq!(handle.index(), 42);
        assert_ne!(handle, EmitterHandle::NONE);
    }
}
