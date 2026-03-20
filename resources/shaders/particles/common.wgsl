// Shared particle types and constants (must match Rust side).

const MAX_PARTICLES: u32 = 1048576u; // 1M particles

// Particle data structure (must match ParticleData in buffer.rs)
// WGSL struct size is padded to multiple of 16 (vec3f alignment).
struct ParticleData {
    position: vec3f,
    scale: f32,
    velocity: vec3f,
    lifetime: f32,
    color: vec4f,
    emitter_index: u32,
    _pad: array<f32, 3>,
}

// Per-frame data (updated via push descriptors)
struct FrameData {
    delta_time: f32,
    total_emit_count: u32,
    emitter_count: u32,
    random_seed: u32,
    total_simulate_count: u32,
    burst_count: u32,
    frame_index: u32,
    _pad: u32,
}

// Atomic counters for particle management
struct ParticleCounters {
    alive_count: atomic<u32>,
    dead_count: atomic<u32>,
    emit_count: atomic<u32>,
    workgroups_finished: atomic<u32>,
}

// Per-emitter configuration
// Layout must match EmitterConfig in mod.rs exactly.
// WGSL vec3f has 16-byte alignment; Rust [f32; 3] has 4-byte alignment.
// Explicit _pad_* fields bridge the gap.
struct EmitterConfig {
    position: vec3f,
    _pad_position: f32,
    shape: u32,
    emit_rate: f32,
    base_lifetime: f32,
    lifetime_variation: f32,
    velocity_direction: vec3f,
    _pad_velocity: f32,
    velocity_magnitude: f32,
    velocity_cone_angle: f32,
    base_scale: f32,
    scale_variation: f32,
    color: vec4f,
    color_variation: f32,
    _pad_color: vec4f,
    shape_params: vec4f,
    gravity: f32,
    turbulence_strength: f32,
    turbulence_frequency: f32,
    _pad_forces: f32,
}

const MAX_EMITTERS: u32 = 1024u;

// Emitter shape enumeration
const EMITTER_SHAPE_POINT: u32 = 0u;
const EMITTER_SHAPE_LINE: u32 = 1u;
const EMITTER_SHAPE_CIRCLE: u32 = 2u;
const EMITTER_SHAPE_SPHERE: u32 = 3u;
const EMITTER_SHAPE_BOX: u32 = 4u;
