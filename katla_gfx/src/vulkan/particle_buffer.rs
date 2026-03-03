/// Per-particle data structure for GPU simulation.
///
/// Size: 64 bytes (cache-line aligned for optimal GPU access).
/// Total memory for 64K particles: 4MB.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleData {
    /// World position (x, y, z)
    pub position: [f32; 3],
    /// Padding for 16-byte alignment
    pub _pad1: f32,
    /// Velocity (x, y, z)
    pub velocity: [f32; 3],
    /// Remaining lifetime in seconds
    pub lifetime: f32,
    /// RGBA color (0-1 range)
    pub color: [f32; 4],
    /// Scale factor
    pub scale: f32,
    /// Padding for 16-byte alignment
    pub _pad2: [f32; 3],
}

impl Default for ParticleData {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            _pad1: 0.0,
            velocity: [0.0; 3],
            lifetime: 0.0,
            color: [1.0; 4],
            scale: 1.0,
            _pad2: [0.0; 3],
        }
    }
}

/// Emitter configuration passed to compute shader via push constants.
///
/// This structure is small enough to fit in push constants (<= 128 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EmitterConfig {
    /// World position of the emitter
    pub position: [f32; 3],
    /// Particles to emit this frame (calculated from emit_rate * delta_time)
    pub emit_count: u32,
    /// Initial velocity direction
    pub velocity_direction: [f32; 3],
    /// Base lifetime for new particles
    pub base_lifetime: f32,
    /// Velocity magnitude (random within cone)
    pub velocity_magnitude: f32,
    /// Random velocity cone angle (0 = straight, PI/2 = hemisphere)
    pub velocity_cone_angle: f32,
    /// Base scale for new particles
    pub base_scale: f32,
    /// Color for new particles (RGBA)
    pub color: [f32; 4],
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            emit_count: 0,
            velocity_direction: [0.0, 1.0, 0.0],
            base_lifetime: 5.0,
            velocity_magnitude: 1.0,
            velocity_cone_angle: 0.5,
            base_scale: 0.1,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_data_size() {
        // Verify 64-byte size for cache alignment
        assert_eq!(std::mem::size_of::<ParticleData>(), 64);
    }

    #[test]
    fn test_emitter_config_size() {
        // Should fit in push constants (<= 128 bytes)
        assert!(std::mem::size_of::<EmitterConfig>() <= 128);
    }

    #[test]
    fn test_particle_data_default() {
        let particle = ParticleData::default();
        assert_eq!(particle.position, [0.0; 3]);
        assert_eq!(particle.velocity, [0.0; 3]);
        assert_eq!(particle.lifetime, 0.0);
        assert_eq!(particle.color, [1.0; 4]);
        assert_eq!(particle.scale, 1.0);
    }

    #[test]
    fn test_emitter_config_default() {
        let config = EmitterConfig::default();
        assert_eq!(config.position, [0.0; 3]);
        assert_eq!(config.emit_count, 0);
        assert_eq!(config.velocity_direction, [0.0, 1.0, 0.0]);
        assert_eq!(config.base_lifetime, 5.0);
    }
}
