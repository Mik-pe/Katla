// Modern Particle Rendering - Vertex + Fragment Shaders
//
// Renders particles as camera-facing billboards.
// Uses vertex ID to generate quad corners (no vertex buffer needed).
//
// Descriptor Set Layout:
// Set 0 (Global Resources):
//   Binding 0: Storage buffer (particle data)
//   Binding 1: Storage buffer (alive particle index list)
// Set 1 (Per-Frame):
//   Binding 0: Storage buffer (frame uniforms - camera, lights)

const MAX_PARTICLES: u32 = 1048576u;

// Particle data structure
struct ParticleData {
    position: vec3f,
    scale: f32,
    velocity: vec3f,
    lifetime: f32,
    color: vec4f,
}

// Frame uniforms (matches StorageUniforms in renderer)
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
}

// Global particle data
@group(0) @binding(0)
var<storage, read> particles: array<ParticleData, MAX_PARTICLES>;

// Alive particle index list
@group(0) @binding(1)
var<storage, read> alive_list: array<u32, MAX_PARTICLES>;

// Per-frame data (Set 1: shared with main renderer)
@group(1) @binding(0)
var<storage, read> frame_data: FrameUniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

// Quad corners for billboard (6 vertices = 2 triangles)
fn get_quad_corner(vertex_id: u32) -> vec2f {
    // Triangle 1: TL, TR, BL
    // Triangle 2: BL, TR, BR
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
    @builtin(vertex_id) vertex_id: u32,
) -> VertexOutput {
    var out: VertexOutput;

    // Calculate particle index and corner
    let particle_index = vertex_id / 6u;
    let corner = get_quad_corner(vertex_id);

    // Safety check (though GPU should only dispatch valid count)
    if (particle_index >= MAX_PARTICLES) {
        out.clip_position = vec4f(0.0, 0.0, 2.0, 1.0);
        out.uv = vec2f(0.0);
        out.color = vec4f(0.0);
        return out;
    }

    let particle_idx = alive_list[particle_index];
    let particle = particles[particle_idx];

    // Skip dead particles
    if (particle.lifetime <= 0.0) {
        out.clip_position = vec4f(0.0, 0.0, 2.0, 1.0);
        out.uv = vec2f(0.0);
        out.color = vec4f(0.0);
        return out;
    }

    out.uv = corner;
    out.color = particle.color;

    // Extract camera right/up vectors from view matrix
    let view_right = vec3f(
        frame_data.view[0][0],
        frame_data.view[1][0],
        frame_data.view[2][0]
    );
    let view_up = vec3f(
        frame_data.view[0][1],
        frame_data.view[1][1],
        frame_data.view[2][1]
    );

    // Calculate billboard offset in world space
    let half_size = particle.scale * 0.5;
    let billboard_offset = (corner.x * view_right + corner.y * view_up) * half_size;

    // Particle position in world space
    let particle_pos = vec3f(
        particle.position[0],
        particle.position[1],
        particle.position[2]
    );

    // Final world position
    let world_pos = particle_pos + billboard_offset;

    // Transform to clip space
    let view_pos = frame_data.view * vec4f(world_pos, 1.0);
    out.clip_position = frame_data.proj * view_pos;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Discard dead particles
    if (in.color.a <= 0.01) {
        discard;
    }

    let uv = in.uv;

    // Calculate distance from center for soft circular particle
    let dist = length(uv);

    // Soft edge: smooth falloff from center to edge
    let alpha = 1.0 - smoothstep(0.5, 1.0, dist);

    // Discard fully transparent pixels
    if (alpha < 0.01) {
        discard;
    }

    // Output color with soft edge
    // Pre-multiply alpha for proper blending
    let rgb = in.color.rgb * in.color.a * alpha;
    let a = in.color.a * alpha * 0.5;  // Reduce overall alpha for transparency

    return vec4f(rgb, a);
}
