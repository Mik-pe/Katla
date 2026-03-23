// Particle Validation Compute Shader
//
// GPU-side validation that checks every alive particle against its emitter config.
// Uses atomic counters to count faults — zero readback overhead until the end.
//
// Runs AFTER the simulate pass. Reads the simulate output alive list and
// validates each particle's emitter_index, color, and position consistency.
//
// Descriptor Set Layout:
// Set 0 (Global Resources):
//   Binding 0: Storage buffer (particle data)
//   Binding 1: Storage buffer (alive list — simulate output, i.e. alive_next)
//   Binding 2: Storage buffer (atomic counters — for reading alive_count)
//   Binding 3: Storage buffer (emitter configurations)
//   Binding 4: Storage buffer (validation results — atomic fault counters)
//   Binding 5: Uniform buffer (validation params)

#include "common.wgsl"

// Validation result counters (atomic, 32 bytes)
struct ValidationResults {
    total_checked: atomic<u32>,
    color_mismatches: atomic<u32>,
    velocity_mismatches: atomic<u32>,
    position_mismatches: atomic<u32>,
    // Per-emitter color mismatch counts (up to 16 emitters)
    per_emitter_mismatches: array<atomic<u32>, 16>,
    // First N mismatch details for debugging (16 entries x 3 u32 each = 192 bytes)
    // Format: [particle_index, emitter_index, packed_color_or_velocity]
    // packed_color: R*1000<<20 | G*1000<<10 | B*1000 (approximate)
    // packed_velocity: sign(Vx)<<31 | abs(Vx)*1000<<20 | sign(Vy)<<15 | Vy*1000<<5 | sign(Vz)<<4 | Vz*1000
    mismatch_details: array<u32, 64>,
    mismatch_count: atomic<u32>,
}

// Validation parameters (32 bytes)
struct ValidationParams {
    alive_count: u32,
    emitter_count: u32,
    frame_index: u32,
    max_mismatch_details: u32,
    color_tolerance: f32,
    velocity_tolerance: f32,
    position_tolerance: f32,
    _pad: f32,
}

@group(0) @binding(0)
var<storage, read> particles: array<ParticleData, MAX_PARTICLES>;

@group(0) @binding(1)
var<storage, read> alive_list: array<u32, MAX_PARTICLES>;

@group(0) @binding(2)
var<storage, read> counters: ParticleCounters;

@group(0) @binding(3)
var<storage, read> emitters: array<EmitterConfig, MAX_EMITTERS>;

@group(0) @binding(4)
var<storage, read_write> results: ValidationResults;

@group(0) @binding(5)
var<uniform> params: ValidationParams;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;

    if (idx >= params.alive_count) { return; }

    let particle_idx = alive_list[idx];
    if (particle_idx >= MAX_PARTICLES) { return; }

    let particle = particles[particle_idx];
    let emitter_idx = particle.emitter_index;

    if (emitter_idx >= params.emitter_count) { return; }

    let emitter = emitters[emitter_idx];

    atomicAdd(&results.total_checked, 1u);

    // Check color consistency.
    // With zero color_variation, the particle color should exactly match the emitter color
    // (minus alpha fade applied by simulate). Compare RGB channels only.
    var color_ok = true;
    let tol = params.color_tolerance;

    if (abs(particle.color.r - emitter.color.r) > tol ||
        abs(particle.color.g - emitter.color.g) > tol ||
        abs(particle.color.b - emitter.color.b) > tol) {
        color_ok = false;
    }

    if (!color_ok) {
        atomicAdd(&results.color_mismatches, 1u);
        if (emitter_idx < 16u) {
            atomicAdd(&results.per_emitter_mismatches[emitter_idx], 1u);
        }

        // Record mismatch detail (up to max_mismatch_details)
        let detail_slot = atomicAdd(&results.mismatch_count, 1u);
        if (detail_slot < params.max_mismatch_details) {
            let base = detail_slot * 4u;
            results.mismatch_details[base] = particle_idx;
            results.mismatch_details[base + 1u] = emitter_idx;
            // Pack color channels as integers for debugging
            results.mismatch_details[base + 2u] = bitcast<u32>(particle.color.r * 10000.0);
            results.mismatch_details[base + 3u] = bitcast<u32>(particle.color.g * 10000.0);
        }
    }

    // Check velocity consistency.
    // With zero cone_angle and zero gravity, velocity should be purely in the
    // emitter's velocity_direction. With zero turbulence, there should be no
    // lateral drift.
    var velocity_ok = true;
    let vtol = params.velocity_tolerance;

    // Only check if velocity tolerance is set (> 0)
    if (vtol > 0.0) {
        // Check for unexpected lateral velocity components
        // With cone_angle=0, all velocity should be along velocity_direction
        let forward = normalize(emitter.velocity_direction);
        let vel_along = dot(particle.velocity, forward);
        let lateral_sq = dot(particle.velocity, particle.velocity) - vel_along * vel_along;

        if (lateral_sq > vtol * vtol) {
            velocity_ok = false;
        }
    }

    if (!velocity_ok) {
        atomicAdd(&results.velocity_mismatches, 1u);
    }

    // Check position consistency.
    // Particle should be near its emitter's position (within some tolerance).
    // With upward velocity and no gravity, particle drifts along velocity_direction.
    // We check that the particle is on the correct side relative to its emitter.
    var position_ok = true;
    let ptol = params.position_tolerance;

    if (ptol > 0.0) {
        // Simple check: distance from emitter should be reasonable
        let to_particle = particle.position - emitter.position;
        let dist_sq = dot(to_particle, to_particle);

        // Max expected distance: velocity * lifetime (e.g., 5.0 * 1.5 = 7.5)
        // Use a generous tolerance
        let max_dist = emitter.velocity_magnitude * emitter.base_lifetime + ptol;
        if (dist_sq > max_dist * max_dist) {
            position_ok = false;
        }
    }

    if (!position_ok) {
        atomicAdd(&results.position_mismatches, 1u);
    }
}
