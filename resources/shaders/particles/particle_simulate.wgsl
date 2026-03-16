// Particle Simulate Compute Shader
//
// Dedicated simulate pass for updating particle physics and handling death.
// Optimized for high-divergence workloads with workgroup size 64.
//
// Descriptor Set Layout:
// Set 0 (Global Resources):
//   Binding 0: Storage buffer (particle data)
//   Binding 1: Storage buffer (dead particle index list)
//   Binding 2: Storage buffer (alive particle index list - current)
//   Binding 3: Storage buffer (alive particle index list - next)
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

@group(0) @binding(2)
var<storage, read> alive_next: array<u32, MAX_PARTICLES>;

@group(0) @binding(3)
var<storage, read_write> alive_simulate_next: array<u32, MAX_PARTICLES>;

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

        // Fade out in last second
        if ((*particle).lifetime < 1.0) {
            (*particle).color.a = (*particle).lifetime;
        }
    }
}

// Simulate compute shader - updates particle physics and handles death
// Workgroup size of 64 optimized for high-divergence workloads
@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;

    // First thread resets alive_count to 0 - we'll recount only surviving particles
    if (idx == 0u) {
        atomicStore(&counters.alive_count, 0u);
    }
    workgroupBarrier();

    // Total particles to simulate = newly emitted (from emit_count) + previously alive (from cached value)
    let newly_emitted = atomicLoad(&counters.emit_count);
    let total_particles = newly_emitted + frame_data.total_simulate_count;

    // Early exit if beyond particle count
    if (idx >= total_particles) { return; }

    let particle_idx = alive_next[idx];

    // Validate particle index
    if (particle_idx >= MAX_PARTICLES) {
        return;
    }

    var particle = particles[particle_idx];

    simulate_particle(&particle, frame_data.delta_time);

    if (particle.lifetime > 0.0) {
        // Still alive - write back and add to alive list
        particles[particle_idx] = particle;
        let next_slot = atomicAdd(&counters.alive_count, 1u);
        alive_simulate_next[next_slot] = particle_idx;
    } else {
        // Particle died - return to dead list
        let dead_slot = atomicAdd(&counters.dead_count, 1u);
        dead_list[dead_slot] = particle_idx;
    }
}
