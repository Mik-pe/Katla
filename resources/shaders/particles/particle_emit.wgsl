// Particle Emit Compute Shader
//
// Dedicated emit pass for spawning new particles from dead list.
// Optimized for embarrassingly parallel emission with workgroup size 256.
//
// Descriptor Set Layout:
// Set 0 (Global Resources):
//   Binding 0: Storage buffer (particle data)
//   Binding 1: Storage buffer (dead particle index list)
//   Binding 2: Storage buffer (alive particle index list - read)
//   Binding 3: Storage buffer (alive particle index list next - write)
//   Binding 4: Storage buffer (atomic counters)
// Set 1 (Emitter Configs):
//   Binding 0: Uniform buffer (frame data)
//   Binding 1: Storage buffer (emitter configurations array)

const MAX_PARTICLES: u32 = 1048576u; // 1M particles
const MAX_EMITTERS: u32 = 1024u;

// Particle data structure (must match ParticleData in buffer.rs)
struct ParticleData {
    position: vec3f,
    scale: f32,
    velocity: vec3f,
    lifetime: f32,
    color: vec4f,
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
    _pad: u32,
}

// Per-emitter configuration
// Layout must match EmitterConfig in mod.rs exactly.
// WGSL vec3f has 16-byte alignment; Rust [f32; 3] has 4-byte alignment.
// Explicit _pad_* fields bridge the gap.
// Offsets: position(0) _pad_position(12) shape(16) emit_rate(20)
//          base_lifetime(24) lifetime_variation(28) velocity_direction(32)
//          _pad_velocity(44) velocity_magnitude(48) velocity_cone_angle(52)
//          base_scale(56) scale_variation(60) color(64) color_variation(80)
//          _pad_color(84) shape_params(96) = 112 bytes
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
}

// Emitter shape enumeration
const EMITTER_SHAPE_POINT: u32 = 0u;
const EMITTER_SHAPE_LINE: u32 = 1u;
const EMITTER_SHAPE_CIRCLE: u32 = 2u;
const EMITTER_SHAPE_SPHERE: u32 = 3u;
const EMITTER_SHAPE_BOX: u32 = 4u;

// Global resources (Set 0: static buffers)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData, MAX_PARTICLES>;

@group(0) @binding(1)
var<storage, read_write> dead_list: array<u32, MAX_PARTICLES>;

// Alive list (read_write) - contains survivors from previous frame, emit appends new particles here
// The Vulkan binding handles per-frame double-buffering transparently
@group(0) @binding(2)
var<storage, read_write> alive_list: array<u32, MAX_PARTICLES>;

// Alive list next (write) - newly emitted particles go here for simulate pass
@group(0) @binding(3)
var<storage, read_write> alive_list_next: array<u32, MAX_PARTICLES>;

@group(0) @binding(4)
var<storage, read_write> counters: ParticleCounters;

// Per-frame data (Set 1: updated via push descriptors)
@group(1) @binding(0)
var<uniform> frame_data: FrameData;

// Per-emitter configurations (Set 1: updated via push descriptors)
@group(1) @binding(1)
var<storage, read> emitters: array<EmitterConfig, MAX_EMITTERS>;

// Pseudo-random number generation
fn hash(seed: u32) -> u32 {
    var x = seed;
    x ^= x >> 17u;
    x *= 0xed5ad4bbu;
    x ^= x >> 11u;
    x *= 0xac4c1b51u;
    x ^= x >> 15u;
    x *= 0x31848babu;
    x ^= x >> 14u;
    return x;
}

fn random_float(seed: ptr<function, u32>) -> f32 {
    *seed = hash(*seed);
    return f32(*seed) / 4294967296.0;
}

fn random_signed(seed: ptr<function, u32>) -> f32 {
    *seed = hash(*seed);
    return f32(*seed) / 2147483648.0 - 1.0;
}

fn random_range(seed: ptr<function, u32>, min: f32, max: f32) -> f32 {
    return min + (max - min) * random_float(seed);
}

// Sample a position from the emitter shape
fn sample_emitter_position(config: EmitterConfig, seed: ptr<function, u32>) -> vec3f {
    let shape_type = config.shape;

    if (shape_type == EMITTER_SHAPE_POINT) {
        // Point: return config position directly
        return config.position;
    }
    else if (shape_type == EMITTER_SHAPE_LINE) {
        // Line: sample along line using shape_params[0] as length
        let length = config.shape_params.x;
        let t = random_range(seed, -0.5, 0.5); // Sample from center
        return config.position + vec3f(0.0, t * length, 0.0); // Default Y-axis line
    }
    else if (shape_type == EMITTER_SHAPE_CIRCLE) {
        // Circle: sample in circle using shape_params[0] as radius
        let radius = config.shape_params.x;
        let theta = random_float(seed) * 6.28318530718; // 2π
        let r = radius * sqrt(random_float(seed)); // Uniform distribution in circle
        let offset = vec3f(cos(theta) * r, 0.0, sin(theta) * r); // XZ plane circle
        return config.position + offset;
    }
    else if (shape_type == EMITTER_SHAPE_SPHERE) {
        // Sphere: sample in sphere using shape_params[0] as radius
        let radius = config.shape_params.x;
        let theta = random_float(seed) * 6.28318530718; // 2π (azimuthal)
        let phi = acos(2.0 * random_float(seed) - 1.0); // Polar angle (uniform sphere)
        let r = radius * pow(random_float(seed), 1.0 / 3.0); // Uniform volume distribution
        let x = r * sin(phi) * cos(theta);
        let y = r * sin(phi) * sin(theta);
        let z = r * cos(phi);
        return config.position + vec3f(x, y, z);
    }
    else if (shape_type == EMITTER_SHAPE_BOX) {
        // Box: sample in box using shape_params[0-2] as dimensions
        let width = config.shape_params.x;
        let height = config.shape_params.y;
        let depth = config.shape_params.z;
        let x = random_range(seed, -width * 0.5, width * 0.5);
        let y = random_range(seed, -height * 0.5, height * 0.5);
        let z = random_range(seed, -depth * 0.5, depth * 0.5);
        return config.position + vec3f(x, y, z);
    }
    else {
        // Default: point emission
        return config.position;
    }
}

// Initialize a new particle from emitter configuration
fn emit_particle(particle_idx: u32, emitter_idx: u32, seed: ptr<function, u32>) -> ParticleData {
    let emitter = emitters[emitter_idx];

    var particle: ParticleData;

    // Position: sample from emitter shape
    particle.position = sample_emitter_position(emitter, seed);

    // Lifetime with variation
    let lifetime_var = emitter.base_lifetime * emitter.lifetime_variation;
    particle.lifetime = random_range(seed, emitter.base_lifetime - lifetime_var, emitter.base_lifetime + lifetime_var);

    // Velocity in cone
    let cone_angle = emitter.velocity_cone_angle;
    let theta = random_float(seed) * 6.28318530718; // 2π
    let phi = random_float(seed) * cone_angle;

    let forward = normalize(emitter.velocity_direction);

    // Choose a reference vector that is not parallel to forward
    // to avoid degenerate cross product (which would produce NaN).
    // When forward is Y-dominant, use Z as up; otherwise use Y as up.
    let abs_forward = abs(forward);
    let up = select(vec3f(0.0, 1.0, 0.0), vec3f(0.0, 0.0, 1.0), abs_forward.y > abs_forward.z);

    let right = normalize(cross(forward, up));
    let local_up = cross(right, forward);

    let dir_in_cone = normalize(
        forward +
        right * sin(theta) * sin(phi) +
        local_up * cos(phi)
    );

    let speed_var = emitter.velocity_magnitude * 0.5; // ±50%
    let speed = random_range(seed, emitter.velocity_magnitude - speed_var, emitter.velocity_magnitude + speed_var);

    particle.velocity = dir_in_cone * speed;

    // Scale with variation
    let scale_var = emitter.base_scale * emitter.scale_variation;
    particle.scale = random_range(seed, emitter.base_scale - scale_var, emitter.base_scale + scale_var);
    particle.scale = max(particle.scale, 0.001); // Prevent zero scale

    // Color with variation
    let color_var = emitter.color_variation;
    particle.color = vec4f(
        random_range(seed, emitter.color.r - color_var, emitter.color.r + color_var),
        random_range(seed, emitter.color.g - color_var, emitter.color.g + color_var),
        random_range(seed, emitter.color.b - color_var, emitter.color.b + color_var),
        random_range(seed, emitter.color.a - color_var, emitter.color.a + color_var)
    );
    particle.color = clamp(particle.color, vec4f(0.0), vec4f(1.0));

    return particle;
}

// Emit compute shader - spawns new particles from dead list
@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;

    // Emit appends new particles to alive_current[frame_index] (binding 2 = alive_list).
    // The CPU pre-sets emit_count to cached_alive_count so that emit writes after existing survivors.
    // Simulate then reads ALL particles from alive_current[frame_index] and writes survivors to alive_next.

    // Early exit if beyond total emit count (rate-based + burst)
    if (idx >= frame_data.total_emit_count) { return; }

    // Early exit if no emitters active
    if (frame_data.emitter_count == 0u) { return; }

    // Calculate emitter index using round-robin distribution
    let wg_id = idx / 256u; // Workgroup ID
    let local_id = idx % 256u; // Thread in workgroup
    let emitter_idx = (wg_id + local_id) % frame_data.emitter_count;

    // Bounds check
    if (emitter_idx >= MAX_EMITTERS) {
        return;
    }

    // Allocate particle from dead list using atomicSub with underflow protection
    // atomicSub returns the ORIGINAL value before subtraction, so:
    // - dead_count starts at MAX_PARTICLES (e.g., 1048576)
    // - First call returns 1048576, then decrements dead_count to 1048575
    // - Second call returns 1048575, then decrements dead_count to 1048574
    // We need to subtract 1 to get the valid index into dead_list
    let original_dead_count = atomicSub(&counters.dead_count, 1u);

    // Check for underflow - if original value was 0, abort
    // When atomicSub underflows (dead_count was already 0), it wraps around to u32::MAX
    if (original_dead_count == 0u || original_dead_count > MAX_PARTICLES) {
        // Underflow occurred - restore counter and abort
        atomicAdd(&counters.dead_count, 1u);
        return;
    }

    // Use original_dead_count - 1 as the index into dead_list
    // This gives us the correct particle index from the dead pool
    let dead_slot = original_dead_count - 1u;
    let particle_idx = dead_list[dead_slot];

    // Validate particle index - MUST restore counter if invalid!
    if (particle_idx >= MAX_PARTICLES) {
        // Restore dead_count since we didn't use this slot
        atomicAdd(&counters.dead_count, 1u);
        return;
    }

    var seed = frame_data.random_seed + idx * 7u;
    var new_particle = emit_particle(particle_idx, emitter_idx, &seed);

    particles[particle_idx] = new_particle;

    let write_slot = atomicAdd(&counters.emit_count, 1u);

    // Write to alive_list (binding 2) which points to alive_current[frame].
    // The CPU sets emit_count = cached_alive_count before this dispatch,
    // so emit appends after existing survivors in alive_current[frame].
    // Simulate will then read ALL particles from alive_current[frame].
    alive_list[write_slot] = particle_idx;
}
