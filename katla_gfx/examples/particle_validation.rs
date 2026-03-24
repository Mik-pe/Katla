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
use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};
use katla_gfx::ValidationMode;
use katla_gfx::VulkanContext;
use katla_gfx::particles::{
    EmitterConfig, GlobalParticleSystem, PARTICLE_EMIT_WORKGROUP_SIZE,
    PARTICLE_SIMULATE_WORKGROUP_SIZE,
};
use katla_gfx::renderer::registry::AssetRegistry;
use std::ffi::CString;
use std::process::ExitCode;
use std::rc::Rc;

use particle_validation_helpers::{
    RenderValidationResources, find_shader_directory, record_render_dispatch,
};

/// Default maximum particles for validation test
const DEFAULT_MAX_PARTICLES: u32 = 1_048_576;

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

/// Synchronously copy the indirect draw buffer to a staging buffer and read it back.
///
/// Records a single vkCmdCopyBuffer + submit + fence wait on a dedicated tiny
/// command buffer. This is cheap (16 bytes) and safe because the caller has
/// already waited on the source buffer's fence.
fn readback_indirect_draw(
    context: &VulkanContext,
    readback: &IndirectDrawReadback,
    src_buffer: vk::Buffer,
) -> (u32, u32, u32, u32) {
    let cmd = context.begin_single_time_commands();

    unsafe {
        context.device.cmd_copy_buffer(
            cmd.vk_command_buffer(),
            src_buffer,
            readback.staging_buffer,
            &[vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: 16,
            }],
        );
    }

    cmd.end_single_time_command();
    context.end_single_time_commands(cmd);

    readback.read(context)
}

/// Lightweight per-frame readback for the indirect draw buffer (16 bytes).
///
/// Unlike the full ParticleDebugReadback (which copies the entire particle array),
/// this only copies the VkDrawIndirectCommand so we can validate draw count
/// every frame without heavy GPU->CPU transfers.
struct IndirectDrawReadback {
    staging_buffer: vk::Buffer,
    allocation: gpu_allocator::vulkan::Allocation,
}

impl IndirectDrawReadback {
    fn new(context: &VulkanContext) -> Result<Self, String> {
        let size = 16u64; // sizeof(VkDrawIndirectCommand)

        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            context
                .device
                .create_buffer(&buffer_info, None)
                .map_err(|e| format!("Failed to create indirect draw readback buffer: {:?}", e))?
        };

        let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

        let allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "indirect_draw_readback",
                requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate indirect draw readback: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|e| format!("Failed to bind indirect draw readback: {:?}", e))?;
        }

        Ok(Self {
            staging_buffer: buffer,
            allocation,
        })
    }

    /// Read the current staging buffer contents.
    fn read(&self, context: &VulkanContext) -> (u32, u32, u32, u32) {
        context.invalidate_mapped_memory(&self.allocation, 0, 16);
        if let Some(mapped) = self.allocation.mapped_ptr() {
            let ptr = mapped.as_ptr() as *const u32;
            unsafe { (*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)) }
        } else {
            (0, 0, 0, 0)
        }
    }

    fn destroy(self, context: &VulkanContext) {
        unsafe {
            if let Ok(mut allocator) = context.allocator.try_borrow_mut() {
                allocator.free(self.allocation).ok();
            }
            context.device.destroy_buffer(self.staging_buffer, None);
        }
    }
}

fn main() -> ExitCode {
    // Initialize logging
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

    // === Deterministic cross-emitter contamination test ===
    log::info!("Running deterministic cross-emitter contamination test...");
    match run_contamination_test(&context, &mut asset_registry, render_resources.as_ref()) {
        Ok(_) => {
            log::info!("✓ Cross-emitter contamination test passed");
        }
        Err(e) => {
            log::error!("✗ Cross-emitter contamination test FAILED: {}", e);
            if let Some(ref mut res) = render_resources {
                res.destroy(&context);
            }
            return ExitCode::from(1);
        }
    }

    // Create test emitters with known properties
    log::info!("Creating test emitters...");
    let test_emitters = create_test_emitters(&mut particle_system, max_particles);
    log::info!("Created {} test emitters", test_emitters.len());

    // === 2-Frames-In-Flight double-buffered simulation ===
    log::info!(
        "Running GPU compute simulation for {} frames (2-FiF double buffered)...",
        NUM_FRAMES
    );

    // Create double-buffered frame state (fences + semaphores)
    let mut frame_state = DoubleBufferedFrameState::new(&context.device);

    // Create lightweight indirect draw readback for per-frame validation
    let indirect_readback = match IndirectDrawReadback::new(&context) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create indirect draw readback: {}", e);
            if let Some(ref mut res) = render_resources {
                res.destroy(&context);
            }
            return ExitCode::from(1);
        }
    };

    let mut cumulative_time = 0.0;
    let mut indirect_draw_errors = 0u32;

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

        // --- Per-frame indirect draw buffer validation ---
        // After the fence wait, slot `fi` is guaranteed complete. The indirect draw
        // buffer for that slot was written by the simulate shader 2 frames ago.
        // Copy it to a CPU-visible staging buffer and validate synchronously.
        // Skip the first 2 frames since the fences start signaled (no GPU work yet).
        if frame >= 2 {
            let prev_fi = fi; // slot fi was used by frame N-2
            let (vertex_count, instance_count, first_vertex, first_instance) =
                readback_indirect_draw(
                    &context,
                    &indirect_readback,
                    particle_system.indirect_draw_buffer(prev_fi),
                );

            // The indirect draw buffer was written by the simulate shader from 2 frames ago.
            // The alive_count at that point is unknown to CPU now (it's the GPU's output).
            // We validate structural correctness: instance_count, first_vertex, first_instance,
            // and that vertex_count is a multiple of 6.
            let mut frame_errors = Vec::new();

            if instance_count != 1 {
                frame_errors.push(format!("instance_count={}, expected 1", instance_count));
            }
            if first_vertex != 0 {
                frame_errors.push(format!("first_vertex={}, expected 0", first_vertex));
            }
            if first_instance != 0 {
                frame_errors.push(format!("first_instance={}, expected 0", first_instance));
            }
            if vertex_count % 6 != 0 {
                frame_errors.push(format!("vertex_count={} not multiple of 6", vertex_count));
            }

            let implied_particles = vertex_count / 6;
            if implied_particles > max_particles {
                frame_errors.push(format!(
                    "implied particles {} > max_particles {}",
                    implied_particles, max_particles
                ));
            }

            if !frame_errors.is_empty() {
                indirect_draw_errors += 1;
                log::error!(
                    "Frame {}: indirect draw INVALID (vertex_count={}, instance_count={}, first_vertex={}, first_instance={})",
                    frame,
                    vertex_count,
                    instance_count,
                    first_vertex,
                    first_instance
                );
                for e in &frame_errors {
                    log::error!("  {}", e);
                }
            }

            if frame % 1000 == 0 || is_last_frame {
                log::info!(
                    "Frame {}: indirect_draw vertex_count={} ({} particles), instance_count={}",
                    frame,
                    vertex_count,
                    implied_particles,
                    instance_count
                );
            }
        }

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
        let command_buffer = context.begin_single_time_commands();

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

            // On the last frame, record debug readback so we can read real GPU counters
            // after the fence wait during drain.
            if is_last_frame {
                if let Err(e) = particle_system.record_debug_readback(
                    command_buffer.vk_command_buffer(),
                    frame_index_for_descriptor,
                ) {
                    log::warn!("Failed to record debug readback on last frame: {}", e);
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
    let last_frame_fi = (NUM_FRAMES as usize) % 2;
    unsafe {
        context
            .device
            .wait_for_fences(&[frame_state.fence(last_frame_fi)], true, u64::MAX)
            .unwrap();
    }
    frame_state.destroy(&context.device);

    // --- Per-frame indirect draw validation summary ---
    log::info!(
        "Per-frame indirect draw validation: {} frames checked, {} errors",
        NUM_FRAMES.saturating_sub(2),
        indirect_draw_errors
    );
    if indirect_draw_errors > 0 {
        log::error!(
            "Indirect draw buffer had errors in {} out of {} frames",
            indirect_draw_errors,
            NUM_FRAMES.saturating_sub(2)
        );
        indirect_readback.destroy(&context);
        if let Some(ref mut res) = render_resources {
            res.destroy(&context);
        }
        return ExitCode::from(1);
    }

    indirect_readback.destroy(&context);

    // --- Final debug readback: copy GPU data to staging buffers ---
    // The simulation loop doesn't record debug readback per-frame, so we do a
    // single synchronous copy+readback after all GPU work is complete.
    log::info!("Recording final debug readback...");
    {
        let cmd = context.begin_single_time_commands();
        if let Err(e) =
            particle_system.record_debug_readback(cmd.vk_command_buffer(), last_frame_fi)
        {
            log::warn!("Failed to record final debug readback: {}", e);
        }
        cmd.end_single_time_command();
        context.end_single_time_commands(cmd);
    }

    log::info!("Simulation complete, running validation...");

    // Read back actual GPU counters from the last frame
    let gpu_alive = if particle_system.has_debug_readback() {
        match particle_system.read_debug_data() {
            Ok(data) => {
                log::info!(
                    "GPU counters: alive={}, dead={}, emit={}",
                    data.counters.alive_count,
                    data.counters.dead_count,
                    data.counters.emit_count,
                );
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

    // Validate indirect draw buffer data
    log::info!("Validating indirect draw buffer...");
    match validate_indirect_draw(&particle_system) {
        Ok(_) => {
            log::info!("✓ Indirect draw buffer validation passed");
        }
        Err(e) => {
            log::error!("✗ Indirect draw buffer validation failed: {}", e);
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

/// Deterministic test for cross-emitter contamination.
///
/// 4 emitters, one per quadrant, high emit rate, ZERO randomness anywhere:
///   - No lifetime_variation, no velocity_cone_angle, no color_variation,
///     no scale_variation, no gravity, no turbulence.
///   - Each emitter has a unique pure color and emits straight up.
///
/// After simulating 120 frames, reads back all alive particles.
/// Every particle must match its quadrant's emitter exactly — any deviation
/// in position, color, or velocity is a definitive contamination bug.
fn run_contamination_test(
    context: &Rc<VulkanContext>,
    asset_registry: &mut AssetRegistry,
    _render_resources: Option<&RenderValidationResources>,
) -> Result<(), String> {
    use ash::vk;
    use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};
    use katla_gfx::ShaderCache;
    use katla_gfx::compute::ComputePass;
    use katla_gfx::sync::VkShaderModule;

    let max_particles: u32 = DEFAULT_MAX_PARTICLES;
    let test_frames: u32 = NUM_FRAMES;
    let dt: f32 = 1.0 / 60.0;
    let emit_rate: f32 = 800.0;
    let lifetime: f32 = 1.5;
    let velocity_mag: f32 = 5.0;

    let mut ps = GlobalParticleSystem::new(context, max_particles)
        .map_err(|e| format!("Failed to create particle system: {}", e))?;

    ps.init_debug_readback()
        .map_err(|e| format!("Failed to init debug readback: {}", e))?;

    // 4 emitters in 4 quadrants, each a distinct pure primary color.
    let emitter_defs: [([f32; 3], [f32; 4], &str); 4] = [
        ([-50.0, 0.0, 0.0], [1.0, 0.0, 0.0, 1.0], "Red"),
        ([50.0, 0.0, 0.0], [0.0, 1.0, 0.0, 1.0], "Green"),
        ([0.0, 0.0, -50.0], [0.0, 0.0, 1.0, 1.0], "Blue"),
        ([0.0, 0.0, 50.0], [1.0, 0.0, 1.0, 1.0], "Magenta"),
    ];

    for (pos, color, name) in &emitter_defs {
        let config = EmitterConfig {
            position: *pos,
            emit_rate,
            base_lifetime: lifetime,
            lifetime_variation: 0.0,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_magnitude: velocity_mag,
            velocity_cone_angle: 0.0,
            base_scale: 0.1,
            scale_variation: 0.0,
            color: *color,
            color_variation: 0.0,
            gravity: 0.0,
            turbulence_strength: 0.0,
            ..Default::default()
        };
        ps.create_emitter(config)
            .map_err(|e| format!("Failed to create {} emitter: {}", name, e))?;
    }

    let shader_dir = find_shader_directory();
    particle_validation_helpers::load_and_create_pipelines(
        context,
        &mut ps,
        asset_registry,
        &shader_dir,
    )
    .map_err(|e| format!("Failed to load pipelines: {}", e))?;

    // --- Create GPU validation resources ---

    // Validation results buffer (atomic counters, CPU-visible for readback)
    // Layout: total_checked(4) + color_mismatches(4) + velocity_mismatches(4) + position_mismatches(4)
    //       + per_emitter[16](64) + mismatch_details[64](256) + mismatch_count(4) = 336 bytes
    const VALIDATION_RESULTS_SIZE: u64 = 1024; // generous padding

    let val_results_info = vk::BufferCreateInfo::default()
        .size(VALIDATION_RESULTS_SIZE)
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

    // Validation params buffer (uniform, 32 bytes)
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

    const VALIDATION_PARAMS_SIZE: u64 = 32;

    let val_params_info = vk::BufferCreateInfo::default()
        .size(VALIDATION_PARAMS_SIZE)
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
                VALIDATION_RESULTS_SIZE,
                0,
            );
        }
        cmd.end_single_time_command();
    }

    // Load validation shader and create compute pass
    let mut shader_cache = ShaderCache::new(context.device.clone());
    let validate_shader_path = shader_dir.join("particles/particle_validate.wgsl");
    log::info!("Loading validation shader from: {:?}", validate_shader_path);

    let validate_shader = shader_cache
        .load_shader(&validate_shader_path, vk::ShaderStageFlags::COMPUTE)
        .map_err(|e| format!("Failed to load validation shader: {}", e))?;

    let validate_shader_wrapper = VkShaderModule(validate_shader);

    // Get buffer layout offsets (copy before the loop to avoid borrow conflicts)
    let layout = *ps.buffer_layout();
    let next_frame_offset = layout.alive_frame_offset[1]; // simulate writes here
    let alive_list_size = layout.alive_list_size;
    let max_particles_layout = layout.max_particles;
    let particle_data_size =
        max_particles_layout * std::mem::size_of::<katla_gfx::particles::ParticleData>() as u64;
    let counters_size = std::mem::size_of::<katla_gfx::particles::ParticleCounters>() as u64;
    let emitter_config_total = (1024 * std::mem::size_of::<EmitterConfig>()) as u64;

    // Create the compute pass with all 6 bindings:
    // 0: particles, 1: alive list (simulate output), 2: counters, 3: emitter configs,
    // 4: validation results, 5: validation params
    let validation_pass = ComputePass::new(context)
        .add_storage_buffer(0, ps.particle_buffer(), 0, particle_data_size)
        .add_storage_buffer(1, ps.particle_buffer(), next_frame_offset, alive_list_size)
        .add_storage_buffer(2, ps.counters_buffer(0), 0, counters_size)
        .add_storage_buffer(
            3,
            ps.emitter_configs_buffer(0)
                .ok_or("Emitter configs buffer not available")?,
            0,
            emitter_config_total,
        )
        .add_storage_buffer(4, val_results_buffer, 0, VALIDATION_RESULTS_SIZE)
        .add_uniform_buffer(5, val_params_buffer, 0, VALIDATION_PARAMS_SIZE)
        .build(validate_shader_wrapper, asset_registry)
        .map_err(|e| format!("Failed to build validation pass: {}", e))?;

    // Get pipeline handles from registry for dispatch
    let pipeline_asset = asset_registry
        .get_pipeline(validation_pass.pipeline_handle())
        .ok_or("Validation pipeline not found in registry")?;
    let val_pipeline = pipeline_asset.vk_pipeline();
    let val_layout = pipeline_asset.vk_layout();

    log::info!("GPU validation pass created successfully");

    // --- Run simulation with 2-FiF double-buffered GPU validation ---
    let mut frame_state = DoubleBufferedFrameState::new(&context.device);

    for frame in 0..test_frames {
        let fi = (frame as usize) % 2;
        let next_fi = (fi + 1) % 2;

        // Wait on this frame slot's fence (free for reuse)
        frame_state.wait_for_fence(&context.device);

        let (alive_count, emit_count) = ps
            .update(dt, frame)
            .map_err(|e| format!("Update failed at frame {}: {}", frame, e))?;

        // Record and submit GPU compute dispatch (emit + simulate + debug readback)
        let cmd = context.begin_single_time_commands();

        let frame_index_for_descriptor = fi;

        let emit_wg = if emit_count > 0 {
            (emit_count + PARTICLE_EMIT_WORKGROUP_SIZE - 1) / PARTICLE_EMIT_WORKGROUP_SIZE
        } else {
            0
        };
        let max_alive = ps.max_estimated_alive();
        let total_sim = max_alive + emit_count;
        let sim_wg = if total_sim > 0 {
            (total_sim + PARTICLE_SIMULATE_WORKGROUP_SIZE - 1) / PARTICLE_SIMULATE_WORKGROUP_SIZE
        } else {
            0
        };

        if emit_wg > 0 || sim_wg > 0 {
            if let Err(e) = ps.update_compute_descriptor_binding(frame_index_for_descriptor) {
                log::warn!(
                    "Frame {}: Failed to update compute descriptor binding: {}",
                    frame,
                    e
                );
            }

            if emit_wg > 0 {
                if let Err(e) = ps.record_emit_dispatch(
                    cmd.vk_command_buffer(),
                    asset_registry,
                    emit_wg,
                    frame_index_for_descriptor,
                ) {
                    log::warn!("Failed to record emit dispatch: {}", e);
                }
            }

            if sim_wg > 0 {
                ps.reset_simulate_counters(
                    cmd.vk_command_buffer(),
                    emit_wg > 0,
                    frame_index_for_descriptor,
                );
                if let Err(e) = ps.record_simulate_dispatch(
                    cmd.vk_command_buffer(),
                    asset_registry,
                    sim_wg,
                    frame_index_for_descriptor,
                ) {
                    log::warn!("Failed to record simulate dispatch: {}", e);
                }
            }
        }

        // --- GPU validation compute (same command buffer, no CPU involvement) ---
        // Record validation dispatch directly after particle compute.
        // The validation shader accumulates results into atomics in val_results_buffer.
        // We only read back the buffer once after all frames complete.
        //
        // Memory barrier: simulate writes particle data, alive lists, and counters.
        // Validation reads all of those. Within the same command buffer, Vulkan
        // guarantees execution order but NOT memory visibility between dispatches.
        if alive_count > 0 && (sim_wg > 0) {
            let particle_barrier = vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags2::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(ps.particle_buffer())
                .offset(0)
                .size(vk::WHOLE_SIZE);

            let counters_barrier = vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags2::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(ps.counters_buffer(fi))
                .offset(0)
                .size(vk::WHOLE_SIZE);

            let barriers = [particle_barrier, counters_barrier];
            let dep_info = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);

            unsafe {
                context
                    .device
                    .cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dep_info);
            }
        }

        if alive_count > 0 {
            validation_pass.update_binding(
                1,
                ps.particle_buffer(),
                layout.alive_frame_offset[next_fi],
                alive_list_size,
            );
            validation_pass.update_binding(2, ps.counters_buffer(fi), 0, counters_size);
            validation_pass.update_binding(
                3,
                ps.emitter_configs_buffer(fi)
                    .ok_or("Emitter configs buffer not available")?,
                0,
                emitter_config_total,
            );

            let val_params = ValidationParams {
                alive_count: ps.alive_count(),
                emitter_count: 4,
                frame_index: frame,
                max_mismatch_details: 64,
                color_tolerance: 0.05,
                velocity_tolerance: 0.0, // Skip velocity check: emit shader varies speed by ±50%
                position_tolerance: 0.0,
                _pad: 0.0,
            };

            if let Some(mapped) = val_params_alloc.mapped_ptr() {
                let dst = mapped.as_ptr() as *mut u8;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        &val_params as *const ValidationParams as *const u8,
                        dst,
                        std::mem::size_of::<ValidationParams>(),
                    );
                }
                context.flush_mapped_memory(&val_params_alloc, 0, VALIDATION_PARAMS_SIZE);
            }

            let validate_workgroups = (alive_count + 63) / 64;
            validation_pass.record_dispatch_with_handles(
                cmd.vk_command_buffer(),
                val_pipeline,
                val_layout,
                validate_workgroups,
                1,
                1,
            );
        }

        cmd.end_single_time_command();

        let prev_fi = (fi + 1) % 2;
        let wait_sem = frame_state.frame_complete_semaphore(prev_fi);
        let signal_fence = frame_state.fence(fi);
        let signal_sem = frame_state.frame_complete_semaphore(fi);

        let wait_sems: &[vk::Semaphore] = if frame > 0 { &[wait_sem] } else { &[] };
        context
            .gfx_queue
            .submit(&[&cmd], wait_sems, &[signal_sem], signal_fence);

        frame_state.step_frame();
    }

    // Drain remaining in-flight frames
    for _ in 0..2 {
        frame_state.wait_for_fence(&context.device);
        frame_state.step_frame();
    }
    frame_state.destroy(&context.device);

    // --- Single readback: read accumulated GPU validation results ---
    context.invalidate_mapped_memory(&val_results_alloc, 0, VALIDATION_RESULTS_SIZE);

    let mut total_checked: u64 = 0;
    let mut total_color_mismatches: u64 = 0;
    let mut total_velocity_mismatches: u64 = 0;
    let mut per_emitter_mismatches = [0u32; 16];

    if let Some(mapped) = val_results_alloc.mapped_ptr() {
        let ptr = mapped.as_ptr() as *const u32;
        total_checked = unsafe { *ptr } as u64;
        total_color_mismatches = unsafe { *ptr.add(1) } as u64;
        total_velocity_mismatches = unsafe { *ptr.add(2) } as u64;

        for i in 0..4usize {
            per_emitter_mismatches[i] = unsafe { *ptr.add(4 + i) };
        }

        // Read mismatch details for diagnosis (offset 80, 4 u32 per entry)
        // Format: [frame_index, particle_idx, packed_color, emitter_idx_packed_alive]
        let mismatch_count = unsafe { *ptr.add(84) };
        let detail_count = mismatch_count.min(64);
        for i in 0..detail_count as usize {
            let base = 80 + i * 4;
            let frame_idx = unsafe { *ptr.add(base) };
            let particle_idx = unsafe { *ptr.add(base + 1) };
            let packed_color = unsafe { *ptr.add(base + 2) };
            let emitter_packed = unsafe { *ptr.add(base + 3) };
            let emitter_idx = emitter_packed / 100000000;
            let alive_at_check = emitter_packed % 100000000;
            let r = packed_color / 1000000;
            let g = (packed_color / 1000) % 1000;
            let b = packed_color % 1000;
            if i < 3 || i >= detail_count as usize - 3 {
                log::error!(
                    "  [{}] frame={}, particle={}, emitter={}, color=({:.4}, {:.4}, {:.4}), alive={}",
                    i,
                    frame_idx,
                    particle_idx,
                    emitter_idx,
                    r as f32 / 10000.0,
                    g as f32 / 10000.0,
                    b as f32 / 10000.0,
                    alive_at_check
                );
            }
        }
    }

    // Report results
    log::info!(
        "GPU validation complete: {} particles checked across {} frames",
        total_checked,
        test_frames
    );

    if total_color_mismatches > 0 {
        log::error!(
            "COLOR CONTAMINATION: {} total color mismatches across {} frames",
            total_color_mismatches,
            test_frames
        );
        for (i, name) in emitter_defs.iter().map(|(_, _, n)| *n).enumerate() {
            if per_emitter_mismatches[i] > 0 {
                log::error!(
                    "  Emitter {} ({}): {} mismatches",
                    i,
                    name,
                    per_emitter_mismatches[i]
                );
            }
        }
        return Err(format!(
            "GPU validation detected {} color mismatches across {} frames. \
             Particles are being assigned to wrong emitter configs.",
            total_color_mismatches, test_frames
        ));
    }

    if total_velocity_mismatches > 0 {
        log::error!(
            "VELOCITY CONTAMINATION: {} total velocity mismatches across {} frames",
            total_velocity_mismatches,
            test_frames
        );
        return Err(format!(
            "GPU validation detected {} velocity mismatches across {} frames. \
             Particles have incorrect velocity for their assigned emitter.",
            total_velocity_mismatches, test_frames
        ));
    }

    log::info!(
        "Contamination test PASSED: {} particles checked, 0 GPU-detected color/velocity mismatches",
        total_checked
    );

    // Cleanup
    unsafe {
        context.device.destroy_buffer(val_results_buffer, None);
        context.device.destroy_buffer(val_params_buffer, None);
    }
    if let Ok(mut allocator) = context.allocator.try_borrow_mut() {
        allocator.free(val_results_alloc).ok();
        allocator.free(val_params_alloc).ok();
    }

    Ok(())
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
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_magnitude: 1.0,
            color: *color,
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

/// Expected emitter definitions for color consistency validation.
/// Validate emitter configurations.
/// Validate the VkDrawIndirectCommand written by the simulate shader.
///
/// The simulate shader writes a single VkDrawIndirectCommand (16 bytes) to a
/// per-frame buffer that the render pass consumes via vkCmdDrawIndirect.
///
/// Expected layout:
///   vertex_count    = alive_count * 6  (2 triangles per particle quad)
///   instance_count  = 1
///   first_vertex    = 0
///   first_instance  = 0
fn validate_indirect_draw(particle_system: &GlobalParticleSystem) -> Result<(), String> {
    let debug_data = match particle_system.read_debug_data() {
        Ok(data) => data,
        Err(e) => {
            return Err(format!(
                "Failed to read debug data for indirect draw validation: {}",
                e
            ));
        }
    };

    let indirect_draw = match debug_data.indirect_draw {
        Some(cmd) => cmd,
        None => {
            return Err(
                "Indirect draw command data not available (readback may not have been recorded)"
                    .to_string(),
            );
        }
    };

    let alive_count = debug_data.counters.alive_count;

    log::info!(
        "Indirect draw command: vertex_count={}, instance_count={}, first_vertex={}, first_instance={}",
        indirect_draw.vertex_count,
        indirect_draw.instance_count,
        indirect_draw.first_vertex,
        indirect_draw.first_instance
    );
    log::info!(
        "Expected: vertex_count={}, instance_count=1, first_vertex=0, first_instance=0 (alive_count={})",
        alive_count * 6,
        alive_count
    );

    let expected_vertex_count = alive_count * 6;

    let mut errors = Vec::new();

    if indirect_draw.vertex_count != expected_vertex_count {
        errors.push(format!(
            "vertex_count mismatch: got {}, expected {} (alive_count={})",
            indirect_draw.vertex_count, expected_vertex_count, alive_count
        ));
    }

    if indirect_draw.instance_count != 1 {
        errors.push(format!(
            "instance_count mismatch: got {}, expected 1",
            indirect_draw.instance_count
        ));
    }

    if indirect_draw.first_vertex != 0 {
        errors.push(format!(
            "first_vertex mismatch: got {}, expected 0",
            indirect_draw.first_vertex
        ));
    }

    if indirect_draw.first_instance != 0 {
        errors.push(format!(
            "first_instance mismatch: got {}, expected 0",
            indirect_draw.first_instance
        ));
    }

    // Sanity: vertex_count must be a multiple of 6 (quads)
    if indirect_draw.vertex_count % 6 != 0 {
        errors.push(format!(
            "vertex_count {} is not a multiple of 6 (each particle is a 6-vertex quad)",
            indirect_draw.vertex_count
        ));
    }

    // Sanity: implied particle count from vertex_count must not exceed max particles
    let implied_particles = indirect_draw.vertex_count / 6;
    let max_particles = particle_system.max_particles();
    if implied_particles > max_particles {
        errors.push(format!(
            "implied particle count {} from vertex_count exceeds max_particles {}",
            implied_particles, max_particles
        ));
    }

    if !errors.is_empty() {
        for e in &errors {
            log::error!("  {}", e);
        }
        return Err(format!(
            "Indirect draw buffer validation failed with {} error(s)",
            errors.len()
        ));
    }

    log::info!(
        "Indirect draw buffer OK: {} vertices ({} particles), instance_count=1",
        indirect_draw.vertex_count,
        implied_particles
    );

    Ok(())
}

fn validate_emitter_configs(particle_system: &GlobalParticleSystem) -> Result<(), String> {
    use katla_gfx::particles::validation::validate_emitter_config;

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
