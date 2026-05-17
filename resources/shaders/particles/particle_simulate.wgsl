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

// Alive list next (write) - surviving particles written here.
// On the CPU side, binding 3 is pointed at alive[(frame+1)%2] via descriptor update.
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

fn simulate_particle(particle: ptr<function, ParticleData>, delta_time: f32) {
    let emitter = emitters[(*particle).emitter_index];

    if (emitter.kill_all != 0u) {
        (*particle).lifetime = 0.0;
        return;
    }

    (*particle).lifetime -= delta_time;

    if ((*particle).lifetime > 0.0) {

        (*particle).position += (*particle).velocity * delta_time;
        (*particle).velocity.y += emitter.gravity * delta_time;

        let age = emitter.base_lifetime - (*particle).lifetime;
        let life_ratio = (*particle).lifetime / emitter.base_lifetime;

        if (emitter.turbulence_strength > 0.0) {
            let freq = emitter.turbulence_frequency;
            let phase = age * freq;

            let wave_x = sin(phase + (*particle).position.y * 0.5) * cos(phase * 0.7);
            let wave_z = cos(phase * 1.3 + (*particle).position.x * 0.5) * sin(phase * 0.5);

            (*particle).velocity.x += wave_x * emitter.turbulence_strength * delta_time;
            (*particle).velocity.z += wave_z * emitter.turbulence_strength * delta_time;
        }

        let fade_in = clamp(age / 0.2, 0.0, 1.0);
        let fade_out = clamp(life_ratio / 0.3, 0.0, 1.0);

        (*particle).color.a = fade_in * fade_out;
    }
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;
    let local_id = idx % 64u;

    // Use emit_count from counters (set by emit pass) rather than frame_data.total_simulate_count.
    // emit_count reflects the actual number of particles in the alive list:
    //   - When emit ran: emit_count = cached_alive_count + actual_emissions
    //   - When emit was skipped: emit_count = cached_alive_count (set by reset_simulate_counters)
    // This prevents processing stale alive_list entries when the dead pool is exhausted
    // and fewer particles were emitted than requested.
    let total_particles = counters.emit_count;

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
        atomicAdd(&counters.workgroups_finished, 1u);
    }
}
