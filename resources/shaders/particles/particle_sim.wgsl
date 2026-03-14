// GPU Particle Simulation Compute Shader
//
// Simulates particles on the GPU using a single read_write storage buffer.
// Workgroup size: 256 (optimal for most GPUs)
//
// Descriptor Set Layout:
// Set 0 (Per-Frame, Shared):
//   Binding 0: Storage buffer (particle data)
//   Binding 1: Uniform buffer (frame data)
// Set 1 (Per-Emitter, Updated via push descriptors):
//   Binding 0: Uniform buffer (emitter config)

// Maximum particles per emitter (must match buffer size)
const MAX_PARTICLES: u32 = 65536u;

// Particle data structure (must match ParticleData in particle_buffer.rs)
struct ParticleData {
    position: vec3f,
    scale: f32,
    velocity: vec3f,
    lifetime: f32,
    color: vec4f,
}

// Per-frame data (Set 0, Binding 1)
struct FrameData {
    delta_time: f32,
    emit_count: u32,
    max_particles: u32,
    random_seed: u32,
}

// Per-emitter configuration (Set 1, Binding 0)
// std140 layout: vec3 is 16-byte aligned
struct EmitterConfig {
    position: vec3f,      // offset 0, 16 bytes with _pad1
    _pad1: u32,
    emit_count: u32,        // offset 16, 4 bytes
    _pad2: u32,             // offset 20
    _pad3: u32,             // offset 24
    _pad4: u32,             // offset 28 → total 32 bytes (16-byte boundary)
    velocity_direction: vec3f,  // offset 32, 16 bytes with _pad5
    _pad5: u32,
    base_lifetime: f32,   // offset 48
    velocity_magnitude: f32,  // offset 52
    velocity_cone_angle: f32,  // offset 56
    base_scale: f32,      // offset 60
    color: vec4f,         // offset 64-79 → total 80 bytes
}

// Set 0: Per-frame resources (shared by all emitters)
// Use fixed-size array to help SPIR-V generate correct bounds checking
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData, 65536>;

@group(0) @binding(1)
var<uniform> frame: FrameData;

// Set 1: Per-emitter configuration (updated via push descriptors)
@group(1) @binding(0)
var<uniform> emitter: EmitterConfig;

// Simple hash function for pseudo-random numbers
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

// Random float in [0, 1)
fn random(seed: u32) -> f32 {
    return f32(hash(seed)) / 4294967296.0;
}

// Random float in [-1, 1]
fn random_signed(seed: u32) -> f32 {
    return f32(hash(seed)) / 2147483648.0 - 1.0;
}

// Initialize a new particle with emitter config
fn init_particle(index: u32, seed: u32) -> ParticleData {
    let r1 = random(seed);
    let r2 = random(seed + 1u);
    let r3 = random(seed + 2u);
    let r4 = random(seed + 3u);

    // Random velocity in cone defined by emitter config
    let cone_angle = emitter.velocity_cone_angle;
    let theta = r1 * 6.28318530718;  // 2 * PI
    let phi = r2 * cone_angle;

    // Spherical to cartesian with emitter direction as basis
    let forward = normalize(emitter.velocity_direction);
    let speed = emitter.velocity_magnitude * (0.5 + r3 * 0.5);  // ±50% variation

    // Create perpendicular vectors for cone
    let up = vec3f(0.0, 1.0, 0.0);
    let right = normalize(cross(forward, up));
    let local_up = cross(right, forward);

    // Velocity within cone
    let velocity = normalize(forward + right * sin(theta) * sin(phi) + local_up * cos(phi)) * speed;

    var p: ParticleData;
    p.position = emitter.position;
    p.scale = emitter.base_scale * (0.5 + r3 * 1.0);  // 50-150% variation
    p.velocity = velocity;
    p.lifetime = emitter.base_lifetime * (0.8 + r4 * 0.4);  // ±20% variation
    p.color = emitter.color;

    return p;
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let index = global_id.x;

    // Guard against out-of-bounds access
    if (index >= MAX_PARTICLES) {
        return;
    }

    var particle = particles[index];

    // Check if particle is alive (lifetime > 0 means alive)
    if (particle.lifetime > 0.0) {
        // Update lifetime
        particle.lifetime -= frame.delta_time;

        if (particle.lifetime > 0.0) {
            // Update position
            particle.position += particle.velocity * frame.delta_time;

            // Apply simple gravity
            particle.velocity.y -= 0.5 * frame.delta_time;

            // Fade out over lifetime (last 1 second)
            if (particle.lifetime < 1.0) {
                particle.color.a = particle.lifetime;
            }
        } else {
            // Particle died - mark as dead (lifetime = 0)
            particle.lifetime = 0.0;
            particle.color.a = 0.0;
        }

        // Write back updated particle
        particles[index] = particle;
    } else {
        // Particle is dead - try to emit a new one
        // Use a ring buffer approach: each frame, emit into a sliding window of slots
        // The window starts at (random_seed % MAX_PARTICLES) and spans emit_count slots
        // This ensures exactly emit_count particles are attempted each frame

        let emit_window_start = frame.random_seed % MAX_PARTICLES;
        let emit_window_end = emit_window_start + emitter.emit_count;

        // Check if this index falls within the emission window (with wraparound)
        var in_window = false;
        if (emit_window_end <= MAX_PARTICLES) {
            // No wraparound
            in_window = index >= emit_window_start && index < emit_window_end;
        } else {
            // Wraparound case: window spans end and beginning of buffer
            in_window = index >= emit_window_start || index < (emit_window_end % MAX_PARTICLES);
        }

        if (in_window) {
            let seed = frame.random_seed + index;
            particle = init_particle(index, seed);
            particles[index] = particle;
        }
    }
}
