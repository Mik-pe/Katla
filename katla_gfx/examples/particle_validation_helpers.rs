//! Helper functions for particle validation example.
//!
//! This module contains GPU compute execution helpers for the particle validation example.

use ash::vk;
use katla_gfx::VulkanContext;
use katla_gfx::particles::{
    GlobalParticleSystem, PARTICLE_EMIT_WORKGROUP_SIZE, PARTICLE_SIMULATE_WORKGROUP_SIZE,
};
use katla_gfx::renderer::registry::AssetRegistry;
use std::path::PathBuf;

/// Execute actual GPU compute dispatch for particle emit and simulate.
pub fn execute_gpu_compute(
    context: &VulkanContext,
    particle_system: &mut GlobalParticleSystem,
    asset_registry: &AssetRegistry,
    frame: u32,
    alive_count: u32,
    emit_count: u32,
) -> Result<(), String> {
    // Calculate workgroup counts based on particle counts
    let emit_workgroups = if emit_count > 0 {
        (emit_count + PARTICLE_EMIT_WORKGROUP_SIZE - 1) / PARTICLE_EMIT_WORKGROUP_SIZE
    } else {
        0
    };

    // Simulate processes ALL particles: old survivors + newly emitted.
    // After emit, alive_current[frame] contains [survivors..alive_count-1] + [emitted..alive_count+emit_count-1].
    let total_to_simulate = alive_count + emit_count;
    let simulate_workgroups = if total_to_simulate > 0 {
        (total_to_simulate + PARTICLE_SIMULATE_WORKGROUP_SIZE - 1)
            / PARTICLE_SIMULATE_WORKGROUP_SIZE
    } else {
        0
    };

    if emit_workgroups == 0 && simulate_workgroups == 0 {
        log::debug!("No particles to emit or simulate, skipping GPU dispatch");
        return Ok(());
    }

    // Create command buffer for compute operations
    let command_buffer = context.begin_single_time_commands();

    // CRITICAL: Update compute descriptor bindings for each dispatch
    // The validation runs frames 0-9, so we need to use frame_index % 2 for double-buffering
    let frame_index_for_descriptor = (frame as usize) % 2; // Use actual frame index for proper alternation

    // Record emit dispatch if we have particles to emit
    if emit_workgroups > 0 {
        // Update descriptor binding for emit dispatch
        if let Err(e) =
            particle_system.update_compute_descriptor_binding_for_emit(frame_index_for_descriptor)
        {
            log::warn!(
                "Frame {}: Failed to update compute descriptor binding for emit: {}",
                frame,
                e
            );
        }

        log::debug!(
            "Recording emit dispatch: {} workgroups ({} particles)",
            emit_workgroups,
            emit_count
        );
        match particle_system.record_emit_dispatch(
            command_buffer.vk_command_buffer(),
            asset_registry,
            emit_workgroups,
        ) {
            Ok(_) => {
                log::debug!("Emit dispatch recorded successfully");
            }
            Err(e) => {
                log::warn!(
                    "Failed to record emit dispatch (pipelines may not be loaded): {}",
                    e
                );
                // Continue anyway - we'll validate CPU-side structures
            }
        }
    }

    // Record simulate dispatch if we have alive particles
    if simulate_workgroups > 0 {
        // Update descriptor binding for simulate dispatch
        if let Err(e) = particle_system
            .update_compute_descriptor_binding_for_simulate(frame_index_for_descriptor)
        {
            log::warn!(
                "Frame {}: Failed to update compute descriptor binding for simulate: {}",
                frame,
                e
            );
        }

        log::debug!(
            "Recording simulate dispatch: {} workgroups ({} particles)",
            simulate_workgroups,
            alive_count
        );
        match particle_system.record_simulate_dispatch(
            command_buffer.vk_command_buffer(),
            asset_registry,
            simulate_workgroups,
        ) {
            Ok(_) => {
                log::debug!("Simulate dispatch recorded successfully");
            }
            Err(e) => {
                log::warn!(
                    "Failed to record simulate dispatch (pipelines may not be loaded): {}",
                    e
                );
                // Continue anyway - we'll validate CPU-side structures
            }
        }

        // Record debug readback BEFORE swap to capture alive_next
        match particle_system.record_debug_readback(command_buffer.vk_command_buffer()) {
            Ok(_) => {
                log::debug!("Debug readback recorded successfully");
            }
            Err(e) => {
                log::warn!("Failed to record debug readback: {}", e);
            }
        }

        // CRITICAL: Swap alive lists after simulate completes
        let next_frame_index = (frame_index_for_descriptor + 1) % 2;
        match particle_system.swap_alive_lists(command_buffer.vk_command_buffer(), next_frame_index)
        {
            Ok(_) => {
                log::debug!("Frame {}: Alive lists swapped successfully", frame);
            }
            Err(e) => {
                log::warn!("Frame {}: Failed to swap alive lists: {}", frame, e);
            }
        }
    }

    // End command buffer and submit to GPU
    command_buffer.end_single_time_command();

    // Submit and wait for GPU completion
    context.end_single_time_commands(command_buffer);

    Ok(())
}

/// Load particle compute shaders and create pipelines.
pub fn load_and_create_pipelines(
    context: &VulkanContext,
    particle_system: &mut GlobalParticleSystem,
    asset_registry: &mut AssetRegistry,
    shader_dir: &PathBuf,
) -> Result<(), String> {
    use katla_gfx::ShaderCache;
    use katla_gfx::sync::VkShaderModule;

    let mut shader_cache = ShaderCache::new(context.device.clone());

    // Load emit shader
    let emit_shader_path = shader_dir.join("particles/particle_emit.wgsl");
    log::info!("Loading emit shader from: {:?}", emit_shader_path);

    let emit_shader = shader_cache
        .load_shader(&emit_shader_path, vk::ShaderStageFlags::COMPUTE)
        .map_err(|e| format!("Failed to load emit shader: {}", e))?;

    let emit_shader_wrapper = VkShaderModule(emit_shader);
    particle_system
        .create_emit_pipeline(asset_registry, emit_shader_wrapper)
        .map_err(|e| format!("Failed to create emit pipeline: {}", e))?;

    log::info!("Emit pipeline created successfully");

    // Load simulate shader
    let simulate_shader_path = shader_dir.join("particles/particle_simulate.wgsl");
    log::info!("Loading simulate shader from: {:?}", simulate_shader_path);

    let simulate_shader = shader_cache
        .load_shader(&simulate_shader_path, vk::ShaderStageFlags::COMPUTE)
        .map_err(|e| format!("Failed to load simulate shader: {}", e))?;

    let simulate_shader_wrapper = VkShaderModule(simulate_shader);
    particle_system
        .create_simulate_pipeline(asset_registry, simulate_shader_wrapper)
        .map_err(|e| format!("Failed to create simulate pipeline: {}", e))?;

    log::info!("Simulate pipeline created successfully");

    Ok(())
}

/// Find the shader directory by checking common locations.
pub fn find_shader_directory() -> PathBuf {
    let possible_paths = vec![
        PathBuf::from("resources/shaders"),
        PathBuf::from("../resources/shaders"),
        PathBuf::from("../../resources/shaders"),
        PathBuf::from("../../../resources/shaders"),
    ];

    for path in possible_paths {
        if path.exists() {
            log::info!("Found shader directory at: {:?}", path);
            return path;
        }
    }

    // Default to resources/shaders even if it doesn't exist
    // The error will be clearer when shader loading fails
    log::warn!("Could not find shader directory, defaulting to resources/shaders");
    PathBuf::from("resources/shaders")
}
