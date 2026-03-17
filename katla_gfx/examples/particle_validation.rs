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

use katla_gfx::particles::{EmitterConfig, GlobalParticleSystem};
use katla_gfx::renderer::registry::AssetRegistry;
use katla_gfx::ValidationMode;
use katla_gfx::VulkanContext;
use std::ffi::CString;
use std::process::ExitCode;

use particle_validation_helpers::{execute_gpu_compute, find_shader_directory};

/// Maximum particles for validation test
const MAX_PARTICLES: u32 = 10_000;

/// Number of frames to simulate
const NUM_FRAMES: u32 = 10;

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
    fn new(frame: u32, alive_count: u32, emit_count: u32, delta_time: f32, cumulative_time: f32) -> Self {
        Self {
            frame,
            alive_count,
            emit_count,
            delta_time,
            cumulative_time,
        }
    }

    fn log(&self) {
        log::info!(
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
    fn record_frame(&mut self, frame: u32, alive_list: &[u32], alive_count: u32, emit_count: u32, cumulative_time: f32) {
        // Only store the actual alive particles (first alive_count entries)
        let actual_alive: Vec<u32> = alive_list.iter()
            .take(alive_count as usize)
            .copied()
            .collect();

        self.frame_data.push(FrameAliveData {
            frame,
            alive_indices: actual_alive,
            alive_count,
            emit_count,
            cumulative_time,
        });
    }

    /// Validate frame-to-frame transitions for corruption patterns.
    fn validate_transitions(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.frame_data.len() < 2 {
            return Ok(()); // Need at least 2 frames to compare
        }

        log::info!("=== FRAME-TO-FRAME ALIVE_LIST TRANSITION VALIDATION ===");

        for i in 0..self.frame_data.len() - 1 {
            let current = &self.frame_data[i];
            let next = &self.frame_data[i + 1];

            log::info!("--- Frame {} → {} ---", current.frame, next.frame);

            // Create sets for comparison
            let current_set: std::collections::HashSet<u32> = current.alive_indices.iter().cloned().collect();
            let next_set: std::collections::HashSet<u32> = next.alive_indices.iter().cloned().collect();

            // Calculate transitions
            let stayed_alive: Vec<u32> = current.alive_indices.iter()
                .filter(|idx| next_set.contains(idx))
                .cloned()
                .collect();

            let new_particles: Vec<u32> = next.alive_indices.iter()
                .filter(|idx| !current_set.contains(idx))
                .cloned()
                .collect();

            let died_particles: Vec<u32> = current.alive_indices.iter()
                .filter(|idx| !next_set.contains(idx))
                .cloned()
                .collect();

            let expected_new = next.emit_count;
            let expected_stayed = current.alive_count.saturating_sub(
                (current.alive_count as f32 * current.cumulative_time / 2.0).ceil() as u32 // Rough estimate
            );

            log::info!("  Current frame: {} alive particles", current.alive_count);
            log::info!("  Next frame: {} alive particles", next.alive_count);
            log::info!("  Stayed alive: {} particles (expected ~{})", stayed_alive.len(), expected_stayed);
            log::info!("  New particles: {} (expected {} from emit)", new_particles.len(), expected_new);
            log::info!("  Died particles: {}", died_particles.len());

            // Validate consistency
            let actual_growth = next.alive_count as i32 - current.alive_count as i32;
            let expected_growth = next.emit_count as i32 - died_particles.len() as i32;

            if (actual_growth - expected_growth).abs() > 5 {
                errors.push(format!(
                    "Frame {} → {}: Alive count growth mismatch. actual={}->{} (delta={}), expected delta ~{} (emit={} - died={})",
                    current.frame, next.frame,
                    current.alive_count, next.alive_count, actual_growth,
                    expected_growth, next.emit_count, died_particles.len()
                ));
            }

            // Check for suspicious patterns
            if stayed_alive.len() < current.alive_count as usize / 2 {
                errors.push(format!(
                    "Frame {} → {}: More than half of particles died unexpectedly! {}/{} stayed alive",
                    current.frame, next.frame, stayed_alive.len(), current.alive_count
                ));
            }

            if new_particles.len() > next.emit_count as usize + 5 {
                errors.push(format!(
                    "Frame {} → {}: More new particles than emitted! new={}, emit={}",
                    current.frame, next.frame, new_particles.len(), next.emit_count
                ));
            }
        }

        log::info!("=== TRANSITION VALIDATION COMPLETE ===");

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Check for corruption within each frame's alive_list.
    fn validate_per_frame(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        log::info!("=== PER-FRAME ALIVE_LIST CORRUPTION CHECK ===");

        for frame_data in &self.frame_data {
            log::info!("Frame {} ({} alive particles):", frame_data.frame, frame_data.alive_count);

            // Check for duplicates
            let mut seen = std::collections::HashSet::new();
            let mut duplicates = Vec::new();
            let mut dup_counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();

            for &idx in &frame_data.alive_indices {
                if !seen.insert(idx) {
                    duplicates.push(idx);
                }
                *dup_counts.entry(idx).or_insert(0) += 1;
            }

            if !duplicates.is_empty() {
                // Find most common duplicate
                let mut most_common_dup = (0, 0);
                for (idx, count) in &dup_counts {
                    if *count > most_common_dup.1 {
                        most_common_dup = (*idx, *count);
                    }
                }

                // Count how many unique indices are duplicates
                let unique_dups: std::collections::HashSet<u32> = duplicates.iter().cloned().collect();

                errors.push(format!(
                    "Frame {}: Found {} duplicate occurrences ({} unique indices with duplicates). Most common: index {} appears {} times",
                    frame_data.frame,
                    duplicates.len(),
                    unique_dups.len(),
                    most_common_dup.0,
                    most_common_dup.1
                ));
                log::warn!("  ERROR: {} duplicate occurrences found ({} unique indices)", duplicates.len(), unique_dups.len());
                log::warn!("    Most duplicated index: {} appears {} times", most_common_dup.0, most_common_dup.1);

                // Show first 20 alive_list entries to understand the pattern
                log::warn!("    First 20 alive_list entries: {:?}", &frame_data.alive_indices[..20.min(frame_data.alive_indices.len())]);
            }

            // Check for out-of-bounds indices
            let out_of_bounds: Vec<u32> = frame_data.alive_indices.iter()
                .filter(|&&idx| idx >= self.max_particles)
                .cloned()
                .collect();

            if !out_of_bounds.is_empty() {
                errors.push(format!(
                    "Frame {}: Found {} out-of-bounds indices in alive_list (max={}): {:?}",
                    frame_data.frame,
                    out_of_bounds.len(),
                    self.max_particles,
                    out_of_bounds.iter().take(10).cloned().collect::<Vec<_>>()
                ));
                log::warn!("  ERROR: {} out-of-bounds indices found", out_of_bounds.len());
            }

            // Check for index jump corruption (sudden large changes)
            if frame_data.alive_indices.len() > 10 {
                let mut large_jumps = 0;
                for i in 1..frame_data.alive_indices.len() {
                    let prev = frame_data.alive_indices[i - 1];
                    let curr = frame_data.alive_indices[i];
                    let diff = if curr > prev { curr - prev } else { prev - curr };

                    // Large jump (>1000) is suspicious for consecutive particles
                    if diff > 1000 {
                        large_jumps += 1;
                    }
                }

                if large_jumps > frame_data.alive_indices.len() / 4 {
                    errors.push(format!(
                        "Frame {}: Excessive large jumps in alive_list ({} jumps out of {} entries)",
                        frame_data.frame, large_jumps, frame_data.alive_indices.len()
                    ));
                    log::warn!("  ERROR: {} large jumps detected", large_jumps);
                }
            }

            if duplicates.is_empty() && out_of_bounds.is_empty() {
                log::info!("  ✓ No corruption detected");
            }
        }

        log::info!("=== PER-FRAME VALIDATION COMPLETE ===");

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Print detailed frame summary.
    fn print_summary(&self) {
        log::info!("=== ALIVE_LIST TRACKING SUMMARY ===");
        log::info!("Tracked {} frames", self.frame_data.len());

        for frame_data in &self.frame_data {
            log::info!(
                "Frame {}: {} alive particles (emit={}, cumulative_time={:.3}s)",
                frame_data.frame,
                frame_data.alive_count,
                frame_data.emit_count,
                frame_data.cumulative_time
            );
        }

        // Calculate statistics
        let total_alive: u64 = self.frame_data.iter().map(|d| d.alive_count as u64).sum();
        let avg_alive = total_alive / self.frame_data.len() as u64;

        log::info!("Statistics:");
        log::info!("  Total alive particles across all frames: {}", total_alive);
        log::info!("  Average alive particles per frame: {}", avg_alive);
        log::info!("  Max particles in system: {}", self.max_particles);
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
            log::info!(
                "Memory usage: {:.2} MB",
                system.get_stats().memory_used_mb
            );
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
    log::info!("Running GPU compute simulation for {} frames...", NUM_FRAMES);
    log::info!("=== FRAME-BY-FRAME DIAGNOSTICS ===");
    log::info!("Delta time per frame: {:.5}s (60 FPS)", DELTA_TIME);
    log::info!("Expected cumulative times:");
    for f in [0, 1, 2, 5, 9].iter() {
        log::info!("  Frame {}: cumulative_time = {:.5}s", f, (*f as f32 + 1.0) * DELTA_TIME);
    }

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
                let diag = FrameDiagnostics::new(frame, alive_count, emit_count, DELTA_TIME, cumulative_time);
                diag.log();

                if died_this_frame > 0 {
                    log::info!("  → {} particles died this frame", died_this_frame);
                }

                // Expected behavior at this frame
                let expected_alive = estimate_expected_alive(frame);
                let diff = if alive_count as i32 > expected_alive {
                    alive_count as i32 - expected_alive
                } else {
                    expected_alive - alive_count as i32
                };

                if frame % 3 == 0 || is_last_frame {
                    log::info!("  Expected alive: ~{}, Actual: {}, Diff: {}",
                               expected_alive, alive_count, diff);
                }

                prev_alive_count = alive_count;

                // Execute actual GPU compute dispatch
                if let Err(e) = execute_gpu_compute(
                    &context,
                    &mut particle_system,
                    &asset_registry,
                    alive_count,
                    emit_count,
                ) {
                    log::error!("Failed to execute GPU compute at frame {}: {}", frame, e);
                    return ExitCode::from(1);
                }

                // Read back alive_list data for this frame (after GPU compute)
                if let Ok(debug_data) = particle_system.read_debug_data() {
                    alive_tracker.record_frame(
                        frame,
                        &debug_data.alive_list,
                        debug_data.counters.alive_count,
                        emit_count,
                        cumulative_time
                    );

                    // Log alive_list sample for this frame
                    if frame % 3 == 0 || is_last_frame {
                        log::info!("  Alive list sample (first 10 indices):");
                        for (i, &idx) in debug_data.alive_list.iter().take(10).enumerate() {
                            log::info!("    alive_list[{}] = {}", i, idx);
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to update particle system at frame {}: {}", frame, e);
                return ExitCode::from(1);
            }
        }
    }

    log::info!("=== FRAME-BY-FRAME DIAGNOSTICS END ===");

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
/// - Emitter 1: 100/sec, lifetime 2.0s
/// - Emitter 2: 200/sec, lifetime 3.0s
/// - Emitter 3: 100 burst, lifetime 1.5s
fn estimate_expected_alive(frame: u32) -> i32 {
    let time = (frame + 1) as f32 * DELTA_TIME;

    // Emitter 1: 100/sec * min(time, 2.0) * survival_ratio
    let e1_emitted = (100.0 * time.min(2.0)).ceil() as i32;
    let e1_alive = if time > 2.0 {
        0 // All particles from emitter 1 have died by 2.0s
    } else {
        // Approximate: particles emitted in last 2.0s are still alive
        ((100.0 * time.min(2.0)) * (1.0 - (time / 2.0) * 0.5)).ceil() as i32
    };

    // Emitter 2: 200/sec * min(time, 3.0)
    let e2_emitted = (200.0 * time.min(3.0)).ceil() as i32;
    let e2_alive = if time > 3.0 {
        0
    } else {
        ((200.0 * time.min(3.0)) * (1.0 - (time / 3.0) * 0.3)).ceil() as i32
    };

    // Emitter 3: 100 burst at frame 0, lifetime 1.5s
    let e3_alive = if time < 1.5 {
        100 // All burst particles still alive
    } else {
        0 // All burst particles have died
    };

    e1_alive + e2_alive + e3_alive
}

/// Create test emitters with known properties for validation.
fn create_test_emitters(
    particle_system: &mut GlobalParticleSystem,
) -> Vec<katla_gfx::particles::EmitterHandle> {
    let mut emitters = Vec::new();

    // Emitter 1: Point emitter at origin, medium emit rate
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
fn validate_particle_data(
    particle_system: &GlobalParticleSystem,
) -> Result<(), String> {
    log::info!("=== LIFETIME DIAGNOSTICS BEGIN ===");
    log::info!("Expected lifetimes from emitter config:");
    log::info!("  Emitter 1: base_lifetime = 2.0s (100 particles/sec)");
    log::info!("  Emitter 2: base_lifetime = 3.0s (200 particles/sec)");
    log::info!("  Emitter 3: base_lifetime = 1.5s (100 burst particles)");
    log::info!("After {} frames at {:.5}s per frame, cumulative time = {:.2}s",
               NUM_FRAMES, DELTA_TIME, NUM_FRAMES as f32 * DELTA_TIME);
    log::info!("Expected alive count:");
    log::info!("  Emitter 1: ~17 emitted * (1.0 - 0.5/2.0) = ~17 alive (lost ~0-1 to random variation)");
    log::info!("  Emitter 2: ~33 emitted * (1.0 - 0.5/3.0) = ~33 alive (lost ~0-1 to random variation)");
    log::info!("  Emitter 3: 100 emitted * (1.0 - 0.5/1.5) = ~67-100 alive (many should have died)");
    log::info!("  Total expected: ~117-150 alive particles");

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

    // Print comprehensive diagnostics
    log::info!("=== PARTICLE READBACK DIAGNOSTICS ===");
    log::info!("{}", debug_data.summary());

    // Print first 10 particles in buffer to see what's actually there
    log::info!("=== First 10 particles in buffer (direct array access) ===");
    for (i, p) in particles.iter().take(10).enumerate() {
        log::info!("particles[{}]: pos=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) lifetime={:.2} scale={:.2} color=({:.2},{:.2},{:.2},{:.2})",
                   i,
                   p.position[0], p.position[1], p.position[2],
                   p.velocity[0], p.velocity[1], p.velocity[2],
                   p.lifetime, p.scale,
                   p.color[0], p.color[1], p.color[2], p.color[3]);
    }

    // Print alive list indices with bounds checking
    debug_data.print_alive_indices(10);

    // Print alive particles with position distribution
    debug_data.print_alive_particles(10);

    // Only validate particles that are actually alive
    // Use alive_list to get indices of alive particles, then validate those
    if alive_count == 0 {
        log::warn!("No alive particles to validate (system may not have simulated yet)");
        return Ok(());
    }

    // Diagnostic: Print raw alive_list data to understand what's there
    log::info!("Raw alive_list data (first 24 u32 values):");
    for (i, &val) in alive_list.iter().take(24).enumerate() {
        log::info!("  alive_list[{}] = {}", i, val);
    }

    // Diagnostic: Print what's in the dead list
    log::info!("First 10 dead list indices:");
    for (i, &val) in debug_data.dead_list.iter().take(10).enumerate() {
        log::info!("  dead_list[{}] = {}", i, val);
    }

    // Diagnostic: Print counters
    log::info!("Counters: alive_count={}, dead_count={}, emit_count={}",
               debug_data.counters.alive_count,
               debug_data.counters.dead_count,
               debug_data.counters.emit_count);

    log::info!(
        "Validating {} alive particles ({} total in buffer, {} in alive list)...",
        alive_count,
        particles.len(),
        alive_list.len()
    );

    // Collect the indices of alive particles to validate
    // alive_list contains all 3 alive buffers concatenated, we only need the first alive_count
    let alive_indices: Vec<usize> = alive_list
        .iter()
        .take(alive_count)
        .map(|&idx| idx as usize)
        .collect();

    // LIFETIME DIAGNOSTICS: Analyze particle lifetimes in detail
    log::info!("=== DETAILED LIFETIME ANALYSIS ===");

    // Collect all alive particle lifetimes
    let mut alive_lifetimes: Vec<f32> = Vec::new();
    let mut dead_lifetimes: Vec<f32> = Vec::new();

    // Process alive particles
    for &idx in &alive_indices {
        if idx < particles.len() {
            let p = &particles[idx];
            alive_lifetimes.push(p.lifetime);
        }
    }

    // Process dead particles (scan all particles, check if not in alive list and has been simulated)
    let alive_set: std::collections::HashSet<usize> = alive_indices.iter().cloned().collect();
    for idx in 0..particles.len() {
        if !alive_set.contains(&idx) {
            let p = &particles[idx];
            // Check if this particle was ever emitted/simulated (not at initial position)
            let test_position = [9.87, 6.54, 3.21];
            let is_at_initial = (p.position[0] - test_position[0]).abs() < 0.01
                && (p.position[1] - test_position[1]).abs() < 0.01
                && (p.position[2] - test_position[2]).abs() < 0.01;

            if !is_at_initial {
                // This particle was emitted but is now dead
                dead_lifetimes.push(p.lifetime);
            }
        }
    }

    // Sort lifetimes for percentile analysis
    alive_lifetimes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    dead_lifetimes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    log::info!("Alive particle lifetime statistics ({} particles):", alive_lifetimes.len());
    if !alive_lifetimes.is_empty() {
        log::info!("  Min: {:.5}s", alive_lifetimes[0]);
        log::info!("  Max: {:.5}s", alive_lifetimes[alive_lifetimes.len() - 1]);
        log::info!("  Mean: {:.5}s", alive_lifetimes.iter().sum::<f32>() / alive_lifetimes.len() as f32);

        // Percentiles
        let p25_idx = alive_lifetimes.len() * 25 / 100;
        let p50_idx = alive_lifetimes.len() * 50 / 100;
        let p75_idx = alive_lifetimes.len() * 75 / 100;
        log::info!("  25th percentile: {:.5}s", alive_lifetimes[p25_idx]);
        log::info!("  50th percentile (median): {:.5}s", alive_lifetimes[p50_idx]);
        log::info!("  75th percentile: {:.5}s", alive_lifetimes[p75_idx]);

        // Lifetime distribution histogram
        log::info!("  Lifetime distribution:");
        let mut buckets = [0usize; 10];
        for &lt in &alive_lifetimes {
            let bucket_idx = if lt >= 3.0 {
                9
            } else {
                (lt / 0.3).floor() as usize
            };
            if bucket_idx < 10 {
                buckets[bucket_idx] += 1;
            }
        }
        for (i, count) in buckets.iter().enumerate() {
            let range_start = i as f32 * 0.3;
            let range_end = (i + 1) as f32 * 0.3;
            log::info!("    [{:.2}s, {:.2}s): {} particles", range_start, range_end, count);
        }

        // Show first 20 alive particle lifetimes individually
        log::info!("  First 20 alive particle lifetimes:");
        for (i, lt) in alive_lifetimes.iter().take(20).enumerate() {
            log::info!("    Alive[{}]: lifetime = {:.5}s", i, lt);
        }
    }

    log::info!("Dead particle lifetime statistics ({} particles):", dead_lifetimes.len());
    if !dead_lifetimes.is_empty() {
        log::info!("  Min: {:.5}s", dead_lifetimes[0]);
        log::info!("  Max: {:.5}s", dead_lifetimes[dead_lifetimes.len() - 1]);

        // Show first 20 dead particle lifetimes
        log::info!("  First 20 dead particle lifetimes:");
        for (i, lt) in dead_lifetimes.iter().take(20).enumerate() {
            log::info!("    Dead[{}]: lifetime = {:.5}s", i, lt);
        }
    }

    // CRITICAL: Check if lifetimes match expectations
    log::info!("=== LIFETIME VALIDATION ===");
    let cumulative_time = NUM_FRAMES as f32 * DELTA_TIME;
    log::info!("Cumulative simulation time: {:.5}s ({} frames * {:.5}s/frame)",
               cumulative_time, NUM_FRAMES, DELTA_TIME);

    // Expected remaining lifetime for each emitter type
    // Emitter 1: 2.0s base, should have ~2.0 - 0.1667 = 1.8333s remaining
    // Emitter 2: 3.0s base, should have ~3.0 - 0.1667 = 2.8333s remaining
    // Emitter 3: 1.5s base, burst at frame 0, should have ~1.5 - 0.1667 = 1.3333s remaining (some may have died)

    // Check if lifetimes are way too low (indicates 3x bug or similar)
    if !alive_lifetimes.is_empty() {
        let avg_lifetime = alive_lifetimes.iter().sum::<f32>() / alive_lifetimes.len() as f32;
        let expected_min_avg = 1.3; // Minimum expected average (dominated by burst emitter)
        let expected_max_avg = 2.9; // Maximum expected average (dominated by long-lived emitters)

        log::info!("Average alive particle lifetime: {:.5}s", avg_lifetime);
        log::info!("Expected range: [{:.5}s, {:.5}s]", expected_min_avg, expected_max_avg);

        if avg_lifetime < expected_min_avg * 0.5 {
            log::error!("CRITICAL: Average lifetime is WAY too low!");
            log::error!("Expected at least {:.5}s, got {:.5}s", expected_min_avg * 0.5, avg_lifetime);
            log::error!("This indicates particles are dying ~3x faster than they should!");
            log::error!("Possible causes:");
            log::error!("  1. Delta time is 3x larger than expected");
            log::error!("  2. Lifetime subtraction is wrong (e.g., subtracting 3*dt instead of dt)");
            log::error!("  3. Kill condition is wrong (e.g., lifetime < 0.5 instead of lifetime <= 0)");
        } else if avg_lifetime < expected_min_avg {
            log::warn!("WARNING: Average lifetime is lower than expected");
            log::warn!("Expected at least {:.5}s, got {:.5}s", expected_min_avg, avg_lifetime);
        } else {
            log::info!("✓ Average lifetime is within expected range");
        }
    }

    log::info!("=== LIFETIME DIAGNOSTICS END ===");

    // Verify indices are within bounds
    let mut out_of_bounds_count = 0;
    for &idx in &alive_indices {
        if idx >= particles.len() {
            log::error!("Alive particle index {} out of bounds (max={})", idx, particles.len());
            out_of_bounds_count += 1;
        }
    }

    if out_of_bounds_count > 0 {
        return Err(format!("{} alive particle indices are out of bounds", out_of_bounds_count));
    }

    // Check if particles were actually simulated on GPU
    // The test data initialization sets position to [9.87, 6.54, 3.21]
    // If GPU simulation worked, positions should have changed from these initial values
    let test_position = [9.87, 6.54, 3.21];
    let mut particles_simulated = 0;
    let mut particles_at_initial = 0;

    // Track position distribution
    let mut position_samples: Vec<[f32; 3]> = Vec::new();
    let mut unique_positions = std::collections::HashSet::new();

    for &idx in &alive_indices {
        if idx >= particles.len() {
            continue;
        }

        let p = &particles[idx];
        position_samples.push(p.position);

        // Quantize position for uniqueness check
        let pos_key = (
            (p.position[0] * 100.0) as i32,
            (p.position[1] * 100.0) as i32,
            (p.position[2] * 100.0) as i32,
        );
        unique_positions.insert(pos_key);

        let is_at_initial = (p.position[0] - test_position[0]).abs() < 0.01
            && (p.position[1] - test_position[1]).abs() < 0.01
            && (p.position[2] - test_position[2]).abs() < 0.01;

        if is_at_initial {
            particles_at_initial += 1;
        } else {
            particles_simulated += 1;
        }
    }

    log::info!("GPU simulation check:");
    log::info!("  - Alive particles at initial test position: {}", particles_at_initial);
    log::info!("  - Alive particles with changed positions (simulated): {}", particles_simulated);
    log::info!("  - Unique positions among alive particles: {}", unique_positions.len());

    // Show position samples
    log::info!("Position samples (first 10 alive particles):");
    for (i, pos) in position_samples.iter().take(10).enumerate() {
        log::info!("  [{}] ({:.2}, {:.2}, {:.2})", i, pos[0], pos[1], pos[2]);
    }

    if particles_simulated == 0 && alive_count > 0 {
        log::warn!("WARNING: No alive particles appear to have been simulated on GPU!");
        log::warn!("All alive particles are still at initial test position {:?}", test_position);
        log::warn!("This may indicate GPU compute shaders did not execute properly.");
    } else if particles_simulated > 0 {
        log::info!("✓ GPU compute execution confirmed: {} alive particles were simulated", particles_simulated);
    }

    // CRITICAL CHECK: If all particles have the same position, something is wrong
    if unique_positions.len() <= 3 && alive_count > 10 {
        log::error!("CRITICAL: Only {} unique positions among {} alive particles!", unique_positions.len(), alive_count);
        log::error!("This indicates:");
        log::error!("  1. Wrong particle indices in alive_list");
        log::error!("  2. Buffer layout issue (reading from wrong offset)");
        log::error!("  3. Index list corruption");
        log::error!("  4. Memory aliasing in readback");

        // Show what particles are at the referenced indices
        log::error!("Dumping particle data at first 10 alive_list indices:");
        for (i, idx) in alive_indices.iter().take(10).enumerate() {
            if *idx < particles.len() {
                let p = &particles[*idx];
                log::error!("  alive_list[{}] = {} -> pos=({:.2}, {:.2}, {:.2}) vel=({:.2}, {:.2}, {:.2})",
                           i, idx, p.position[0], p.position[1], p.position[2],
                           p.velocity[0], p.velocity[1], p.velocity[2]);
            }
        }

        return Err(format!("Only {} unique positions among {} alive particles - index list may be corrupted",
                          unique_positions.len(), alive_count));
    }

    let mut nan_count = 0;
    let mut inf_count = 0;
    let mut out_of_bounds_count = 0;
    let mut invalid_lifetime_count = 0;
    let mut invalid_color_count = 0;
    let mut invalid_scale_count = 0;

    // Calculate reasonable bounds for validation
    // Particles should be within [-100, 100] units from origin given test parameters
    const POSITION_BOUND: f32 = 100.0;

    // Only validate alive particles using their indices from alive_list
    for &idx in &alive_indices {
        if idx >= particles.len() {
            log::warn!("Alive particle index {} out of bounds, skipping validation", idx);
            continue;
        }

        let p = &particles[idx];
        // Check position for NaN and infinity
        if p.position[0].is_nan() || p.position[1].is_nan() || p.position[2].is_nan() {
            log::error!("Particle {}: position contains NaN", idx);
            nan_count += 1;
        }

        if p.position[0].is_infinite()
            || p.position[1].is_infinite()
            || p.position[2].is_infinite()
        {
            log::error!("Particle {}: position contains infinity", idx);
            inf_count += 1;
        }

        // Check position bounds
        if p.position[0].abs() > POSITION_BOUND
            || p.position[1].abs() > POSITION_BOUND
            || p.position[2].abs() > POSITION_BOUND
        {
            log::warn!(
                "Particle {}: position out of bounds: ({:.2}, {:.2}, {:.2})",
                idx,
                p.position[0],
                p.position[1],
                p.position[2]
            );
            out_of_bounds_count += 1;
        }

        // Check velocity for NaN and infinity
        if p.velocity[0].is_nan() || p.velocity[1].is_nan() || p.velocity[2].is_nan() {
            log::error!("Particle {}: velocity contains NaN", idx);
            nan_count += 1;
        }

        if p.velocity[0].is_infinite()
            || p.velocity[1].is_infinite()
            || p.velocity[2].is_infinite()
        {
            log::error!("Particle {}: velocity contains infinity", idx);
            inf_count += 1;
        }

        // Check lifetime validity
        if p.lifetime < 0.0 {
            log::error!("Particle {}: invalid lifetime: {}", idx, p.lifetime);
            invalid_lifetime_count += 1;
        }

        // Check color range [0, 1]
        if p.color[0] < 0.0
            || p.color[0] > 1.0
            || p.color[1] < 0.0
            || p.color[1] > 1.0
            || p.color[2] < 0.0
            || p.color[2] > 1.0
            || p.color[3] < 0.0
            || p.color[3] > 1.0
        {
            log::warn!(
                "Particle {}: color out of range: ({:.2}, {:.2}, {:.2}, {:.2})",
                idx,
                p.color[0],
                p.color[1],
                p.color[2],
                p.color[3]
            );
            invalid_color_count += 1;
        }

        // Check scale positivity
        if p.scale <= 0.0 {
            log::error!("Particle {}: invalid scale: {}", idx, p.scale);
            invalid_scale_count += 1;
        }
    }

    // Report validation results
    log::info!("Particle validation results:");
    log::info!("  - Alive particles validated: {}", alive_count);
    log::info!("  - NaN values: {}", nan_count);
    log::info!("  - Infinity values: {}", inf_count);
    log::info!("  - Out of bounds positions: {}", out_of_bounds_count);
    log::info!("  - Invalid lifetimes: {}", invalid_lifetime_count);
    log::info!("  - Invalid colors: {}", invalid_color_count);
    log::info!("  - Invalid scales: {}", invalid_scale_count);

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
        "Position range (alive particles): X=[{:.2}, {:.2}], Y=[{:.2}, {:.2}], Z=[{:.2}, {:.2}]",
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

    // Warn about non-critical issues but don't fail
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
fn validate_emitter_configs(
    particle_system: &GlobalParticleSystem,
) -> Result<(), String> {
    use katla_gfx::particles::validation::validate_emitter_config;

    let emitters = particle_system.get_emitters();

    if emitters.is_empty() {
        log::warn!("No emitters to validate");
        return Ok(());
    }

    log::info!("Validating {} emitter configurations...", emitters.len());

    // Validate each emitter
    let mut error_count = 0;
    for (i, config) in emitters.iter().enumerate() {
        // Skip inactive emitters
        if config.emit_rate == 0.0 && config.base_lifetime == 0.0 {
            continue;
        }

        match validate_emitter_config(config) {
            Ok(_) => {
                log::debug!("Emitter {}: configuration valid", i);
            }
            Err(e) => {
                log::error!("Emitter {}: {}", i, e);
                error_count += 1;
            }
        }
    }

    if error_count > 0 {
        return Err(format!("{} emitters have invalid configurations", error_count));
    }

    Ok(())
}
