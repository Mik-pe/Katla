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

#include "common.wgsl"

// Global resources (Set 0: static buffers)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData, MAX_PARTICLES>;

@group(0) @binding(1)
var<storage, read_write> dead_list: array<u32, MAX_PARTICLES>;

// Alive list (read) - contains particles to simulate (emitted + survivors)
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
@group(1) @binding(0)
var<uniform> frame_data: FrameData;

fn simulate_particle(particle: ptr<function, ParticleData>, delta_time: f32) {
    (*particle).lifetime -= delta_time;

    if ((*particle).lifetime > 0.0) {
        (*particle).position += (*particle).velocity * delta_time;
        (*particle).velocity.y -= 9.8 * delta_time;
        (*particle).color.a = 1.0;
    }
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;
    let local_id = idx % 64u;

    let total_particles = frame_data.total_simulate_count;

    if (idx < total_particles) {
        let particle_idx = alive_list[idx];

        if (particle_idx < MAX_PARTICLES) {
            var particle = particles[particle_idx];

            simulate_particle(&particle, frame_data.delta_time);

            if (particle.lifetime > 0.0) {
                particles[particle_idx] = particle;

                let survivor_slot = atomicAdd(&counters.alive_count, 1u);

                if (survivor_slot < MAX_PARTICLES) {
                    alive_list_next[survivor_slot] = particle_idx;
                }
            } else {
                let dead_slot = atomicAdd(&counters.dead_count, 1u);
                if (dead_slot < MAX_PARTICLES) {
                    dead_list[dead_slot] = particle_idx;
                } else {
                    atomicSub(&counters.dead_count, 1u);
                }
            }
        }
    }

    storageBarrier();

    if (local_id == 0u) {
        let finished = atomicAdd(&counters.workgroups_finished, 1u);
        let total_wg = (frame_data.total_simulate_count + 63u) / 64u;
        if (finished == total_wg - 1u) {
            let total_alive = atomicLoad(&counters.alive_count);
            draw_command.vertex_count = total_alive * 6u;
            draw_command.instance_count = 1u;
            draw_command.first_vertex = 0u;
            draw_command.first_instance = 0u;
        }
    }
}
