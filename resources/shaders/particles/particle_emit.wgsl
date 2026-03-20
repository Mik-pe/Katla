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

#include "common.wgsl"

// Global resources (Set 0: static buffers)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData, MAX_PARTICLES>;

@group(0) @binding(1)
var<storage, read_write> dead_list: array<u32, MAX_PARTICLES>;

// Alive list (read_write) - contains survivors from previous frame, emit appends new particles here
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

fn sample_emitter_position(config: EmitterConfig, seed: ptr<function, u32>) -> vec3f {
    let shape_type = config.shape;

    if (shape_type == EMITTER_SHAPE_POINT) {
        return config.position;
    }
    else if (shape_type == EMITTER_SHAPE_LINE) {
        let length = config.shape_params.x;
        let t = random_range(seed, -0.5, 0.5);
        return config.position + vec3f(0.0, t * length, 0.0);
    }
    else if (shape_type == EMITTER_SHAPE_CIRCLE) {
        let radius = config.shape_params.x;
        let theta = random_float(seed) * 6.28318530718;
        let r = radius * sqrt(random_float(seed));
        let offset = vec3f(cos(theta) * r, 0.0, sin(theta) * r);
        return config.position + offset;
    }
    else if (shape_type == EMITTER_SHAPE_SPHERE) {
        let radius = config.shape_params.x;
        let theta = random_float(seed) * 6.28318530718;
        let phi = acos(2.0 * random_float(seed) - 1.0);
        let r = radius * pow(random_float(seed), 1.0 / 3.0);
        let x = r * sin(phi) * cos(theta);
        let y = r * sin(phi) * sin(theta);
        let z = r * cos(phi);
        return config.position + vec3f(x, y, z);
    }
    else if (shape_type == EMITTER_SHAPE_BOX) {
        let width = config.shape_params.x;
        let height = config.shape_params.y;
        let depth = config.shape_params.z;
        let x = random_range(seed, -width * 0.5, width * 0.5);
        let y = random_range(seed, -height * 0.5, height * 0.5);
        let z = random_range(seed, -depth * 0.5, depth * 0.5);
        return config.position + vec3f(x, y, z);
    }
    else {
        return config.position;
    }
}

fn emit_particle(particle_idx: u32, emitter_idx: u32, seed: ptr<function, u32>) -> ParticleData {
    let emitter = emitters[emitter_idx];

    var particle: ParticleData;

    particle.position = sample_emitter_position(emitter, seed);

    let lifetime_var = emitter.base_lifetime * emitter.lifetime_variation;
    particle.lifetime = random_range(seed, emitter.base_lifetime - lifetime_var, emitter.base_lifetime + lifetime_var);

    let cone_angle = emitter.velocity_cone_angle;
    let theta = random_float(seed) * 6.28318530718;
    let phi = random_float(seed) * cone_angle;

    let forward = normalize(emitter.velocity_direction);

    let abs_forward = abs(forward);
    let up = select(vec3f(0.0, 1.0, 0.0), vec3f(0.0, 0.0, 1.0), abs_forward.y > abs_forward.z);

    let right = normalize(cross(forward, up));
    let local_up = cross(right, forward);

    let dir_in_cone = normalize(
        forward * cos(phi) +
        right * sin(theta) * sin(phi) +
        local_up * cos(theta) * sin(phi)
    );

    let speed_var = emitter.velocity_magnitude * 0.5;
    let speed = random_range(seed, emitter.velocity_magnitude - speed_var, emitter.velocity_magnitude + speed_var);

    particle.velocity = dir_in_cone * speed;

    let scale_var = emitter.base_scale * emitter.scale_variation;
    particle.scale = random_range(seed, emitter.base_scale - scale_var, emitter.base_scale + scale_var);
    particle.scale = max(particle.scale, 0.001);

    let color_var = emitter.color_variation;
    particle.color = vec4f(
        random_range(seed, emitter.color.r - color_var, emitter.color.r + color_var),
        random_range(seed, emitter.color.g - color_var, emitter.color.g + color_var),
        random_range(seed, emitter.color.b - color_var, emitter.color.b + color_var),
        random_range(seed, emitter.color.a - color_var, emitter.color.a + color_var)
    );
    particle.color = clamp(particle.color, vec4f(0.0), vec4f(1.0));

    particle.emitter_index = emitter_idx;

    return particle;
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;

    if (idx >= frame_data.total_emit_count) { return; }
    if (frame_data.emitter_count == 0u) { return; }

    let wg_id = idx / 256u;
    let local_id = idx % 256u;
    let emitter_idx = (wg_id + local_id) % frame_data.emitter_count;

    if (emitter_idx >= MAX_EMITTERS) {
        return;
    }

    let original_dead_count = atomicSub(&counters.dead_count, 1u);

    if (original_dead_count == 0u || original_dead_count > MAX_PARTICLES) {
        atomicAdd(&counters.dead_count, 1u);
        return;
    }

    let dead_slot = original_dead_count - 1u;
    let particle_idx = dead_list[dead_slot];

    if (particle_idx >= MAX_PARTICLES) {
        atomicAdd(&counters.dead_count, 1u);
        return;
    }

    var seed = frame_data.random_seed + idx * 7u;
    var new_particle = emit_particle(particle_idx, emitter_idx, &seed);

    particles[particle_idx] = new_particle;

    let write_slot = atomicAdd(&counters.emit_count, 1u);

    alive_list[write_slot] = particle_idx;
}
