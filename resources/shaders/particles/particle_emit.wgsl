// Particle Emit Compute Shader
//
// Dedicated emit pass for spawning new particles from dead list.
// Optimized for embarrassingly parallel emission with workgroup size 256.
//
// Descriptor Set Layout:
// Set 0 (Global Resources):
//   Binding 0: Storage buffer (particle data)
//   Binding 1: Storage buffer (dead particle index list)
//   Binding 2: Storage buffer (alive particle index list - current)
//   Binding 3: Storage buffer (alive particle index list - next)
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
}

// Atomic counters for particle management
struct ParticleCounters {
    alive_count: atomic<u32>,
    dead_count: atomic<u32>,
    emit_count: atomic<u32>,
    _pad: u32,
}

// Per-emitter configuration
struct EmitterConfig {
    position: vec3f,
    _pad0: f32,
    emit_rate: f32,
    base_lifetime: f32,
    lifetime_variation: f32,
    velocity_direction: vec3f,
    _pad1: f32,
    velocity_magnitude: f32,
    velocity_cone_angle: f32,
    base_scale: f32,
    scale_variation: f32,
    color: vec4f,
    color_variation: f32,
}

// Global resources (Set 0: static buffers)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData, MAX_PARTICLES>;

@group(0) @binding(1)
var<storage, read_write> dead_list: array<u32, MAX_PARTICLES>;

@group(0) @binding(2)
var<storage, read> alive_current: array<u32, MAX_PARTICLES>;

@group(0) @binding(3)
var<storage, read_write> alive_next: array<u32, MAX_PARTICLES>;

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

// Initialize a new particle from emitter configuration
fn emit_particle(particle_idx: u32, emitter_idx: u32, seed: ptr<function, u32>) -> ParticleData {
    let emitter = emitters[emitter_idx];

    var particle: ParticleData;

    // Position: emitter position
    particle.position = emitter.position;

    // Lifetime with variation
    let lifetime_var = emitter.base_lifetime * emitter.lifetime_variation;
    particle.lifetime = random_range(seed, emitter.base_lifetime - lifetime_var, emitter.base_lifetime + lifetime_var);

    // Velocity in cone
    let cone_angle = emitter.velocity_cone_angle;
    let theta = random_float(seed) * 6.28318530718; // 2π
    let phi = random_float(seed) * cone_angle;

    let forward = normalize(emitter.velocity_direction);
    let up = vec3f(0.0, 1.0, 0.0);
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

    // First thread resets emit_count to 0 for this frame
    if (idx == 0u) {
        atomicStore(&counters.emit_count, 0u);
    }
    workgroupBarrier();

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

    // Allocate particle from dead list
    let dead_slot = atomicSub(&counters.dead_count, 1u);

    if (dead_slot > 0u) {
        let particle_idx = dead_list[dead_slot - 1u];

        // Validate particle index
        if (particle_idx >= MAX_PARTICLES) {
            return;
        }

        var seed = frame_data.random_seed + idx * 7u;
        var new_particle = emit_particle(particle_idx, emitter_idx, &seed);

        particles[particle_idx] = new_particle;

        // Add to alive list and increment emit_count
        let alive_slot = atomicAdd(&counters.alive_count, 1u);
        let emit_slot = atomicAdd(&counters.emit_count, 1u);
        alive_next[alive_slot] = particle_idx;
    }
}
