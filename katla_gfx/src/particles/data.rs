//! Portable particle shader buffer layouts.

use bytemuck::{Pod, Zeroable};

/// Particle data structure (64 bytes).
///
/// Layout must match WGSL struct exactly. WGSL pads struct size to a multiple
/// of the largest member alignment (vec3f = 16 bytes), so 12 bytes of padding
/// are added after emitter_index.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleData {
    /// World position (x, y, z)
    pub position: [f32; 3],
    /// Scale factor
    pub scale: f32,
    /// Velocity (x, y, z)
    pub velocity: [f32; 3],
    /// Remaining lifetime in seconds
    pub lifetime: f32,
    /// RGBA color (0-1 range)
    pub color: [f32; 4],
    /// Index of the emitter that spawned this particle
    pub emitter_index: u32,
    /// Total lifetime assigned at emit (used to compute normalized age for color/size curves)
    pub max_lifetime: f32,
    /// Scale assigned at emit (used as base for size-over-lifetime curve)
    pub initial_scale: f32,
    /// Padding to match WGSL struct alignment (vec3f align = 16, struct size must be multiple of 16)
    pub _pad: f32,
}

/// Per-frame data for graph particle simulation.
///
/// Must match WGSL `FrameData` exactly (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameData {
    pub delta_time: f32,
    pub total_emit_count: u32,
    pub emitter_count: u32,
    pub random_seed: u32,
    pub total_simulate_count: u32,
    pub burst_count: u32,
    pub frame_index: u32,
    pub max_particles: u32,
}

/// Atomic counters for particle management (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleCounters {
    /// Number of alive particles (atomic)
    pub alive_count: u32,
    /// Number of dead particles (atomic, starts at MAX_PARTICLES)
    pub dead_count: u32,
    /// Number of newly emitted particles this frame (set by emit, read by simulate)
    pub emit_count: u32,
    /// Number of workgroups that completed simulate processing.
    /// Used by the last workgroup to write the indirect draw command.
    pub workgroups_finished: u32,
}

/// Four-word non-indexed indirect drawing command.
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
pub struct IndirectDrawCommandData {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_data_size() {
        assert_eq!(std::mem::size_of::<ParticleData>(), 64);
    }

    #[test]
    fn test_counters_size() {
        assert_eq!(std::mem::size_of::<ParticleCounters>(), 16);
    }

    #[test]
    fn test_frame_data_size() {
        assert_eq!(std::mem::size_of::<FrameData>(), 32);
    }

    #[test]
    fn test_indirect_draw_command_size() {
        assert_eq!(std::mem::size_of::<IndirectDrawCommandData>(), 16);
    }

    #[test]
    fn test_frame_data_offsets() {
        assert_eq!(std::mem::offset_of!(FrameData, delta_time), 0);
        assert_eq!(std::mem::offset_of!(FrameData, total_emit_count), 4);
        assert_eq!(std::mem::offset_of!(FrameData, emitter_count), 8);
        assert_eq!(std::mem::offset_of!(FrameData, random_seed), 12);
        assert_eq!(std::mem::offset_of!(FrameData, total_simulate_count), 16);
        assert_eq!(std::mem::offset_of!(FrameData, burst_count), 20);
        assert_eq!(std::mem::offset_of!(FrameData, frame_index), 24);
        assert_eq!(std::mem::offset_of!(FrameData, max_particles), 28);
    }
}
