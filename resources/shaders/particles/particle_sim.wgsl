// GPU Particle Simulation Compute Shader
//
// Simulates particles on the GPU using a single read_write storage buffer.
// Workgroup size: 256 (optimal for most GPUs)
//
// Each particle:
// - Updates position based on velocity
// - Decreases lifetime
// - Gets recycled when lifetime reaches 0
//
// Frame data is passed via a uniform buffer (binding 1).

// Particle data structure (must match ParticleData in particle_buffer.rs)
struct ParticleData {
    position: vec3f,
    _pad1: f32,
    velocity: vec3f,
    lifetime: f32,
    color: vec4f,
    scale: f32,
    _pad2: vec3f,
}

// Per-frame data (passed via uniform buffer)
struct FrameData {
    delta_time: f32,
    emit_count: u32,
    max_particles: u32,
    random_seed: u32,
}

// Storage buffer with read_write access
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData>;

// Uniform buffer for frame data
@group(0) @binding(1)
var<uniform> frame: FrameData;

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

// Initialize a new particle with random properties
fn init_particle(index: u32, seed: u32) -> ParticleData {
    let r1 = random(seed);
    let r2 = random(seed + 1u);
    let r3 = random(seed + 2u);
    let r4 = random(seed + 3u);

    // Random velocity in a cone (simplified)
    let angle = r1 * 0.5;  // Cone angle
    let speed = 1.0 + r2 * 2.0;  // Speed range [1, 3]

    // Random direction on unit sphere, biased upward
    let theta = r3 * 6.28318530718;  // 2 * PI
    let phi = 0.5 + r4 * 0.5;  // Bias toward upper hemisphere [0.5, 1.0]

    let vx = sin(phi) * cos(theta) * speed;
    let vy = cos(phi) * speed;
    let vz = sin(phi) * sin(theta) * speed;

    var p: ParticleData;
    p.position = vec3f(0.0, 0.0, 0.0);  // Emitter position set via config
    p._pad1 = 0.0;
    p.velocity = vec3f(vx, vy, vz);
    p.lifetime = 3.0 + r1 * 4.0;  // Lifetime range [3, 7]
    p.color = vec4f(1.0, 0.8 + r2 * 0.2, 0.3, 1.0);  // Orange/yellow fire colors
    p.scale = 0.1 + r3 * 0.2;
    p._pad2 = vec3f(0.0, 0.0, 0.0);

    return p;
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let index = global_id.x;

    // Guard against out-of-bounds access
    if (index >= frame.max_particles) {
        return;
    }

    var particle = particles[index];

    // Check if particle is alive
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
            // Particle died - mark as dead (negative lifetime)
            particle.lifetime = -1.0;
            particle.color.a = 0.0;
        }
    }

    // Emit new particles (only first N threads handle emission)
    if (index < frame.emit_count) {
        // Find a dead particle slot to reuse
        // For simplicity, we emit at the current index if it's dead
        let emit_index = index;

        if (emit_index < frame.max_particles) {
            let dead_particle = particles[emit_index];
            if (dead_particle.lifetime <= 0.0) {
                // Reuse this slot
                let seed = frame.random_seed + index;
                particle = init_particle(emit_index, seed);
            }
        }
    }

    // Write back
    particles[index] = particle;
}
