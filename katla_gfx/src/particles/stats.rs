//! Statistics tracking for particle system performance and behavior monitoring.

use serde::{Deserialize, Serialize};

/// Particle system statistics snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParticleStats {
    // Particle counts
    /// Maximum number of particles the system can hold
    pub max_alive_count: u32,
    /// Current number of alive particles
    pub current_alive_count: u32,
    /// Current number of dead particles
    pub dead_count: u32,

    // Lifetime stats
    /// Total particles emitted since system start
    pub total_emitted: u64,
    /// Total particles that died naturally since system start
    pub total_died: u64,

    // Performance stats
    /// Compute shader execution time for last frame (milliseconds)
    pub compute_time_ms: f32,
    /// Average compute shader execution time over last 60 frames (milliseconds)
    pub avg_compute_time_ms: f32,
    /// Peak compute shader execution time (milliseconds)
    pub peak_compute_time_ms: f32,

    // Per-emitter stats
    /// Active particles per emitter (indices match emitter indices)
    pub emitter_counts: Vec<u32>,

    // Memory stats
    /// Total GPU memory usage (megabytes)
    pub memory_used_mb: f32,
    /// Buffer utilization ratio (alive_count / max_particles)
    pub buffer_utilization: f32,

    // Frame stats
    /// Total frames rendered since system start
    pub frame_count: u64,
    /// Total compute dispatches executed
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_stats_default() {
        let stats = ParticleStats::default();
        assert_eq!(stats.max_alive_count, 0);
        assert_eq!(stats.current_alive_count, 0);
        assert_eq!(stats.compute_time_ms, 0.0);
    }

    #[test]
    fn test_particle_stats_serialization() {
        let stats = ParticleStats {
            max_alive_count: 1000,
            current_alive_count: 500,
            ..Default::default()
        };

        let serialized = serde_json::to_string(&stats).unwrap();
        let deserialized: ParticleStats = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.max_alive_count, 1000);
        assert_eq!(deserialized.current_alive_count, 500);
    }
}
