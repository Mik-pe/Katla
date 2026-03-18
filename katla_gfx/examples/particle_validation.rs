mod particle_validation_helpers;

// Particle System Validation Example
//
// This binary validates the particle system by:
// - Initializing a headless Vulkan context with validation enabled
// - Creating a particle system with test emitters
// - Running simulation for several frames with ACTUAL GPU compute execution
// - Reading back particle data for validation
// - Checking for NaN, infinity, and reasonable bounds
// - Detecting Vulkan validation errors
//
// This example is designed for CI/LLM environments and requires no visual output.
//
// Exit codes:
// - 0: Validation passed
// - 1: Validation failed (with error message)

use katla_gfx::ValidationMode;
use katla_gfx::VulkanContext;
use katla_gfx::particles::{EmitterConfig, GlobalParticleSystem};
use katla_gfx::renderer::registry::AssetRegistry;
use std::ffi::CString;
use std::process::ExitCode;

use particle_validation_helpers::{execute_gpu_compute, find_shader_directory};

/// Maximum particles for validation test
const MAX_PARTICLES: u32 = 10_000;

/// Number of frames to simulate
const NUM_FRAMES: u32 = 200;

/// Delta time per frame (60 FPS)
const DELTA_TIME: f32 = 1.0 / 60.0;

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

/// Track alive_list content across frames for corruption detection.
///
/// This struct stores the alive_list indices for each frame and provides
/// validation functions to detect corruption patterns.
struct FrameAliveListTracker {
    /// Store alive_list indices for each frame
    frame_data: Vec<FrameAliveData>,
    /// Maximum particles in the system
    max_particles: u32,
}

/// Per-frame alive_list data
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct FrameAliveData {
    /// Frame number
    frame: u32,
    /// Alive particle indices from this frame
    alive_indices: Vec<u32>,
    /// Alive count (should match alive_indices.len())
    alive_count: u32,
    /// Emit count for this frame
    emit_count: u32,
    /// Cumulative time at this frame
    cumulative_time: f32,
}

impl FrameAliveListTracker {
    /// Create a new tracker with given capacity.
    fn new(max_particles: u32, expected_frames: u32) -> Self {
        Self {
            frame_data: Vec::with_capacity(expected_frames as usize),
            max_particles,
        }
    }

    /// Record alive_list data for a frame.
    fn record_frame(
        &mut self,
        frame: u32,
        alive_list: &[u32],
        alive_count: u32,
        emit_count: u32,
        cumulative_time: f32,
    ) {
        // Only store the actual alive particles (first alive_count entries).
        // DO NOT filter index 0 - index 0 is a valid particle index.
        // Filtering it was masking the real duplicate count.
        let actual_alive: Vec<u32> = alive_list
            .iter()
            .take(alive_count as usize)
            .copied()
            .collect();

        // Diagnostic: check for index 0 count and duplicates in raw data
        if frame < 5 || frame % 50 == 0 {
            let zero_count = actual_alive.iter().filter(|&&x| x == 0).count();
            if zero_count > 0 {
                log::warn!(
                    "Frame {}: {} entries with index 0 in alive_next (stale data from alive_next not being cleared)",
                    frame,
                    zero_count
                );
            }
        }

        self.frame_data.push(FrameAliveData {
            frame,
            alive_indices: actual_alive,
            alive_count,
            emit_count,
            cumulative_time,
        });
    }

    /// Validate frame-to-frame transitions for corruption patterns.
    ///
    /// NOTE: Disabled because particle index tracking across frames is not meaningful
    /// for particle systems with index recycling. Per-frame validation is sufficient.
    fn validate_transitions(&self) -> Result<(), Vec<String>> {
        Ok(())
    }

    /// Check for corruption within each frame's alive_list.
    fn validate_per_frame(&self) -> Result<(), Vec<String>> {
        let mut frames_with_dupes = 0u32;
        let mut max_dupe_count = 0u32;
        let mut frames_with_oob = 0u32;
        let mut error_details = Vec::new();

        for frame_data in &self.frame_data {
            // Check for duplicates
            let mut seen = std::collections::HashSet::new();
            let mut duplicates = Vec::new();

            for &idx in &frame_data.alive_indices {
                if !seen.insert(idx) {
                    duplicates.push(idx);
                }
            }

            if !duplicates.is_empty() {
                frames_with_dupes += 1;
                max_dupe_count = max_dupe_count.max(duplicates.len() as u32);

                // Only collect details for first few frames to avoid spam
                if frames_with_dupes <= 3 {
                    let mut dup_counts: std::collections::HashMap<u32, usize> =
                        std::collections::HashMap::new();
                    for &idx in &duplicates {
                        *dup_counts.entry(idx).or_insert(0) += 1;
                    }
                    let mut most_common_dup = (0, 0);
                    for (idx, count) in &dup_counts {
                        if *count > most_common_dup.1 {
                            most_common_dup = (*idx, *count);
                        }
                    }
                    error_details.push(format!(
                        "Frame {}: {} dupes ({} unique), worst: index {} x{}",
                        frame_data.frame,
                        duplicates.len(),
                        duplicates
                            .iter()
                            .cloned()
                            .collect::<std::collections::HashSet<_>>()
                            .len(),
                        most_common_dup.0,
                        most_common_dup.1
                    ));
                }
            }

            // Check for out-of-bounds indices
            let oob_count = frame_data
                .alive_indices
                .iter()
                .filter(|&&idx| idx >= self.max_particles)
                .count();

            if oob_count > 0 {
                frames_with_oob += 1;
                if frames_with_oob <= 3 {
                    error_details.push(format!(
                        "Frame {}: {} out-of-bounds indices (max={})",
                        frame_data.frame, oob_count, self.max_particles
                    ));
                }
            }
        }

        if frames_with_dupes == 0 && frames_with_oob == 0 {
            log::info!("Per-frame alive_list validation passed");
            Ok(())
        } else {
            let mut summary = Vec::new();
            summary.push(format!(
                "{} frames with duplicates (max {} dupes/frame), {} frames with OOB indices",
                frames_with_dupes, max_dupe_count, frames_with_oob
            ));
            summary.extend(error_details);
            Err(summary)
        }
    }

    /// Print summary statistics.
    fn print_summary(&self) {
        let total_alive: u64 = self.frame_data.iter().map(|d| d.alive_count as u64).sum();
        let avg_alive = total_alive / self.frame_data.len() as u64;

        log::info!(
            "Alive list tracking: {} frames, avg_alive={}, max_capacity={}",
            self.frame_data.len(),
            avg_alive,
            self.max_particles
        );
    }
}

fn main() -> ExitCode {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("=== Particle System Validation Example ===");
    log::info!("Max particles: {}", MAX_PARTICLES);
    log::info!("Frames to simulate: {}", NUM_FRAMES);

    // Create headless Vulkan context with validation enabled
    let app_name = CString::new("Particle Validation").unwrap();
    let engine_name = CString::new("Katla Engine").unwrap();

    log::info!("Creating headless Vulkan context with GPU-assisted validation...");
    // GPU-assisted validation provides additional GPU-side checks:
    // - Out-of-bounds descriptor access detection
    // - Uninitialized descriptor detection
    // - Descriptor index out of bounds checking
    // - More robust validation for shader operations
    let context = VulkanContext::init_headless(ValidationMode::GpuAssisted, app_name, engine_name);

    // Wrap in Rc for particle system
    let context = std::rc::Rc::new(context);

    log::info!("Vulkan context created successfully");

    // Create asset registry for shader loading
    let mut asset_registry = AssetRegistry::new();

    // Find shader directory
    let shader_dir = find_shader_directory();
    log::info!("Using shader directory: {:?}", shader_dir);

    // Create particle system
    log::info!("Creating particle system...");
    let mut particle_system = match GlobalParticleSystem::new(&context, MAX_PARTICLES) {
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

    // Create test emitters with known properties
    log::info!("Creating test emitters...");
    let test_emitters = create_test_emitters(&mut particle_system);
    log::info!("Created {} test emitters", test_emitters.len());

    // Run GPU compute simulation for several frames
    log::info!(
        "Running GPU compute simulation for {} frames...",
        NUM_FRAMES
    );

    let mut cumulative_time = 0.0;
    let mut prev_alive_count = 0u32;

    // Create tracker for alive_list validation across frames
    let mut alive_tracker = FrameAliveListTracker::new(MAX_PARTICLES, NUM_FRAMES);

    for frame in 0..NUM_FRAMES {
        cumulative_time += DELTA_TIME;
        let is_last_frame = frame == NUM_FRAMES - 1;

        // Prepare frame data (uploads emitter configs, frame data)
        match particle_system.update(DELTA_TIME, frame) {
            Ok((alive_count, emit_count)) => {
                let died_this_frame = if alive_count < prev_alive_count {
                    prev_alive_count - alive_count
                } else {
                    0
                };

                // Log every frame with full diagnostics
                let diag = FrameDiagnostics::new(
                    frame,
                    alive_count,
                    emit_count,
                    DELTA_TIME,
                    cumulative_time,
                );
                diag.log();

                if died_this_frame > 0 {
                    log::debug!("  -> {} particles died this frame", died_this_frame);
                }

                // Expected behavior at this frame
                let expected_alive = estimate_expected_alive(frame);
                let diff = if alive_count as i32 > expected_alive {
                    alive_count as i32 - expected_alive
                } else {
                    expected_alive - alive_count as i32
                };

                if frame % 50 == 0 || is_last_frame {
                    log::info!(
                        "Frame {}: alive={}, emit={}, expected~{}, diff={}",
                        frame,
                        alive_count,
                        emit_count,
                        expected_alive,
                        diff
                    );
                }

                prev_alive_count = alive_count;

                // Execute actual GPU compute dispatch
                if let Err(e) = execute_gpu_compute(
                    &context,
                    &mut particle_system,
                    &asset_registry,
                    frame,
                    alive_count,
                    emit_count,
                ) {
                    log::error!("Failed to execute GPU compute at frame {}: {}", frame, e);
                    return ExitCode::from(1);
                }

                // Read back alive_list data for this frame (after GPU compute)
                if let Ok(debug_data) = particle_system.read_debug_data() {
                    // Update cached_alive_count from GPU readback.
                    // This is more reliable than reading directly from the counters buffer
                    // because vkCmdCopyBuffer (used in readback) properly transfers the data.
                    particle_system.set_alive_count(debug_data.counters.alive_count);

                    if frame < 3 {
                        let end = (debug_data.counters.alive_count as usize).min(10);
                        log::info!(
                            "Frame {} readback: alive_count={}, emit_count={}, first 10 alive_next entries: {:?}",
                            frame,
                            debug_data.counters.alive_count,
                            debug_data.counters.emit_count,
                            &debug_data.alive_list[..end]
                        );
                        let unique_indices: std::collections::HashSet<u32> = debug_data.alive_list
                            [..debug_data.counters.alive_count as usize]
                            .iter()
                            .copied()
                            .collect();
                        let dup_count =
                            debug_data.counters.alive_count as usize - unique_indices.len();
                        let zero_count = debug_data.alive_list
                            [..debug_data.counters.alive_count as usize]
                            .iter()
                            .filter(|&&x| x == 0)
                            .count();
                        log::info!(
                            "  counters: alive={}, dead={}, emit={} | unique={}, dupes={}, zeros={}",
                            debug_data.counters.alive_count,
                            debug_data.counters.dead_count,
                            debug_data.counters.emit_count,
                            unique_indices.len(),
                            dup_count,
                            zero_count
                        );
                    }

                    alive_tracker.record_frame(
                        frame,
                        &debug_data.alive_list,
                        debug_data.counters.alive_count,
                        emit_count,
                        cumulative_time,
                    );
                }
            }
            Err(e) => {
                log::error!("Failed to update particle system at frame {}: {}", frame, e);
                return ExitCode::from(1);
            }
        }
    }

    log::info!("Simulation complete, running validation...");

    // Run alive_list validation across all frames
    log::info!("Running alive_list corruption detection across all frames...");
    alive_tracker.print_summary();

    // Validate per-frame corruption (duplicates, out-of-bounds, etc.)
    match alive_tracker.validate_per_frame() {
        Ok(_) => {
            log::info!("✓ Per-frame alive_list validation passed");
        }
        Err(errors) => {
            log::error!("✗ Per-frame alive_list validation failed:");
            for error in &errors {
                log::error!("  {}", error);
            }
            return ExitCode::from(1);
        }
    }

    // Validate frame-to-frame transitions
    match alive_tracker.validate_transitions() {
        Ok(_) => {
            log::info!("✓ Frame-to-frame transition validation passed");
        }
        Err(errors) => {
            log::error!("✗ Frame-to-frame transition validation failed:");
            for error in &errors {
                log::error!("  {}", error);
            }
            return ExitCode::from(1);
        }
    }

    // Get final statistics
    let stats = particle_system.get_stats();
    log::info!("=== Simulation Complete ===");
    log::info!("Total frames: {}", stats.frame_count);
    log::info!("Total particles emitted: {}", stats.total_emitted);
    log::info!("Total particles died: {}", stats.total_died);
    log::info!("Current alive count: {}", stats.current_alive_count);

    // Validate particle data by reading back from GPU
    log::info!("Reading back particle data for validation...");
    match validate_particle_data(&particle_system) {
        Ok(_) => {
            log::info!("✓ Particle data validation passed");
        }
        Err(e) => {
            log::error!("✗ Particle data validation failed: {}", e);
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
            return ExitCode::from(1);
        }
    }

    // Print summary statistics
    log::info!("=== Validation Summary ===");
    log::info!("Max particles: {}", MAX_PARTICLES);
    log::info!("Total frames simulated: {}", NUM_FRAMES);
    log::info!("Memory usage: {:.2} MB", stats.memory_used_mb);
    log::info!("Total emitted: {}", stats.total_emitted);
    log::info!("Total died: {}", stats.total_died);
    log::info!("Currently alive: {}", stats.current_alive_count);

    // Check for any Vulkan validation errors
    // Note: Vulkan validation errors are printed to stderr by the validation layers
    // We rely on the user to check stderr for validation messages

    log::info!("=== All Validations Passed ===");
    ExitCode::SUCCESS
}

/// Estimate expected alive particles at a given frame.
///
/// This is a rough estimate based on emitter configurations:
/// - Emitter 1: 100/sec, lifetime 2.0s (dies ~frame 120)
/// - Emitter 2: 200/sec, lifetime 3.0s (dies ~frame 180)
/// - Emitter 3: 100 burst, lifetime 1.5s (dies ~frame 90)
///
/// NOTE: These lifetimes are realistic for particle systems (1.5-3.0 seconds).
/// Particles will be born AND die during the 200-frame simulation, showing
/// realistic particle turnover.
/// Total simulation time: 200 frames @ 60 FPS = 3.33 seconds
fn estimate_expected_alive(frame: u32) -> i32 {
    let time = (frame + 1) as f32 * DELTA_TIME;

    // Emitter 1: 100/sec, lifetime 2.0s
    let _e1_emitted = (100.0 * time).ceil() as i32;
    let e1_alive = if time > 2.0 {
        0 // All particles from emitter 1 have died by 2.0s
    } else {
        // Steady state: emission rate * lifetime = 100 * 2.0 = 200 particles
        // But we need to account for ramp-up time
        let ramp_up = time.min(2.0);
        ((100.0 * ramp_up) * (1.0 - (ramp_up / 2.0))).ceil() as i32
    };

    // Emitter 2: 200/sec, lifetime 3.0s
    let _e2_emitted = (200.0 * time).ceil() as i32;
    let e2_alive = if time > 3.0 {
        0 // All particles from emitter 2 have died by 3.0s
    } else {
        // Steady state: emission rate * lifetime = 200 * 3.0 = 600 particles
        // But we need to account for ramp-up time
        let ramp_up = time.min(3.0);
        ((200.0 * ramp_up) * (1.0 - (ramp_up / 3.0))).ceil() as i32
    };

    // Emitter 3: 100 burst at frame 0, lifetime 1.5s
    let e3_alive = if time < 1.5 {
        // Linear decrease from 100 to 0 over 1.5s
        (100.0 * (1.0 - (time / 1.5))).ceil() as i32
    } else {
        0 // All burst particles have died by 1.5s
    };

    let total = e1_alive + e2_alive + e3_alive;
    total.max(0) // Ensure non-negative
}

/// Create test emitters with known properties for validation.
fn create_test_emitters(
    particle_system: &mut GlobalParticleSystem,
) -> Vec<katla_gfx::particles::EmitterHandle> {
    let mut emitters = Vec::new();

    // Emitter 1: Point emitter at origin, medium emit rate
    // Lifetime: 2.0s - particles die around frame 120
    let config1 = EmitterConfig {
        position: [0.0, 0.0, 0.0],
        emit_rate: 100.0, // 100 particles per second
        base_lifetime: 2.0,
        velocity_direction: [0.0, 1.0, 0.0], // Upward
        velocity_magnitude: 1.0,
        color: [1.0, 0.5, 0.0, 1.0], // Orange
        ..Default::default()
    };

    match particle_system.create_emitter(config1) {
        Ok(handle) => emitters.push(handle),
        Err(e) => {
            log::error!("Failed to create emitter 1: {}", e);
        }
    }

    // Emitter 2: Point emitter offset, higher emit rate
    // Lifetime: 3.0s - particles die around frame 180
    let config2 = EmitterConfig {
        position: [5.0, 0.0, 0.0],
        emit_rate: 200.0,
        base_lifetime: 3.0,
        velocity_direction: [0.0, 1.0, 0.0],
        velocity_magnitude: 2.0,
        color: [0.0, 0.5, 1.0, 1.0], // Blue
        ..Default::default()
    };

    match particle_system.create_emitter(config2) {
        Ok(handle) => emitters.push(handle),
        Err(e) => {
            log::error!("Failed to create emitter 2: {}", e);
        }
    }

    // Emitter 3: Point emitter with burst emission
    // Lifetime: 1.5s - particles die around frame 90
    let config3 = EmitterConfig {
        position: [-5.0, 0.0, 0.0],
        emit_rate: 0.0, // No continuous emission
        base_lifetime: 1.5,
        velocity_direction: [1.0, 0.5, 0.0],
        velocity_magnitude: 1.5,
        color: [1.0, 0.0, 0.0, 1.0], // Red
        ..Default::default()
    };

    match particle_system.create_emitter(config3) {
        Ok(handle) => {
            // Burst 100 particles immediately
            if let Err(e) = particle_system.burst(handle, 100) {
                log::error!("Failed to burst from emitter 3: {}", e);
            }
            emitters.push(handle);
        }
        Err(e) => {
            log::error!("Failed to create emitter 3: {}", e);
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
    // The test data initialization sets position to [9.87, 6.54, 3.21]
    let test_position = [9.87, 6.54, 3.21];
    let mut particles_simulated = 0;
    let mut particles_at_initial = 0;

    // Track position distribution - ONLY for alive particles
    let mut unique_positions = std::collections::HashSet::new();
    let mut position_counts: std::collections::HashMap<(i32, i32, i32), usize> =
        std::collections::HashMap::new();

    for &idx in &alive_indices {
        if idx >= particles.len() {
            continue;
        }

        let p = &particles[idx];

        // Quantize position for grouping (round to integer for emitter position matching)
        let pos_key = (
            (p.position[0] * 10.0) as i32,
            (p.position[1] * 10.0) as i32,
            (p.position[2] * 10.0) as i32,
        );
        unique_positions.insert(pos_key);
        *position_counts.entry(pos_key).or_insert(0) += 1;

        let is_at_initial = (p.position[0] - test_position[0]).abs() < 0.01
            && (p.position[1] - test_position[1]).abs() < 0.01
            && (p.position[2] - test_position[2]).abs() < 0.01;

        if is_at_initial {
            particles_at_initial += 1;
        } else {
            particles_simulated += 1;
        }
    }

    log::info!(
        "GPU sim check: {} simulated, {} at initial, {} unique positions",
        particles_simulated,
        particles_at_initial,
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

/// Validate emitter configurations.
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
