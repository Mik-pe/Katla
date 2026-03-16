// Particle Simulate Compute Shader
//
// Dedicated simulate pass for updating particle physics and handling death.
// Optimized for high-divergence workloads with workgroup size 64.
//
// Descriptor Set Layout:
// Set 0 (Global Resources):
//   Binding 0: Storage buffer (particle data)
//   Binding 1: Storage buffer (dead particle index list)
//   Binding 2: Storage buffer (alive particle index list - read)
//   Binding 3: Storage buffer (alive particle index list next - write)
//   Binding 4: Storage buffer (atomic counters)
// Set 1 (Frame Data):
//   Binding 0: Uniform buffer (frame data only - no emitter configs needed)

const MAX_PARTICLES: u32 = 1048576u; // 1M particles

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

// Global resources (Set 0: static buffers)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData, MAX_PARTICLES>;

@group(0) @binding(1)
var<storage, read_write> dead_list: array<u32, MAX_PARTICLES>;

// Alive list (read) - contains particles to simulate (emitted + survivors)
// The Vulkan binding handles per-frame double-buffering transparently
@group(0) @binding(2)
var<storage, read> alive_list: array<u32, MAX_PARTICLES>;

// Alive list next (write) - surviving particles written here for next frame
@group(0) @binding(3)
var<storage, read_write> alive_list_next: array<u32, MAX_PARTICLES>;

@group(0) @binding(4)
var<storage, read_write> counters: ParticleCounters;

// Per-frame data (Set 1: updated via push descriptors)
// Note: Simulate pass only needs frame data, not emitter configs
@group(1) @binding(0)
var<uniform> frame_data: FrameData;

// Simulate particle physics and lifetime
fn simulate_particle(particle: ptr<function, ParticleData>, delta_time: f32) {
    (*particle).lifetime -= delta_time;

    if ((*particle).lifetime > 0.0) {
        // Update position
        (*particle).position += (*particle).velocity * delta_time;

        // Apply gravity
        (*particle).velocity.y -= 9.8 * delta_time;

        // Keep alpha at 1.0 for fully opaque particles
        (*particle).color.a = 1.0;
    }
}

// Simulate compute shader - updates particle physics and handles death
// Workgroup size of 64 optimized for high-divergence workloads
@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;

    // First thread resets alive_count to 0 - we'll recount only surviving particles
    // This happens AFTER emit pass has already used the previous alive_count
    if (idx == 0u) {
        atomicStore(&counters.alive_count, 0u);
    }

    workgroupBarrier();

    // Total particles to simulate = newly emitted + previously alive
    let total_particles = frame_data.total_simulate_count;

    // Early exit if beyond particle count
    if (idx >= total_particles) { return; }

    // Read particle index from alive_list (emitted + previous survivors)
    // The descriptor binding handles which memory region to read from
    let particle_idx = alive_list[idx];

    // Validate particle index
    if (particle_idx >= MAX_PARTICLES) {
        return;
    }

    var particle = particles[particle_idx];

    simulate_particle(&particle, frame_data.delta_time);

    if (particle.lifetime > 0.0) {
        // Still alive - write particle data back and add to alive_list_next
        particles[particle_idx] = particle;
        let next_slot = atomicAdd(&counters.alive_count, 1u);

        // Bounds check before writing to alive_list_next
        if (next_slot < MAX_PARTICLES) {
            alive_list_next[next_slot] = particle_idx;
        }
    } else {
        // Particle died - do nothing
        // The particle index remains in the conceptual "dead pool"
        // and will be reused by the emit shader when it decrements dead_count
        // We do NOT write back to the dead list as it's a static list of all indices
    }
}
