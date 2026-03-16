// Modern Particle Rendering - Vertex + Fragment Shaders
//
// Renders particles as camera-facing billboards in world space.

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

// Particle data buffer (Set 0, Binding 0)
@group(0) @binding(0)
var<storage, read> particles: array<ParticleData, MAX_PARTICLES>;

// Alive particle index list (Set 0, Binding 2)
// Note: We use binding 2 to match the compute shader's alive_current binding
@group(0) @binding(2)
var<storage, read> alive_list: array<u32, MAX_PARTICLES>;

// Frame uniforms (Set 1, Binding 0)
// This uses the same Set 1 binding 0 as the compute shader for frame data
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
    @builtin(vertex_index) vertex_id: u32,
) -> VertexOutput {
    var out: VertexOutput;

    // Calculate particle index and corner
    let particle_index = alive_list[vertex_id / 6u];
    let particle = particles[particle_index];
    let corner = get_quad_corner(vertex_id);

    out.uv = corner;
    out.color = particle.color;

    // Use actual camera matrices from frame data
    let view = frame_data.view;
    let proj = frame_data.proj;

    // Extract camera right/up vectors from view matrix
    let view_right = vec3f(view[0][0], view[1][0], view[2][0]);
    let view_up = vec3f(view[0][1], view[1][1], view[2][1]);

    // Billboard size in world units (use particle scale)
    let half_size = particle.scale * 0.5;
    let billboard_offset = (corner.x * view_right + corner.y * view_up) * half_size;

    // Final world position
    let world_pos = particle.position + billboard_offset;

    // Transform to clip space using actual camera
    let view_pos = view * vec4f(world_pos, 1.0);
    out.clip_position = proj * view_pos;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // DEBUG: Make particles super visible
    let uv = in.uv;

    // Simple circle with sharp edge for visibility
    let dist = length(uv);

    // Discard outside circle
    if (dist > 1.0) {
        discard;
    }

    // Solid color, no transparency for debugging
    return in.color;
}
