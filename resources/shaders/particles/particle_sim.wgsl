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

// Per-frame data (Set 0, Binding 1)
struct FrameData {
    delta_time: f32,
    emit_count: u32,
    max_particles: u32,
    random_seed: u32,
}

// Per-emitter configuration (Set 1, Binding 0)
struct EmitterConfig {
    position: vec3f,
    emit_count: u32,
    velocity_direction: vec3f,
    base_lifetime: f32,
    velocity_magnitude: f32,
    velocity_cone_angle: f32,
    base_scale: f32,
    color: vec4f,
}

// Set 0: Per-frame resources (shared by all emitters)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData>;

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
    p._pad1 = 0.0;
    p.velocity = velocity;
    p.lifetime = emitter.base_lifetime * (0.8 + r4 * 0.4);  // ±20% variation
    p.color = emitter.color;
    p.scale = emitter.base_scale * (0.5 + r3 * 1.0);  // 50-150% variation
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

    // TODO: Limit initialization to avoid GPU timeout
    // Only initialize first 1000 particles on first frame
    if (index < 1000u && particle.lifetime <= 0.0) {
        let seed = frame.random_seed + index;
        particle = init_particle(index, seed);
        particles[index] = particle;
        return;
    }

    // Emit new particles (only first N threads handle emission)
    let emit_this_frame = min(emitter.emit_count, 1000u);  // Cap at 1000
    if (index < emit_this_frame) {
        // Always emit new particles in the first N slots
        // This ensures particles are spawned every frame
        let seed = frame.random_seed + index;
        particle = init_particle(index, seed);

        // Write immediately so other threads can see it
        particles[index] = particle;
        return;
    }

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

    // Write back
    particles[index] = particle;
}
