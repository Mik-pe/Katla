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
//   Binding 5: Storage buffer (indirect draw command - written by simulate, read by render)
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
    workgroups_finished: atomic<u32>,
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

// Indirect draw command (written by simulate, read by render via vkCmdDrawIndirect)
struct DrawIndirectCommand {
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
}

@group(0) @binding(5)
var<storage, read_write> draw_command: DrawIndirectCommand;

// Per-frame data (Set 1: updated via push descriptors)
// Note: Simulate pass only needs frame data, not emitter configs
@group(1) @binding(0)
var<uniform> frame_data: FrameData;

// Simulate particle physics and lifetime
fn simulate_particle(particle: ptr<function, ParticleData>, delta_time: f32) {
    let old_lifetime = (*particle).lifetime;
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

// Simulate particle physics and handle death
// Workgroup size of 64 optimized for high-divergence workloads
@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;
    let local_id = idx % 64u;

    // DOUBLE-BUFFERING FLOW:
    // 1. Emit wrote new particles to alive_list (= alive_current[frame]) after existing survivors
    // 2. Simulate reads ALL particles from alive_list and writes survivors to alive_list_next
    // 3. After simulate, swap copies alive_list_next -> alive_current[next_frame]
    //
    // alive_count was reset to 0 by vkCmdFillBuffer before this dispatch.
    // alive_list contains: [old survivors(0..alive_count-1)] + [newly emitted(alive_count..total_simulate_count-1)]
    // Simulate writes survivors to alive_list_next starting at slot 0.

    // Total particles to simulate (old survivors + newly emitted, all in alive_list)
    let total_particles = frame_data.total_simulate_count;

    if (idx < total_particles) {
        // Read particle index from alive_list (old survivors + newly emitted)
        let particle_idx = alive_list[idx];

        // Validate particle index
        if (particle_idx < MAX_PARTICLES) {
            var particle = particles[particle_idx];

            simulate_particle(&particle, frame_data.delta_time);

            if (particle.lifetime > 0.0) {
                // Still alive - write particle data back and add to alive_list_next
                particles[particle_idx] = particle;

                // Atomically add this survivor to the count
                let survivor_slot = atomicAdd(&counters.alive_count, 1u);

                // Bounds check before writing to alive_list_next
                if (survivor_slot < MAX_PARTICLES) {
                    alive_list_next[survivor_slot] = particle_idx;
                }
            } else {
                // Particle died - return index to dead list for reuse by emit shader.
                // Clamp dead_count to MAX_PARTICLES to prevent the counter from drifting
                // beyond the valid range, which would permanently block emission.
                let dead_slot = atomicAdd(&counters.dead_count, 1u);
                if (dead_slot < MAX_PARTICLES) {
                    dead_list[dead_slot] = particle_idx;
                } else {
                    // Counter overshot — restore it so emit doesn't see a corrupted value.
                    // This particle index is lost but that's safe: at MAX_PARTICLES dead,
                    // there are no alive particles that could die.
                    atomicSub(&counters.dead_count, 1u);
                }
            }
        }
    }

    // Workgroup completion: the last workgroup to finish writes the indirect draw command.
    // ALL invocations participate in the barrier (including those that had no particle to process).
    // workgroupBarrier() ensures all invocations in this workgroup have completed their atomicAdds.
    // atomicAdd on workgroups_finished provides ordering between workgroups.
    workgroupBarrier();
    storageBarrier();

    if (local_id == 0u) {
        let finished = atomicAdd(&counters.workgroups_finished, 1u);
        let total_wg = (frame_data.total_simulate_count + 63u) / 64u;
        if (finished == total_wg - 1u) {
            // Last workgroup to finish — write the draw command
            let total_alive = atomicLoad(&counters.alive_count);
            draw_command.vertex_count = total_alive * 6u;
            draw_command.instance_count = 1u;
            draw_command.first_vertex = 0u;
            draw_command.first_instance = 0u;
        }
    }
}
