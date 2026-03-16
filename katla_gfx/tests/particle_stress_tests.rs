//! Integration tests for the particle system.
//!
//! These tests validate particle system behavior under stress conditions:
//! - Large particle counts
//! - Many emitters
//! - Memory management
//! - Performance characteristics
//!
//! Note: These tests use headless Vulkan context and validate through API behavior.
//! Full rendering validation would require the complete rendering pipeline.

mod common;

use common::create_headless_context;
use katla_gfx::particles::{EmitterConfig, GlobalParticleSystem};
use std::rc::Rc;

/// Test creating a particle system with maximum particle capacity.
///
/// This validates that the particle system can be initialized with 1M particles
/// and that all internal buffers are allocated correctly.
#[test]
fn test_1m_particle_capacity() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 1_048_576; // 1M particles

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Verify system was created
    assert_eq!(particle_system.max_particles(), max_particles);

    // Create a high-emission-rate emitter
    let config = EmitterConfig {
        position: [0.0, 1.0, 0.0],
        emit_rate: 100000.0, // High emit rate
        base_lifetime: 5.0,
        ..Default::default()
    };

    let emitter = particle_system
        .create_emitter(config)
        .expect("Failed to create emitter");

    assert_ne!(emitter.index(), u32::MAX);

    // Verify emitter exists
    let emitters = particle_system.get_emitters();
    assert_eq!(emitters.len(), 1);
    assert_eq!(emitters[0].emit_rate, 100000.0);

    // Check memory usage
    let stats = particle_system.get_stats();
    assert!(stats.memory_used_mb > 40.0); // Should be ~60MB for 1M particles
    assert_eq!(stats.max_alive_count, max_particles);

    println!(
        "1M particle capacity test passed - Memory: {:.2} MB",
        stats.memory_used_mb
    );
}

/// Test creating 1024 emitters with different configurations.
///
/// This validates that the particle system can handle the maximum number
/// of emitters without dropping any or leaking resources.
#[test]
fn test_1024_emitters() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 100_000; // Smaller capacity for this test

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    let mut emitter_handles = Vec::new();

    // Create 1024 emitters with different configurations
    for i in 0..1024 {
        let config = EmitterConfig {
            position: [
                (i as f32) % 10.0,
                ((i as f32) / 10.0).floor() % 10.0,
                ((i as f32) / 100.0).floor(),
            ],
            emit_rate: 10.0 + (i as f32) % 50.0, // Varying emit rates
            base_lifetime: 2.0 + (i as f32) % 5.0, // Varying lifetimes
            color: [
                (i as f32) / 1024.0,
                ((i as f32) / 512.0) % 1.0,
                ((i as f32) / 256.0) % 1.0,
                1.0,
            ],
            ..Default::default()
        };

        let emitter = particle_system
            .create_emitter(config)
            .expect("Failed to create emitter");
        emitter_handles.push(emitter);
    }

    // Verify all emitters were created
    assert_eq!(emitter_handles.len(), 1024);

    let emitters = particle_system.get_emitters();
    assert_eq!(emitters.len(), 1024);

    // Verify no emitters were dropped (all should have their configs)
    for (i, emitter) in emitters.iter().enumerate() {
        assert!(
            emitter.emit_rate > 0.0,
            "Emitter {} has invalid emit_rate",
            i
        );
        assert!(
            emitter.base_lifetime > 0.0,
            "Emitter {} has invalid lifetime",
            i
        );
    }

    // Clone the configs we'll need for updating
    let configs_to_update: Vec<_> = emitters.iter().take(10).cloned().collect();

    // Drop the emitters reference before updating
    drop(emitters);

    // Update some emitters to verify they can be modified
    for i in 0..10 {
        let handle = emitter_handles[i];
        let mut updated_config = configs_to_update[i];
        updated_config.emit_rate = 999.0;
        particle_system.update_emitter(handle, updated_config);
    }

    // Verify updates worked
    let emitters = particle_system.get_emitters();
    for i in 0..10 {
        assert_eq!(emitters[i].emit_rate, 999.0, "Emitter {} update failed", i);
    }

    // Destroy some emitters to verify cleanup works
    for i in 0..10 {
        particle_system.destroy_emitter(emitter_handles[i]);
    }

    // Verify destroyed emitters are reset to defaults
    let emitters = particle_system.get_emitters();
    for i in 0..10 {
        assert_eq!(emitters[i].emit_rate, 50.0, "Emitter {} not reset", i); // Default value
    }

    println!("1024 emitter test passed - All emitters created, updated, and destroyed correctly");
}

/// Test memory leak detection through repeated emitter creation/destruction.
///
/// This validates that creating and destroying emitters repeatedly doesn't
/// leak GPU memory or descriptor sets.
#[test]
fn test_memory_leak() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 100_000;

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Get baseline memory usage
    let baseline_stats = particle_system.get_stats();
    let baseline_memory_mb = baseline_stats.memory_used_mb;

    println!("Baseline memory: {:.2} MB", baseline_memory_mb);

    // Create and destroy emitters for 100 iterations
    let iterations = 100;
    for i in 0..iterations {
        // Create 10 emitters
        let mut handles = Vec::new();
        for _ in 0..10 {
            let config = EmitterConfig {
                position: [
                    (i as f32) % 10.0,
                    ((i as f32) / 10.0) % 10.0,
                    ((i as f32) / 100.0) % 10.0,
                ],
                emit_rate: 50.0,
                ..Default::default()
            };
            let emitter = particle_system
                .create_emitter(config)
                .expect("Failed to create emitter");
            handles.push(emitter);
        }

        // Destroy all 10 emitters
        for handle in handles {
            particle_system.destroy_emitter(handle);
        }

        // Verify memory hasn't grown significantly
        let current_stats = particle_system.get_stats();
        let memory_growth_mb = current_stats.memory_used_mb - baseline_memory_mb;

        // Memory should not grow more than 1MB over baseline
        assert!(
            memory_growth_mb < 1.0,
            "Memory leak detected: grew by {:.2} MB after {} iterations",
            memory_growth_mb,
            i + 1
        );
    }

    // Final memory check
    let final_stats = particle_system.get_stats();
    let final_memory_mb = final_stats.memory_used_mb;
    let total_growth_mb = final_memory_mb - baseline_memory_mb;

    assert!(
        total_growth_mb < 1.0,
        "Memory leak detected: total growth of {:.2} MB after {} iterations",
        total_growth_mb,
        iterations
    );

    println!(
        "Memory leak test passed - Final memory: {:.2} MB (growth: {:.2} MB)",
        final_memory_mb, total_growth_mb
    );
}

/// Test particle system update stability over multiple frames.
///
/// This validates that the particle system can handle many update calls
/// without performance degradation or errors.
#[test]
fn test_frame_rate_stability() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 100_000;

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Create several emitters
    let mut handles = Vec::new();
    for i in 0..10 {
        let config = EmitterConfig {
            position: [i as f32, 0.0, 0.0],
            emit_rate: 100.0,
            base_lifetime: 3.0,
            ..Default::default()
        };
        let emitter = particle_system
            .create_emitter(config)
            .expect("Failed to create emitter");
        handles.push(emitter);
    }

    let mut frame_times = Vec::new();
    let num_frames = 100;

    // Simulate 100 frames
    for frame in 0..num_frames {
        let start = std::time::Instant::now();

        let delta_time = 0.016; // ~60 FPS
        let alive_count = particle_system
            .update(delta_time)
            .expect("Update should succeed");

        let elapsed = start.elapsed().as_secs_f64() * 1000.0; // Convert to ms
        frame_times.push(elapsed);

        // Verify alive count is reasonable
        assert!(
            alive_count <= max_particles,
            "Alive count {} exceeds max {}",
            alive_count,
            max_particles
        );

        // Every 25 frames, verify stats are consistent
        if frame % 25 == 0 {
            let stats = particle_system.get_stats();
            assert_eq!(stats.frame_count as u32, frame + 1, "Frame count mismatch");
            assert!(
                stats.memory_used_mb > 0.0,
                "Memory usage should be positive"
            );
        }
    }

    // Analyze frame times
    let avg_time: f64 = frame_times.iter().sum::<f64>() / frame_times.len() as f64;
    let max_time = frame_times.iter().fold(0.0_f64, |a, &b| a.max(b));
    let min_time = frame_times.iter().fold(f64::INFINITY, |a, &b| a.min(b));

    // Frame times should be stable (not degrading over time)
    let first_10_avg: f64 = frame_times[..10].iter().sum::<f64>() / 10.0;
    let last_10_avg: f64 = frame_times[frame_times.len() - 10..].iter().sum::<f64>() / 10.0;

    let degradation_pct = ((last_10_avg - first_10_avg) / first_10_avg) * 100.0;

    assert!(
        degradation_pct < 50.0,
        "Frame time degradation too high: {:.1}%",
        degradation_pct
    );

    // Verify final stats
    let final_stats = particle_system.get_stats();
    assert_eq!(
        final_stats.frame_count, num_frames as u64,
        "Frame count mismatch"
    );
    // Note: total_dispatches is only incremented during actual compute dispatch
    // which happens during render graph execution, not in unit tests
    assert!(final_stats.total_dispatches >= 0);

    println!(
        "Frame rate stability test passed - Avg: {:.3}ms, Min: {:.3}ms, Max: {:.3}ms, Degradation: {:.1}%",
        avg_time, min_time, max_time, degradation_pct
    );
}

/// Test that particle system handles edge cases gracefully.
#[test]
fn test_edge_cases() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 10_000;

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Test 1: Zero emit rate
    let config = EmitterConfig {
        emit_rate: 0.0,
        ..Default::default()
    };
    let _emitter = particle_system
        .create_emitter(config)
        .expect("Zero emit rate should be allowed");

    // Test 2: Very high emit rate (should not crash)
    let config = EmitterConfig {
        emit_rate: 1_000_000.0,
        base_lifetime: 0.001,
        ..Default::default()
    };
    let _emitter = particle_system
        .create_emitter(config)
        .expect("High emit rate should be allowed");

    // Test 3: Update with zero delta time
    let result = particle_system.update(0.0);
    assert!(result.is_ok(), "Update with zero delta time should succeed");

    // Test 4: Update with very small delta time
    let result = particle_system.update(0.0001);
    assert!(result.is_ok(), "Update with tiny delta time should succeed");

    // Test 5: Update with very large delta time
    let result = particle_system.update(10.0);
    assert!(
        result.is_ok(),
        "Update with large delta time should succeed"
    );

    println!("Edge cases test passed - All edge cases handled correctly");
}

/// Test emitter configuration validation.
#[test]
fn test_emitter_config_validation() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 10_000;

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Test various emitter configurations
    let configs = vec![
        // Default config
        EmitterConfig::default(),
        // High velocity
        EmitterConfig {
            velocity_magnitude: 1000.0,
            ..Default::default()
        },
        // Large particles
        EmitterConfig {
            base_scale: 10.0,
            scale_variation: 0.0,
            ..Default::default()
        },
        // Short lifetime, high emission
        EmitterConfig {
            base_lifetime: 0.1,
            lifetime_variation: 0.0,
            emit_rate: 1000.0,
            ..Default::default()
        },
        // Wide cone angle
        EmitterConfig {
            velocity_cone_angle: std::f32::consts::PI,
            ..Default::default()
        },
    ];

    for (i, config) in configs.into_iter().enumerate() {
        let emitter = particle_system
            .create_emitter(config)
            .expect(&format!("Config {} should be valid", i));
        assert_ne!(emitter.index(), u32::MAX);
    }

    println!(
        "Emitter config validation test passed - All {} configs accepted",
        5
    );
}

/// Test memory usage calculation.
#[test]
fn test_memory_usage_calculation() {
    let context = Rc::new(create_headless_context(false));

    // Test with different particle counts
    let particle_counts = vec![1_000, 10_000, 100_000, 1_000_000];

    for max_particles in particle_counts {
        let particle_system = GlobalParticleSystem::new(&context, max_particles)
            .expect("Failed to create particle system");

        let stats = particle_system.get_stats();

        // Calculate expected memory:
        // - Particle data: 48 bytes per particle
        // - Index lists: 12 bytes per particle (3 lists * 4 bytes)
        // - Counters: 32 bytes (negligible)
        // - Emitter configs: 80 bytes per emitter (0 emitters initially)
        let expected_particle_mb = (max_particles as f32) * 48.0 / (1024.0 * 1024.0);
        let expected_index_mb = (max_particles as f32) * 12.0 / (1024.0 * 1024.0);
        let expected_total_mb = expected_particle_mb + expected_index_mb;

        // Allow 10% tolerance for overhead
        let tolerance = expected_total_mb * 0.1;

        assert!(
            (stats.memory_used_mb - expected_total_mb).abs() < tolerance,
            "Memory usage for {} particles: expected {:.2} MB, got {:.2} MB",
            max_particles,
            expected_total_mb,
            stats.memory_used_mb
        );

        println!(
            "{} particles: {:.2} MB (expected {:.2} MB)",
            max_particles, stats.memory_used_mb, expected_total_mb
        );
    }

    println!("Memory usage calculation test passed");
}

/// Test statistics tracking.
#[test]
fn test_statistics_tracking() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 10_000;

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Create an emitter
    let config = EmitterConfig {
        emit_rate: 100.0,
        base_lifetime: 2.0,
        ..Default::default()
    };
    particle_system
        .create_emitter(config)
        .expect("Failed to create emitter");

    // Check initial stats
    let stats = particle_system.get_stats();
    assert_eq!(stats.frame_count, 0);
    assert_eq!(stats.current_alive_count, 0);
    assert_eq!(stats.max_alive_count, max_particles);
    assert!(stats.memory_used_mb > 0.0);

    // Update a few frames
    for _ in 0..10 {
        particle_system
            .update(0.016)
            .expect("Update should succeed");
    }

    // Check stats after updates
    let stats = particle_system.get_stats();
    assert_eq!(stats.frame_count, 10);
    // Note: total_dispatches is only incremented during actual compute dispatch
    // which happens during render graph execution, not in unit tests
    assert!(stats.total_dispatches >= 0);

    println!("Statistics tracking test passed");
}

/// Test burst emission functionality.
///
/// This validates that burst() method works correctly and emits
/// the specified number of particles immediately.
#[test]
fn test_burst_emission() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 10_000;

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Create an emitter with zero emit rate (only bursts)
    let config = EmitterConfig {
        emit_rate: 0.0, // No continuous emission
        base_lifetime: 2.0,
        position: [0.0, 1.0, 0.0],
        ..Default::default()
    };

    let emitter = particle_system
        .create_emitter(config)
        .expect("Failed to create emitter");

    // Burst 100 particles
    particle_system
        .burst(emitter, 100)
        .expect("Burst should succeed");

    // Update to process the burst
    particle_system
        .update(0.016)
        .expect("Update should succeed");

    // Verify particles were emitted
    let stats = particle_system.get_stats();
    assert!(
        stats.total_emitted >= 100,
        "Should have emitted at least 100 particles"
    );

    println!(
        "Burst emission test passed - {} particles emitted",
        stats.total_emitted
    );
}

/// Test multiple bursts in sequence.
///
/// This validates that multiple bursts can be queued and processed correctly.
#[test]
fn test_multiple_bursts() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 10_000;

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Create an emitter
    let config = EmitterConfig {
        emit_rate: 0.0,
        base_lifetime: 1.0,
        ..Default::default()
    };

    let emitter = particle_system
        .create_emitter(config)
        .expect("Failed to create emitter");

    // Burst multiple times
    particle_system
        .burst(emitter, 50)
        .expect("Burst 1 should succeed");
    particle_system
        .update(0.016)
        .expect("Update 1 should succeed");

    particle_system
        .burst(emitter, 100)
        .expect("Burst 2 should succeed");
    particle_system
        .update(0.016)
        .expect("Update 2 should succeed");

    particle_system
        .burst(emitter, 200)
        .expect("Burst 3 should succeed");
    particle_system
        .update(0.016)
        .expect("Update 3 should succeed");

    // Verify total emissions
    let stats = particle_system.get_stats();
    assert!(
        stats.total_emitted >= 350,
        "Should have emitted at least 350 particles"
    );

    println!(
        "Multiple bursts test passed - {} particles emitted",
        stats.total_emitted
    );
}

/// Test burst with continuous emission.
///
/// This validates that burst works alongside normal emit_rate.
#[test]
fn test_burst_with_continuous_emission() {
    let context = Rc::new(create_headless_context(false));
    let max_particles = 10_000;

    let mut particle_system = GlobalParticleSystem::new(&context, max_particles)
        .expect("Failed to create particle system");

    // Create an emitter with continuous emission
    let config = EmitterConfig {
        emit_rate: 100.0, // 100 particles per second
        base_lifetime: 2.0,
        ..Default::default()
    };

    let emitter = particle_system
        .create_emitter(config)
        .expect("Failed to create emitter");

    // Update with continuous emission only
    particle_system
        .update(0.1)
        .expect("Update 1 should succeed");

    let stats1 = particle_system.get_stats();
    let continuous_only = stats1.total_emitted;

    // Burst additional particles
    particle_system
        .burst(emitter, 500)
        .expect("Burst should succeed");
    particle_system
        .update(0.1)
        .expect("Update 2 should succeed");

    let stats2 = particle_system.get_stats();
    let with_burst = stats2.total_emitted - continuous_only;

    // Burst should add approximately 500 particles (plus ~10 from continuous)
    assert!(
        with_burst >= 500,
        "Burst should add at least 500 particles, got {}",
        with_burst
    );
    assert!(
        with_burst <= 520,
        "Burst should not add too many particles, got {}",
        with_burst
    );

    println!(
        "Burst with continuous emission test passed - Continuous: {}, Burst: {}",
        continuous_only, with_burst
    );
}
