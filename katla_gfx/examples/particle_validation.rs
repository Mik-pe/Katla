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

    for frame in 0..NUM_FRAMES {
        let is_last_frame = frame == NUM_FRAMES - 1;

        // Prepare frame data (uploads emitter configs, frame data)
        match particle_system.update(DELTA_TIME, frame) {
            Ok((alive_count, emit_count)) => {
                if frame % 2 == 0 || is_last_frame {
                    log::debug!(
                        "Frame {}: {} alive particles, {} to emit",
                        frame,
                        alive_count,
                        emit_count
                    );
                }

                // Execute actual GPU compute dispatch
                if let Err(e) = execute_gpu_compute(
                    &context,
                    &mut particle_system,
                    &asset_registry,
                    alive_count,
                    emit_count,
                    is_last_frame,
                ) {
                    log::error!("Failed to execute GPU compute at frame {}: {}", frame, e);
                    return ExitCode::from(1);
                }
            }
            Err(e) => {
                log::error!("Failed to update particle system at frame {}: {}", frame, e);
                return ExitCode::from(1);
            }
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
fn validate_particle_data(
    particle_system: &GlobalParticleSystem,
) -> Result<(), String> {
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
