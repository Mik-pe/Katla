//! Particle system statistics for the application layer.

use serde::{Deserialize, Serialize};

/// Particle system statistics snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParticleStats {
    pub max_alive_count: u32,
    pub current_alive_count: u32,
    pub dead_count: u32,
    pub total_emitted: u64,
    pub total_died: u64,
    pub compute_time_ms: f32,
    pub avg_compute_time_ms: f32,
    pub peak_compute_time_ms: f32,
    pub emitter_counts: Vec<u32>,
    pub memory_used_mb: f32,
    pub buffer_utilization: f32,
    pub frame_count: u64,
    pub total_dispatches: u64,
}

impl Default for ParticleStats {
    fn default() -> Self {
        Self {
            max_alive_count: 0,
            current_alive_count: 0,
            dead_count: 0,
            total_emitted: 0,
            total_died: 0,
            compute_time_ms: 0.0,
            avg_compute_time_ms: 0.0,
            peak_compute_time_ms: 0.0,
            emitter_counts: Vec::new(),
            memory_used_mb: 0.0,
            buffer_utilization: 0.0,
            frame_count: 0,
            total_dispatches: 0,
        }
    }
}
