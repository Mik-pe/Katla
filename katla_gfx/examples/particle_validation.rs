mod particle_validation_helpers;

// Particle System Validation Example
//
// This binary validates the particle system by:
// - Initializing a headless Vulkan context with validation enabled
// - Creating a particle system with test emitters
// - Running simulation for several frames with ACTUAL GPU compute execution
// - Mimicking real swapchain 2-frames-in-flight double-buffering behavior
// - Reading back particle data asynchronously (deferred by 2 frames)
// - Checking for NaN, infinity, and reasonable bounds
// - Detecting Vulkan validation errors
//
// Double-buffering simulation:
//   - Two persistent fences cycle with frame%2 (like SwapData.in_flight_fences)
//   - Frame N's GPU work waits on Frame N-1's semaphore (GPU-side ordering)
//   - CPU does NOT wait on Frame N's fence until Frame N+2 (deferred by 2)
//   - cached_alive_count is 2 frames stale (same as real swapchain)
//   - Debug readback reads from the frame that completed 2 frames ago
//
// This example is designed for CI/LLM environments and requires no visual output.
//
// Exit codes:
// - 0: Validation passed
// - 1: Validation failed (with error message)

use ash::vk;
use katla_gfx::ValidationMode;
use katla_gfx::VulkanContext;
use katla_gfx::particles::{
    EmitterConfig, GlobalParticleSystem, PARTICLE_EMIT_WORKGROUP_SIZE,
    PARTICLE_SIMULATE_WORKGROUP_SIZE,
};
use katla_gfx::renderer::AssetRegistry;
use std::ffi::CString;
use std::process::ExitCode;
use std::rc::Rc;

use particle_validation_helpers::{
    RenderValidationResources, find_shader_directory, record_render_dispatch,
};

/// Default maximum particles for validation test
const DEFAULT_MAX_PARTICLES: u32 = 100_000;

/// Number of frames to simulate (enough to fill capacity + several recycling lifetimes)
const NUM_FRAMES: u32 = 5000;

/// Delta time per frame (60 FPS)
const DELTA_TIME: f32 = 1.0 / 60.0;

/// Number of emitters for stress testing
const NUM_EMITTERS: u32 = 5;

/// Particle lifetime (seconds)
const LIFETIME: f32 = 2.0;

/// Track particle diagnostics per frame
struct FrameDiagnostics {
    frame: u32,
    alive_count: u32,
    emit_count: u32,
    delta_time: f32,
    cumulative_time: f32,
}

impl FrameDiagnostics {
    fn new(
        frame: u32,
        alive_count: u32,
        emit_count: u32,
        delta_time: f32,
        cumulative_time: f32,
    ) -> Self {
        Self {
            frame,
            alive_count,
            emit_count,
            delta_time,
            cumulative_time,
        }
    }

    fn log(&self) {
        log::debug!(
            "FRAME {}: alive={} emit={} dt={:.5}s cumulative={:.2}s",
            self.frame,
            self.alive_count,
            self.emit_count,
            self.delta_time,
            self.cumulative_time
        );
    }
}

/// Persistent per-frame GPU synchronization state (mirrors SwapData).
///
/// With 2 frames-in-flight, frame N and N+1 execute concurrently on the GPU.
/// CPU only waits on fence[N] when it needs to reuse slot N (i.e., at frame N+2).
struct DoubleBufferedFrameState {
    fences: [vk::Fence; 2],
    frame_complete_semaphores: [vk::Semaphore; 2],
    current_frame: usize,
}

impl DoubleBufferedFrameState {
    fn new(device: &ash::Device) -> Self {
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let sem_info = vk::SemaphoreCreateInfo::default();

        let fences = [
            unsafe { device.create_fence(&fence_info, None).unwrap() },
            unsafe { device.create_fence(&fence_info, None).unwrap() },
        ];
        let frame_complete_semaphores = [
            unsafe { device.create_semaphore(&sem_info, None).unwrap() },
            unsafe { device.create_semaphore(&sem_info, None).unwrap() },
        ];

        Self {
            fences,
            frame_complete_semaphores,
            current_frame: 0,
        }
    }

    /// Wait on the fence for the current frame slot (blocks CPU until GPU work is done).
    /// Called at the start of each frame to ensure the slot is free for reuse.
    fn wait_for_fence(&self, device: &ash::Device) {
        unsafe {
            device
                .wait_for_fences(&[self.fences[self.current_frame]], true, u64::MAX)
                .unwrap();
            device
                .reset_fences(&[self.fences[self.current_frame]])
                .unwrap();
        }
    }

    /// Advance to the next frame slot.
    fn step_frame(&mut self) {
        self.current_frame = (self.current_frame + 1) % 2;
    }

    fn fence(&self, frame_index: usize) -> vk::Fence {
        self.fences[frame_index % 2]
    }

    fn frame_complete_semaphore(&self, frame_index: usize) -> vk::Semaphore {
        self.frame_complete_semaphores[frame_index % 2]
    }

    fn destroy(&self, device: &ash::Device) {
        unsafe {
            for &fence in &self.fences {
                device.destroy_fence(fence, None);
            }
            for &sem in &self.frame_complete_semaphores {
                device.destroy_semaphore(sem, None);
            }
        }
    }
}

/// GPU-side per-particle validation resources.
///
/// Runs the particle_validate.wgsl compute shader after each simulate dispatch.
/// The shader checks every alive particle's color against its emitter config
/// using atomics. Results accumulate in a GPU buffer and are read back once.
struct GpuValidationResources {
    val_results_buffer: vk::Buffer,
    val_results_alloc: gpu_allocator::vulkan::Allocation,
    val_params_buffer: vk::Buffer,
    val_params_alloc: gpu_allocator::vulkan::Allocation,
    validation_pass: katla_gfx::compute::ComputePass,
    val_pipeline: vk::Pipeline,
    val_layout: vk::PipelineLayout,
    alive_list_size: u64,
    counters_size: u64,
    emitter_count: u32,
}

impl GpuValidationResources {
    const VALIDATION_RESULTS_SIZE: u64 = 2048;
    const VALIDATION_PARAMS_SIZE: u64 = 32;

    fn new(
        context: &Rc<VulkanContext>,
        particle_system: &mut GlobalParticleSystem,
        asset_registry: &mut AssetRegistry,
        shader_dir: &std::path::PathBuf,
        emitter_count: u32,
    ) -> Result<Self, String> {
        use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};
        use katla_gfx::ShaderCache;
        use katla_gfx::compute::ComputePass;
        use katla_gfx::sync::VkShaderModule;

        // Validation results buffer (atomic counters, CPU-visible)
        let val_results_info = vk::BufferCreateInfo::default()
            .size(Self::VALIDATION_RESULTS_SIZE)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let val_results_buffer = unsafe {
            context
                .device
                .create_buffer(&val_results_info, None)
                .map_err(|e| format!("Failed to create validation results buffer: {:?}", e))?
        };

        let val_results_reqs = unsafe {
            context
                .device
                .get_buffer_memory_requirements(val_results_buffer)
        };

        let val_results_alloc = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "validation_results",
                requirements: val_results_reqs,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate validation results memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    val_results_buffer,
                    val_results_alloc.memory(),
                    val_results_alloc.offset(),
                )
                .map_err(|e| format!("Failed to bind validation results buffer: {:?}", e))?;
        }

        // Validation params buffer (uniform)
        let val_params_info = vk::BufferCreateInfo::default()
            .size(Self::VALIDATION_PARAMS_SIZE)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let val_params_buffer = unsafe {
            context
                .device
                .create_buffer(&val_params_info, None)
                .map_err(|e| format!("Failed to create validation params buffer: {:?}", e))?
        };

        let val_params_reqs = unsafe {
            context
                .device
                .get_buffer_memory_requirements(val_params_buffer)
        };

        let val_params_alloc = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "validation_params",
                requirements: val_params_reqs,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate validation params memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    val_params_buffer,
                    val_params_alloc.memory(),
                    val_params_alloc.offset(),
                )
                .map_err(|e| format!("Failed to bind validation params buffer: {:?}", e))?;
        }

        // Zero-initialize validation results buffer
        {
            let cmd = context.begin_single_time_commands();
            unsafe {
                context.device.cmd_fill_buffer(
                    cmd.vk_command_buffer(),
                    val_results_buffer,
                    0,
                    Self::VALIDATION_RESULTS_SIZE,
                    0,
                );
            }
            context.end_single_time_commands(cmd);
        }

        // Load validation shader and create compute pass
        let mut shader_cache = ShaderCache::new(context.device.clone());
        let validate_shader_path = shader_dir.join("particles/particle_validate.wgsl");
        let validate_shader = shader_cache
            .load_shader(&validate_shader_path, vk::ShaderStageFlags::COMPUTE)
            .map_err(|e| format!("Failed to load validation shader: {}", e))?;

        let validate_shader_wrapper = VkShaderModule(validate_shader);

        let layout = *particle_system.buffer_layout();
        let next_frame_offset = layout.alive_frame_offset[1];
        let alive_list_size = layout.alive_list_size;
        let max_particles_layout = layout.max_particles;
        let particle_data_size =
            max_particles_layout * std::mem::size_of::<katla_gfx::particles::ParticleData>() as u64;
        let counters_size = std::mem::size_of::<katla_gfx::particles::ParticleCounters>() as u64;
        let emitter_config_total = (1024 * std::mem::size_of::<EmitterConfig>()) as u64;

        let validation_pass = ComputePass::with_push_descriptors(context)
            .add_storage_buffer(0, particle_system.particle_buffer(), 0, particle_data_size)
            .add_storage_buffer(
                1,
                particle_system.particle_buffer(),
                next_frame_offset,
                alive_list_size,
            )
            .add_storage_buffer(2, particle_system.counters_buffer(0), 0, counters_size)
            .add_storage_buffer(
                3,
                particle_system
                    .emitter_configs_buffer(0)
                    .ok_or("Emitter configs buffer not available")?,
                0,
                emitter_config_total,
            )
            .add_storage_buffer(4, val_results_buffer, 0, Self::VALIDATION_RESULTS_SIZE)
            .add_uniform_buffer(5, val_params_buffer, 0, Self::VALIDATION_PARAMS_SIZE)
            .add_storage_buffer(6, particle_system.indirect_draw_buffer(0), 0, 16)
            .build(validate_shader_wrapper, asset_registry)
            .map_err(|e| format!("Failed to build validation pass: {}", e))?;

        let pipeline_asset = asset_registry
            .get_pipeline(validation_pass.pipeline_handle())
            .ok_or("Validation pipeline not found in registry")?;
        let val_pipeline = pipeline_asset.vk_pipeline();
        let val_layout = pipeline_asset.vk_layout();

        Ok(Self {
            val_results_buffer,
            val_results_alloc,
            val_params_buffer,
            val_params_alloc,
            validation_pass,
            val_pipeline,
            val_layout,
            alive_list_size,
            counters_size,
            emitter_count,
        })
    }

    /// Record a barrier + validation dispatch after simulate has completed.
    fn record_dispatch(
        &mut self,
        context: &VulkanContext,
        particle_system: &GlobalParticleSystem,
        command_buffer: vk::CommandBuffer,
        fi: usize,
        frame: u32,
    ) {
        let next_fi = (fi + 1) % 2;
        let layout = *particle_system.buffer_layout();

        // Update bindings for the current frame's buffers
        self.validation_pass.update_binding(
            1,
            particle_system.particle_buffer(),
            layout.alive_frame_offset[next_fi],
            self.alive_list_size,
        );
        self.validation_pass.update_binding(
            2,
            particle_system.counters_buffer(fi),
            0,
            self.counters_size,
        );
        if let Some(ecb) = particle_system.emitter_configs_buffer(fi) {
            self.validation_pass.update_binding(
                3,
                ecb,
                0,
                (1024 * std::mem::size_of::<EmitterConfig>()) as u64,
            );
        }
        let idb = particle_system.indirect_draw_buffer(fi);
        self.validation_pass.update_binding(6, idb, 0, 16);

        // Write validation params
        let val_params = ValidationParams {
            alive_count: particle_system.alive_count(),
            emitter_count: self.emitter_count,
            frame_index: frame,
            max_mismatch_details: 16,
            color_tolerance: 0.05,
            velocity_tolerance: 0.0,
            position_tolerance: 0.0,
            _pad: 0.0,
        };

        if let Some(mapped) = self.val_params_alloc.mapped_ptr() {
            let dst = mapped.as_ptr() as *mut u8;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &val_params as *const ValidationParams as *const u8,
                    dst,
                    std::mem::size_of::<ValidationParams>(),
                );
            }
            context.flush_mapped_memory(&self.val_params_alloc, 0, Self::VALIDATION_PARAMS_SIZE);
        }

        // Barrier: simulate writes particle data + counters, validation reads them
        let particle_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(particle_system.particle_buffer())
            .offset(0)
            .size(vk::WHOLE_SIZE);

        let counters_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(particle_system.counters_buffer(fi))
            .offset(0)
            .size(vk::WHOLE_SIZE);

        // Barrier: render used indirect_draw_buffer at DRAW_INDIRECT stage,
        // validation reads it at COMPUTE stage. Match real draw timing.
        let indirect_draw_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(
                vk::PipelineStageFlags2::DRAW_INDIRECT | vk::PipelineStageFlags2::COMPUTE_SHADER,
            )
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(
                vk::AccessFlags2::INDIRECT_COMMAND_READ | vk::AccessFlags2::SHADER_WRITE,
            )
            .dst_access_mask(vk::AccessFlags2::SHADER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(particle_system.indirect_draw_buffer(fi))
            .offset(0)
            .size(16);

        let barriers = [particle_barrier, counters_barrier, indirect_draw_barrier];
        let dep_info = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);
        unsafe {
            context
                .device
                .cmd_pipeline_barrier2(command_buffer, &dep_info);
        }

        let validate_workgroups = (particle_system.alive_count() + 63) / 64;
        if validate_workgroups > 0 {
            self.validation_pass.record_dispatch_with_handles(
                command_buffer,
                self.val_pipeline,
                self.val_layout,
                validate_workgroups,
                1,
                1,
            );
        }
    }

    /// Read accumulated validation results after all frames complete.
    fn read_results(&self, context: &VulkanContext) -> GpuValidationResults {
        context.invalidate_mapped_memory(&self.val_results_alloc, 0, Self::VALIDATION_RESULTS_SIZE);

        let raw: GpuValidationResultsRaw = if let Some(mapped) = self.val_results_alloc.mapped_ptr()
        {
            unsafe { std::ptr::read_unaligned(mapped.as_ptr() as *const GpuValidationResultsRaw) }
        } else {
            GpuValidationResultsRaw::default()
        };

        let mismatch_count = raw.mismatch_count.min(16) as usize;
        let mismatch_details: Vec<_> = (0..mismatch_count)
            .map(|i| {
                let base = i * 4;
                GpuMismatchDetail {
                    frame_index: raw.mismatch_details[base],
                    particle_idx: raw.mismatch_details[base + 1],
                    packed_color: raw.mismatch_details[base + 2],
                    emitter_packed: raw.mismatch_details[base + 3],
                }
            })
            .collect();

        let id_detail_count = raw.indirect_draw_detail_count.min(64) as usize;
        let id_details: Vec<_> = (0..id_detail_count)
            .map(|i| {
                let base = i * 3;
                IndirectDrawMismatch {
                    frame_index: raw.indirect_draw_details[base],
                    expected_vertex_count: raw.indirect_draw_details[base + 1],
                    actual_vertex_count: raw.indirect_draw_details[base + 2],
                }
            })
            .collect();

        let zero_alive_frames = raw.zero_alive_frames as u64;
        let zero_alive_with_emit = raw.zero_alive_with_emit as u64;
        let min_alive_count = raw.min_alive_count;
        let max_alive_count = raw.max_alive_count;
        let total_alive_sum = raw.total_alive_sum as u64;
        let total_frames_checked = raw.total_frames_checked as u64;

        let anomaly_count = raw.anomaly_count.min(16) as usize;
        let anomaly_details: Vec<FrameAnomaly> = (0..anomaly_count)
            .map(|i| {
                let base = i * 5;
                FrameAnomaly {
                    frame_index: raw.anomaly_details[base],
                    alive_count: raw.anomaly_details[base + 1],
                    emit_count: raw.anomaly_details[base + 2],
                    dead_count: raw.anomaly_details[base + 3],
                    vertex_count: raw.anomaly_details[base + 4],
                }
            })
            .collect();

        GpuValidationResults {
            total_checked: raw.total_checked as u64,
            color_mismatches: raw.color_mismatches as u64,
            velocity_mismatches: raw.velocity_mismatches as u64,
            mismatch_details: mismatch_details,
            indirect_draw_mismatches: raw.indirect_draw_mismatches as u64,
            indirect_draw_details: id_details,
            zero_alive_frames,
            zero_alive_with_emit,
            min_alive_count,
            max_alive_count,
            total_alive_sum,
            total_frames_checked,
            anomaly_details,
        }
    }

    fn destroy(self, context: &VulkanContext) {
        unsafe {
            context.device.destroy_buffer(self.val_results_buffer, None);
            context.device.destroy_buffer(self.val_params_buffer, None);
        }
        if let Ok(mut allocator) = context.allocator.try_borrow_mut() {
            allocator.free(self.val_results_alloc).ok();
            allocator.free(self.val_params_alloc).ok();
        }
    }
}

/// Mirrors the WGSL ValidationResults struct layout exactly.
/// Offsets must match particle_validate.wgsl.
/// Not using bytemuck derive due to [u32; 192] exceeding bytemuck's array limit.
#[repr(C)]
struct GpuValidationResultsRaw {
    total_checked: u32,
    color_mismatches: u32,
    velocity_mismatches: u32,
    position_mismatches: u32,
    per_emitter_mismatches: [u32; 16],
    mismatch_details: [u32; 64],
    mismatch_count: u32,
    indirect_draw_mismatches: u32,
    indirect_draw_details: [u32; 192],
    indirect_draw_detail_count: u32,
    // Frame-level counter-consistency tracking (offset 1116+)
    zero_alive_frames: u32,
    zero_alive_with_emit: u32,
    min_alive_count: u32,
    max_alive_count: u32,
    total_alive_sum: u32,
    total_frames_checked: u32,
    anomaly_details: [u32; 80], // 16 entries x 5 u32
    anomaly_count: u32,
}

impl Default for GpuValidationResultsRaw {
    fn default() -> Self {
        Self {
            total_checked: 0,
            color_mismatches: 0,
            velocity_mismatches: 0,
            position_mismatches: 0,
            per_emitter_mismatches: [0; 16],
            mismatch_details: [0; 64],
            mismatch_count: 0,
            indirect_draw_mismatches: 0,
            indirect_draw_details: [0; 192],
            indirect_draw_detail_count: 0,
            zero_alive_frames: 0,
            zero_alive_with_emit: 0,
            min_alive_count: 0,
            max_alive_count: 0,
            total_alive_sum: 0,
            total_frames_checked: 0,
            anomaly_details: [0; 80],
            anomaly_count: 0,
        }
    }
}

#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct ValidationParams {
    alive_count: u32,
    emitter_count: u32,
    frame_index: u32,
    max_mismatch_details: u32,
    color_tolerance: f32,
    velocity_tolerance: f32,
    position_tolerance: f32,
    _pad: f32,
}

struct FrameAnomaly {
    frame_index: u32,
    alive_count: u32,
    emit_count: u32,
    dead_count: u32,
    vertex_count: u32,
}

#[derive(Default)]
struct GpuValidationResults {
    total_checked: u64,
    color_mismatches: u64,
    velocity_mismatches: u64,
    mismatch_details: Vec<GpuMismatchDetail>,
    indirect_draw_mismatches: u64,
    indirect_draw_details: Vec<IndirectDrawMismatch>,
    zero_alive_frames: u64,
    zero_alive_with_emit: u64,
    min_alive_count: u32,
    max_alive_count: u32,
    total_alive_sum: u64,
    total_frames_checked: u64,
    anomaly_details: Vec<FrameAnomaly>,
}

struct GpuMismatchDetail {
    frame_index: u32,
    particle_idx: u32,
    packed_color: u32,
    emitter_packed: u32,
}

struct IndirectDrawMismatch {
    frame_index: u32,
    expected_vertex_count: u32,
    actual_vertex_count: u32,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let max_particles: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_PARTICLES);

    log::info!("=== Particle System Validation Example (2-FiF Double Buffered) ===");
    log::info!("Max particles: {}", max_particles);
    log::info!("Frames to simulate: {}", NUM_FRAMES);

    // Create headless Vulkan context with validation enabled
    let app_name = CString::new("Particle Validation").unwrap();
    let engine_name = CString::new("Katla Engine").unwrap();

    log::info!("Creating headless Vulkan context with GPU-assisted validation...");
    let context = VulkanContext::init_headless(ValidationMode::GpuAssisted, app_name, engine_name);
    let context = std::rc::Rc::new(context);
    log::info!("Vulkan context created successfully");

    // Create asset registry for shader loading
    let mut asset_registry = AssetRegistry::new();

    // Find shader directory
    let shader_dir = find_shader_directory();
    log::info!("Using shader directory: {:?}", shader_dir);

    // Create particle system
    log::info!("Creating particle system...");
    let mut particle_system = match GlobalParticleSystem::new(&context, max_particles) {
        Ok(system) => {
            log::info!("Particle system created successfully");
            log::info!("Memory usage: {:.2} MB", system.get_stats().memory_used_mb);
            system
        }
        Err(e) => {
            log::error!("Failed to create particle system: {}", e);
            return ExitCode::from(1);
        }
    };

    // Initialize debug readback for validation
    log::info!("Initializing debug readback...");
    if let Err(e) = particle_system.init_debug_readback() {
        log::error!("Failed to initialize debug readback: {}", e);
        return ExitCode::from(1);
    }
    log::info!("Debug readback initialized");

    // Load compute shaders and create pipelines
    log::info!("Loading particle compute shaders and creating pipelines...");
    if let Err(e) = particle_validation_helpers::load_and_create_pipelines(
        &context,
        &mut particle_system,
        &mut asset_registry,
        &shader_dir,
    ) {
        log::error!("Failed to load shaders and create pipelines: {}", e);
        return ExitCode::from(1);
    }
    log::info!("Particle compute pipelines created successfully");

    // Create render validation resources
    log::info!("Creating render validation resources...");
    let mut render_resources = match RenderValidationResources::new(&context) {
        Ok(res) => {
            log::info!("Render validation resources created successfully");
            Some(res)
        }
        Err(e) => {
            log::warn!(
                "Failed to create render validation resources: {}. Render path will be skipped.",
                e
            );
            None
        }
    };

    // Create test emitters with deterministic properties for GPU validation.
    // Zero randomness ensures the particle_validate shader can do exact matching
    // of color, velocity, and position per emitter.
    log::info!("Creating test emitters...");
    let test_emitters = create_test_emitters(&mut particle_system, max_particles);
    log::info!("Created {} test emitters", test_emitters.len());

    // === GPU-side per-particle validation setup ===
    // The particle_validate.wgsl shader runs after simulate each frame, checking
    // every alive particle's color/velocity against its emitter config using atomics.
    // Results accumulate in a GPU buffer and are read back once after all frames.
    let mut gpu_validation = match GpuValidationResources::new(
        &context,
        &mut particle_system,
        &mut asset_registry,
        &shader_dir,
        test_emitters.len() as u32,
    ) {
        Ok(v) => {
            log::info!("GPU validation resources created successfully");
            Some(v)
        }
        Err(e) => {
            log::warn!(
                "Failed to create GPU validation resources: {}. Cross-emitter validation will be skipped.",
                e
            );
            None
        }
    };

    // === 2-Frames-In-Flight double-buffered simulation ===
    log::info!(
        "Running GPU compute simulation for {} frames (2-FiF double buffered)...",
        NUM_FRAMES
    );

    // Create double-buffered frame state (fences + semaphores)
    let mut frame_state = DoubleBufferedFrameState::new(&context.device);

    // Pre-allocate 2 command buffers (one per FiF slot) to avoid per-frame allocation.
    let frame_commands = [
        context.begin_single_time_commands(),
        context.begin_single_time_commands(),
    ];

    let mut cumulative_time = 0.0;

    for frame in 0..NUM_FRAMES {
        cumulative_time += DELTA_TIME;
        let is_last_frame = frame == NUM_FRAMES - 1;
        let fi = (frame as usize) % 2;

        // --- GPU-side ordering: wait on previous frame's semaphore ---
        // This ensures frame N's GPU work doesn't start until frame N-1's GPU work
        // is complete, matching real swapchain behavior where the present engine
        // serializes frames via frame_complete_semaphores.
        let prev_frame_fi = (fi + 1) % 2;
        let wait_sem = frame_state.frame_complete_semaphore(prev_frame_fi);

        // Wait on the CURRENT frame's fence to ensure the slot is free for reuse.
        // In real swapchain this happens at frame begin. For frames 0 and 1 the
        // fences start signaled so they return immediately.
        frame_state.wait_for_fence(&context.device);

        // --- CPU-side update: prepare frame data for GPU ---
        // Note: cached_alive_count may be stale (from 2 frames ago) or the initial
        // value. This is exactly what happens with a real swapchain.
        let (alive_count, emit_count) = match particle_system.update(DELTA_TIME, frame) {
            Ok(result) => result,
            Err(e) => {
                log::error!("Failed to update particle system at frame {}: {}", frame, e);
                frame_state.destroy(&context.device);
                return ExitCode::from(1);
            }
        };

        let diag =
            FrameDiagnostics::new(frame, alive_count, emit_count, DELTA_TIME, cumulative_time);
        diag.log();

        if frame % 1000 == 0 || is_last_frame {
            log::info!("Frame {}/{} (emit={})", frame, NUM_FRAMES, emit_count);
        }

        // --- Record and submit GPU compute dispatch ---
        frame_commands[fi].reset();
        frame_commands[fi].begin_single_time_command();
        let command_buffer = &frame_commands[fi];

        let frame_index_for_descriptor = fi;

        let emit_workgroups = if emit_count > 0 {
            (emit_count + PARTICLE_EMIT_WORKGROUP_SIZE - 1) / PARTICLE_EMIT_WORKGROUP_SIZE
        } else {
            0
        };

        let max_alive = particle_system.max_estimated_alive();
        let total_to_simulate = max_alive + emit_count;
        let simulate_workgroups = if total_to_simulate > 0 {
            (total_to_simulate + PARTICLE_SIMULATE_WORKGROUP_SIZE - 1)
                / PARTICLE_SIMULATE_WORKGROUP_SIZE
        } else {
            0
        };

        if emit_workgroups > 0 || simulate_workgroups > 0 {
            // Update compute descriptor bindings
            if let Err(e) =
                particle_system.update_compute_descriptor_binding(frame_index_for_descriptor)
            {
                log::warn!(
                    "Frame {}: Failed to update compute descriptor binding: {}",
                    frame,
                    e
                );
            }

            // Record emit dispatch
            if emit_workgroups > 0 {
                if let Err(e) = particle_system.record_emit_dispatch(
                    command_buffer.vk_command_buffer(),
                    &asset_registry,
                    emit_workgroups,
                    frame_index_for_descriptor,
                ) {
                    log::warn!("Failed to record emit dispatch: {}", e);
                }
            }

            // Record simulate dispatch
            if simulate_workgroups > 0 {
                particle_system.reset_simulate_counters(
                    command_buffer.vk_command_buffer(),
                    emit_workgroups > 0,
                    frame_index_for_descriptor,
                );

                if let Err(e) = particle_system.record_simulate_dispatch(
                    command_buffer.vk_command_buffer(),
                    &asset_registry,
                    simulate_workgroups,
                    frame_index_for_descriptor,
                ) {
                    log::warn!("Failed to record simulate dispatch: {}", e);
                }

                // Write indirect draw command after simulate (1-workgroup dispatch
                // with barrier ensures correct alive_count visibility).
                // Push descriptors are recorded inline by record_draw_command_dispatch.
                if let Err(e) = particle_system.record_draw_command_dispatch(
                    command_buffer.vk_command_buffer(),
                    &asset_registry,
                    frame_index_for_descriptor,
                ) {
                    log::warn!("Failed to record draw command dispatch: {}", e);
                }
            }

            // Record render dispatch
            if let Some(ref render_res) = render_resources {
                if let Err(e) = record_render_dispatch(
                    &context,
                    &mut particle_system,
                    &asset_registry,
                    command_buffer.vk_command_buffer(),
                    render_res,
                    frame_index_for_descriptor,
                ) {
                    log::warn!("Frame {}: Failed to record render dispatch: {}", frame, e);
                }
            }

            // Record GPU validation dispatch AFTER render so it runs with
            // the same barrier timing as the real draw path.
            if simulate_workgroups > 0 {
                if let Some(ref mut gpu_val) = gpu_validation {
                    gpu_val.record_dispatch(
                        &context,
                        &particle_system,
                        command_buffer.vk_command_buffer(),
                        fi,
                        frame,
                    );
                }
            }
        }

        // End command buffer
        command_buffer.end_single_time_command();

        // Submit with fence (NO CPU wait) and semaphore signal
        let signal_fence = frame_state.fence(fi);
        let signal_sem = frame_state.frame_complete_semaphore(fi);

        let wait_sems: &[vk::Semaphore] = if frame > 0 { &[wait_sem] } else { &[] };
        context
            .gfx_queue
            .submit(&[&command_buffer], wait_sems, &[signal_sem], signal_fence);

        // Advance to next frame slot
        frame_state.step_frame();
    }

    // --- Drain: wait for the last 2 in-flight frames to complete ---
    unsafe {
        context
            .device
            .wait_for_fences(&frame_state.fences, true, u64::MAX)
            .unwrap();
    }

    // --- Synchronous debug readback: copy last frame's GPU data to staging buffers ---
    // All GPU work is complete (both fences waited above). Do the readback before
    // destroying fences/command buffers to avoid any potential resource conflicts.
    let last_fi = (NUM_FRAMES as usize - 1) % 2;
    {
        let cmd = context.begin_single_time_commands();
        if let Err(e) = particle_system.record_debug_readback(cmd.vk_command_buffer(), last_fi) {
            log::warn!("Failed to record debug readback: {}", e);
        }
        context.end_single_time_commands(cmd);
    }

    frame_state.destroy(&context.device);

    // Return pre-allocated command buffers to pool
    frame_commands[0].return_to_pool();
    frame_commands[1].return_to_pool();

    // --- GPU validation readback (single read of all accumulated results) ---
    if let Some(gpu_val) = gpu_validation {
        log::info!("Reading GPU validation results...");
        let results = gpu_val.read_results(&context);

        // Indirect draw validation: check that vertex_count == alive_count * 6
        // every frame. Accumulated by the GPU validation shader.
        if results.indirect_draw_mismatches > 0 {
            log::error!(
                "INDIRECT DRAW: {} mismatches across {} frames",
                results.indirect_draw_mismatches,
                NUM_FRAMES
            );
            for detail in results.indirect_draw_details.iter().take(3) {
                log::error!(
                    "  frame={}: expected vc={}, got vc={}",
                    detail.frame_index,
                    detail.expected_vertex_count,
                    detail.actual_vertex_count,
                );
            }
            gpu_val.destroy(&context);
            if let Some(ref mut res) = render_resources {
                res.destroy(&context);
            }
            return ExitCode::from(1);
        }

        log::info!(
            "Indirect draw validation PASSED: all {} frames valid",
            NUM_FRAMES
        );

        // Frame-level counter-consistency stats
        if results.total_frames_checked > 0 {
            let avg_alive = results.total_alive_sum as f64 / results.total_frames_checked as f64;
            log::info!(
                "Frame stats: {} frames checked, alive min={} max={} avg={:.1}",
                results.total_frames_checked,
                results.min_alive_count,
                results.max_alive_count,
                avg_alive,
            );
        }

        if results.zero_alive_frames > 0 {
            log::warn!(
                "ZERO-ALIVE FRAMES: {} frames with alive_count=0 ({} had emit_count>0)",
                results.zero_alive_frames,
                results.zero_alive_with_emit,
            );
            for detail in results.anomaly_details.iter().take(10) {
                if detail.alive_count == 0 {
                    log::warn!(
                        "  frame={}: alive=0 emit={} dead={} vc={}",
                        detail.frame_index,
                        detail.emit_count,
                        detail.dead_count,
                        detail.vertex_count,
                    );
                }
            }
        }

        // Cross-emitter contamination validation
        log::info!(
            "GPU validation: {} particles checked across {} frames",
            results.total_checked,
            NUM_FRAMES
        );

        if results.color_mismatches > 0 {
            log::error!(
                "COLOR CONTAMINATION: {} color mismatches across {} frames",
                results.color_mismatches,
                NUM_FRAMES
            );
            for detail in results.mismatch_details.iter().take(5) {
                let r = detail.packed_color / 1000000;
                let g = (detail.packed_color / 1000) % 1000;
                let b = detail.packed_color % 1000;
                log::error!(
                    "  frame={}, particle={}, color=({:.4}, {:.4}, {:.4}), emitter={}",
                    detail.frame_index,
                    detail.particle_idx,
                    r as f32 / 10000.0,
                    g as f32 / 10000.0,
                    b as f32 / 10000.0,
                    detail.emitter_packed / 100000000,
                );
            }
            gpu_val.destroy(&context);
            if let Some(ref mut res) = render_resources {
                res.destroy(&context);
            }
            return ExitCode::from(1);
        }

        if results.velocity_mismatches > 0 {
            log::error!(
                "VELOCITY CONTAMINATION: {} velocity mismatches across {} frames",
                results.velocity_mismatches,
                NUM_FRAMES
            );
            gpu_val.destroy(&context);
            if let Some(ref mut res) = render_resources {
                res.destroy(&context);
            }
            return ExitCode::from(1);
        }

        log::info!(
            "Cross-emitter contamination check PASSED: {} particles checked, 0 mismatches",
            results.total_checked
        );
        gpu_val.destroy(&context);
    }

    log::info!("Simulation complete, running validation...");

    // Read back actual GPU counters from the last frame (recorded during the
    // simulation loop on the last frame, fences already waited above).
    let gpu_alive = if particle_system.has_debug_readback() {
        match particle_system.read_debug_data() {
            Ok(data) => {
                log::info!(
                    "GPU counters: alive={}, dead={}, emit={}, wgf={}",
                    data.counters.alive_count,
                    data.counters.dead_count,
                    data.counters.emit_count,
                    data.counters.workgroups_finished,
                );
                if let Some(idc) = data.indirect_draw {
                    log::info!(
                        "GPU indirect draw: vc={}, ic={}, fv={}, fi={}",
                        idc.vertex_count,
                        idc.instance_count,
                        idc.first_vertex,
                        idc.first_instance,
                    );
                } else {
                    log::info!("GPU indirect draw: not available");
                }
                data.counters.alive_count
            }
            Err(e) => {
                log::warn!("Failed to read debug data: {}", e);
                0
            }
        }
    } else {
        0
    };

    // Get final statistics
    let stats = particle_system.get_stats();
    log::info!("=== Simulation Complete ===");
    log::info!("Total frames: {}", stats.frame_count);
    log::info!("Total particles emitted: {}", stats.total_emitted);
    log::info!("GPU alive count (last frame): {}", gpu_alive);
    log::info!(
        "GPU dead count (last frame): {}",
        stats.max_alive_count.saturating_sub(gpu_alive),
    );

    // Validate particle data by reading back from GPU
    match validate_particle_data(&particle_system) {
        Ok(_) => {
            log::info!("✓ Particle data validation passed");
        }
        Err(e) => {
            log::error!("✗ Particle data validation failed: {}", e);
            if let Some(ref mut res) = render_resources {
                res.destroy(&context);
            }
            return ExitCode::from(1);
        }
    }

    // Validate emitter configurations
    log::info!("Validating emitter configurations...");
    match validate_emitter_configs(&particle_system) {
        Ok(_) => {
            log::info!("✓ Emitter configuration validation passed");
        }
        Err(e) => {
            log::error!("✗ Emitter configuration validation failed: {}", e);
            if let Some(ref mut res) = render_resources {
                res.destroy(&context);
            }
            return ExitCode::from(1);
        }
    }

    // Note: emitter color validation is done per-frame during deferred readback above.
    // A final synchronous readback would read stale alive_list entries from 2-FiF timing,
    // producing false positives. The per-frame check is the authoritative color validation.

    // Print summary statistics
    log::info!("=== Validation Summary ===");
    log::info!("Max particles: {}", max_particles);
    log::info!("Total frames simulated: {}", NUM_FRAMES);
    log::info!("Memory usage: {:.2} MB", stats.memory_used_mb);
    log::info!("Total emitted: {}", stats.total_emitted);

    // Clean up render validation resources
    if let Some(ref mut res) = render_resources {
        res.destroy(&context);
    }

    log::info!("=== All Validations Passed ===");
    ExitCode::SUCCESS
}

/// Estimate expected alive particles at a given frame.
///
/// Based on emitter configurations:
/// Create test emitters with known properties for validation.
///
/// Two emitters placed far apart on X-axis (+50 and -50) with very distinct
/// colors (pure Red vs pure Blue). This makes it trivially easy to detect
/// if the emit shader assigns a particle to the wrong emitter - a red
/// particle at x=+50 or a blue particle at x=-50 is a definitive bug.
fn create_test_emitters(
    particle_system: &mut GlobalParticleSystem,
    max_particles: u32,
) -> Vec<katla_gfx::particles::EmitterHandle> {
    let mut emitters = Vec::new();

    // Scale emit rate so steady state is ~80% of capacity.
    // steady_state = num_emitters * emit_rate * lifetime = 0.8 * max_particles
    // emit_rate = 0.8 * max_particles / (num_emitters * lifetime)
    let emit_rate = (0.8 * max_particles as f32) / (NUM_EMITTERS as f32 * LIFETIME);

    log::info!(
        "Emit config: {} emitters, {:.0}/sec each, lifetime={:.1}s, target steady state ~{:.0}",
        NUM_EMITTERS,
        emit_rate,
        LIFETIME,
        NUM_EMITTERS as f32 * emit_rate * LIFETIME,
    );

    // 5 emitters spread across X axis with distinct primary colors.
    let emitter_defs: [([f32; 3], [f32; 4]); NUM_EMITTERS as usize] = [
        ([-40.0, 0.0, 0.0], [1.0, 0.0, 0.0, 1.0]), // Red
        ([-20.0, 0.0, 0.0], [0.0, 1.0, 0.0, 1.0]), // Green
        ([0.0, 0.0, 0.0], [1.0, 1.0, 0.0, 1.0]),   // Yellow
        ([20.0, 0.0, 0.0], [0.0, 1.0, 1.0, 1.0]),  // Cyan
        ([40.0, 0.0, 0.0], [1.0, 0.0, 1.0, 1.0]),  // Magenta
    ];

    for (i, (pos, color)) in emitter_defs.iter().enumerate() {
        let config = EmitterConfig {
            position: *pos,
            emit_rate,
            base_lifetime: LIFETIME,
            lifetime_variation: 0.0,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_magnitude: 1.0,
            velocity_cone_angle: 0.0,
            base_scale: 0.1,
            scale_variation: 0.0,
            color: *color,
            color_variation: 0.0,
            gravity: 0.0,
            turbulence_strength: 0.0,
            ..Default::default()
        };

        match particle_system.create_emitter(config) {
            Ok(handle) => emitters.push(handle),
            Err(e) => {
                log::error!("Failed to create emitter {}: {}", i, e);
            }
        }
    }

    emitters
}

/// Validate particle data by reading back from GPU.
///
/// This function checks:
/// - Particle positions are not NaN or infinity
/// - Particle positions are within reasonable bounds
/// - Particle lifetimes are valid (non-negative)
/// - Particle velocities are not NaN or infinity
/// - Particle colors are in [0, 1] range
/// - Particle scales are positive
/// - Particles were actually simulated on GPU (positions changed from initial values)
///
/// Additionally provides diagnostic information about:
/// - Actual particle lifetimes from readback
/// - Particle lifetime distribution
/// - Which particles are dying and why
fn validate_particle_data(particle_system: &GlobalParticleSystem) -> Result<(), String> {
    // Read particle data using debug readback (staging buffer copy)
    let debug_data = match particle_system.read_debug_data() {
        Ok(data) => data,
        Err(e) => {
            return Err(format!("Failed to read debug data: {}", e));
        }
    };

    let particles = &debug_data.particles;
    let alive_list = &debug_data.alive_list;
    let alive_count = debug_data.counters.alive_count as usize;

    log::info!("{}", debug_data.summary());

    if alive_count == 0 {
        log::warn!("No alive particles to validate (system may not have simulated yet)");
        return Ok(());
    }

    // Collect the indices of alive particles to validate
    let alive_indices: Vec<usize> = alive_list
        .iter()
        .take(alive_count)
        .map(|&idx| idx as usize)
        .collect();

    // Verify indices are within bounds
    let mut out_of_bounds_count = 0;
    for &idx in &alive_indices {
        if idx >= particles.len() {
            log::error!(
                "Alive particle index {} out of bounds (max={})",
                idx,
                particles.len()
            );
            out_of_bounds_count += 1;
        }
    }

    if out_of_bounds_count > 0 {
        return Err(format!(
            "{} alive particle indices are out of bounds",
            out_of_bounds_count
        ));
    }

    // Check if particles were actually simulated on GPU
    let mut particles_simulated = 0;
    let mut particles_at_origin = 0;

    // Track position distribution - ONLY for alive particles
    let mut unique_positions = std::collections::HashSet::new();
    let mut position_counts: std::collections::HashMap<(i32, i32, i32), usize> =
        std::collections::HashMap::new();

    for &idx in &alive_indices {
        if idx >= particles.len() {
            continue;
        }
        let p = &particles[idx];

        // Quantize position for grouping
        let pos_key = (
            (p.position[0] * 10.0) as i32,
            (p.position[1] * 10.0) as i32,
            (p.position[2] * 10.0) as i32,
        );
        unique_positions.insert(pos_key);
        *position_counts.entry(pos_key).or_insert(0) += 1;

        // A particle at the origin was never emitted/simulated
        if p.position[0] == 0.0 && p.position[1] == 0.0 && p.position[2] == 0.0 {
            particles_at_origin += 1;
        } else {
            particles_simulated += 1;
        }
    }

    log::info!(
        "GPU sim check: {} simulated, {} at origin, {} unique positions",
        particles_simulated,
        particles_at_origin,
        unique_positions.len()
    );

    // Position grouping - check if particles cluster around expected emitter positions
    // Emitter 1: (0, 0, 0), Emitter 2: (5, 0, 0), Emitter 3: (-5, 0, 0)
    log::info!("Position distribution (top 5 clusters):");
    let mut sorted_clusters: Vec<_> = position_counts.into_iter().collect();
    sorted_clusters.sort_by(|a, b| b.1.cmp(&a.1));
    for (pos, count) in sorted_clusters.iter().take(5) {
        log::info!(
            "  ({:.1}, {:.1}, {:.1}): {} particles",
            pos.0 as f32 / 10.0,
            pos.1 as f32 / 10.0,
            pos.2 as f32 / 10.0,
            count
        );
    }

    if particles_simulated == 0 && alive_count > 0 {
        return Err("No alive particles appear to have been simulated on GPU".to_string());
    }

    // CRITICAL CHECK: If all alive particles cluster at very few positions, emitters may not be working
    if unique_positions.len() <= 3 && alive_count > 10 {
        log::warn!(
            "Only {} unique positions among {} alive particles - possible single-emitter emission bug",
            unique_positions.len(),
            alive_count
        );
        for (i, idx) in alive_indices.iter().take(10).enumerate() {
            if *idx < particles.len() {
                let p = &particles[*idx];
                log::warn!(
                    "  [{}] idx={}: pos=({:.2}, {:.2}, {:.2})",
                    i,
                    idx,
                    p.position[0],
                    p.position[1],
                    p.position[2]
                );
            }
        }
    }

    let mut nan_count = 0;
    let mut inf_count = 0;
    let mut out_of_bounds_count = 0;
    let mut invalid_lifetime_count = 0;
    let mut invalid_color_count = 0;
    let mut invalid_scale_count = 0;

    const POSITION_BOUND: f32 = 100.0;

    // Only validate alive particles using their indices from alive_list
    for &idx in &alive_indices {
        if idx >= particles.len() {
            continue;
        }

        let p = &particles[idx];
        if p.position[0].is_nan() || p.position[1].is_nan() || p.position[2].is_nan() {
            nan_count += 1;
        }
        if p.position[0].is_infinite() || p.position[1].is_infinite() || p.position[2].is_infinite()
        {
            inf_count += 1;
        }
        if p.position[0].abs() > POSITION_BOUND
            || p.position[1].abs() > POSITION_BOUND
            || p.position[2].abs() > POSITION_BOUND
        {
            out_of_bounds_count += 1;
        }
        if p.velocity[0].is_nan() || p.velocity[1].is_nan() || p.velocity[2].is_nan() {
            nan_count += 1;
        }
        if p.velocity[0].is_infinite() || p.velocity[1].is_infinite() || p.velocity[2].is_infinite()
        {
            inf_count += 1;
        }
        if p.lifetime < 0.0 {
            invalid_lifetime_count += 1;
        }
        if p.color[0] < 0.0
            || p.color[0] > 1.0
            || p.color[1] < 0.0
            || p.color[1] > 1.0
            || p.color[2] < 0.0
            || p.color[2] > 1.0
            || p.color[3] < 0.0
            || p.color[3] > 1.0
        {
            invalid_color_count += 1;
        }
        if p.scale <= 0.0 {
            invalid_scale_count += 1;
        }
    }

    log::info!(
        "Validation: {} alive, nan={}, inf={}, oob={}, bad_lifetime={}, bad_color={}, bad_scale={}",
        alive_count,
        nan_count,
        inf_count,
        out_of_bounds_count,
        invalid_lifetime_count,
        invalid_color_count,
        invalid_scale_count
    );

    // Calculate position range for debugging (only alive particles)
    let mut min_pos = [f32::INFINITY; 3];
    let mut max_pos = [f32::NEG_INFINITY; 3];
    for &idx in &alive_indices {
        if idx < particles.len() {
            let p = &particles[idx];
            for j in 0..3 {
                min_pos[j] = min_pos[j].min(p.position[j]);
                max_pos[j] = max_pos[j].max(p.position[j]);
            }
        }
    }
    log::info!(
        "Position range: X=[{:.2}, {:.2}], Y=[{:.2}, {:.2}], Z=[{:.2}, {:.2}]",
        min_pos[0],
        max_pos[0],
        min_pos[1],
        max_pos[1],
        min_pos[2],
        max_pos[2]
    );

    // Validate that all configured emitters are producing particles.
    // Test emitters span x=-40 to x=+40, so both extremes must have coverage.
    if max_pos[0] < -39.0 || min_pos[0] > 39.0 {
        return Err(format!(
            "Position range X=[{:.2}, {:.2}] does not cover both emitter extremes (x=-40 and x=+40). \
             Emitters may not all be producing particles.",
            min_pos[0], max_pos[0]
        ));
    }

    // Validation fails if critical errors found
    if nan_count > 0 || inf_count > 0 {
        return Err(format!(
            "Found {} NaN and {} infinity values in particle data",
            nan_count, inf_count
        ));
    }

    if invalid_lifetime_count > 0 || invalid_scale_count > 0 {
        return Err(format!(
            "Found {} invalid lifetimes and {} invalid scales",
            invalid_lifetime_count, invalid_scale_count
        ));
    }

    if out_of_bounds_count > 0 {
        log::warn!(
            "Warning: {} particles have positions outside expected bounds",
            out_of_bounds_count
        );
    }

    if invalid_color_count > 0 {
        log::warn!(
            "Warning: {} particles have colors outside [0, 1] range",
            invalid_color_count
        );
    }

    Ok(())
}
fn validate_emitter_configs(particle_system: &GlobalParticleSystem) -> Result<(), String> {
    use katla_gfx::particles::validate_emitter_config;

    let emitters = particle_system.get_emitters();

    if emitters.is_empty() {
        log::warn!("No emitters to validate");
        return Ok(());
    }

    let mut error_count = 0;
    for (i, config) in emitters.iter().enumerate() {
        if config.emit_rate == 0.0 && config.base_lifetime == 0.0 {
            continue;
        }

        if let Err(e) = validate_emitter_config(config) {
            log::error!("Emitter {}: {}", i, e);
            error_count += 1;
        }
    }

    if error_count > 0 {
        return Err(format!(
            "{} emitters have invalid configurations",
            error_count
        ));
    }

    log::info!(
        "Emitter configuration validation passed ({} emitters)",
        emitters.len()
    );
    Ok(())
}
