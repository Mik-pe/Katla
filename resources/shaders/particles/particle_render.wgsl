// Modern Particle Rendering - Vertex + Fragment Shaders
//
// Renders particles as camera-facing billboards in world space.
//
// Descriptor Set Layout:
// - Set 0: Particle system buffers (particles, alive_list)
// - Set 1: Frame uniforms (view/proj matrices) - from renderer storage descriptor set

#include "common.wgsl"
#include "../common/frame_uniforms.wgsl"

// Particle data buffer (Set 0, Binding 0)
@group(0) @binding(0)
var<storage, read> particles: array<ParticleData, MAX_PARTICLES>;

// Dead particle list (Set 0, Binding 1) - unused in render but must match layout
@group(0) @binding(1)
var<storage, read> dead_list: array<u32, MAX_PARTICLES>;

// Alive particle index list (Set 0, Binding 2)
@group(0) @binding(2)
var<storage, read> alive_list: array<u32, MAX_PARTICLES>;

// Alive list next (Set 0, Binding 3) - unused in render but must match layout
@group(0) @binding(3)
var<storage, read> alive_list_next: array<u32, MAX_PARTICLES>;

// Counters (Set 0, Binding 4) - unused in render but must match layout
@group(0) @binding(4)
var<storage, read> counters: ParticleCounters;

// Frame uniforms (Set 1, Binding 0) - from renderer storage descriptor set
@group(1) @binding(0)
var<storage, read> frame_uniforms: FrameUniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

fn get_quad_corner(vertex_id: u32) -> vec2f {
    let corners = array<vec2f, 6>(
        vec2f(-1.0,  1.0),  // TL (0)
        vec2f( 1.0,  1.0),  // TR (1)
        vec2f(-1.0, -1.0),  // BL (2)
        vec2f(-1.0, -1.0),  // BL (3)
        vec2f( 1.0,  1.0),  // TR (4)
        vec2f( 1.0, -1.0),  // BR (5)
    );
    return corners[vertex_id % 6u];
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_id: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let particle_index = alive_list[vertex_id / 6u];
    let particle = particles[particle_index];
    let corner = get_quad_corner(vertex_id);

    out.uv = corner;
    out.color = particle.color;

    let view = frame_uniforms.view;
    let proj = frame_uniforms.proj;

    let view_right = vec3f(view[0][0], view[1][0], view[2][0]);
    let view_up = vec3f(view[0][1], view[1][1], view[2][1]);

    let half_size = particle.scale * 0.5;
    let billboard_offset = (corner.x * view_right + corner.y * view_up) * half_size;

    let world_pos = particle.position + billboard_offset;

    let view_pos = view * vec4f(world_pos, 1.0);
    out.clip_position = proj * view_pos;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let dist = length(in.uv);

    if (dist > 1.0) {
        discard;
    }

    return in.color;
}
