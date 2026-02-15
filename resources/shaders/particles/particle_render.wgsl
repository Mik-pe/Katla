// GPU Particle Rendering Shaders
//
// Renders particles as billboard quads facing the camera.
// Uses instanced rendering where each instance is a particle.
//
// Vertex shader generates a quad per particle (no vertex buffer needed).
// Fragment shader draws soft-edged circles with color from particle data.

// Particle data structure (must match ParticleData in particle_buffer.rs)
struct ParticleData {
    position: vec3f,
    _pad1: f32,
    velocity: vec3f,
    lifetime: f32,
    color: vec4f,
    scale: f32,
    _pad2: vec3f,
}

// Frame uniforms for camera (matches StorageUniforms in renderer)
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
}

// Set 0: Frame uniforms (shared with main renderer)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

// Set 1: Particle buffer
@group(1) @binding(0)
var<storage, read> particles: array<ParticleData>;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

// Quad corners for billboard (6 vertices = 2 triangles)
// Generated in vertex shader based on vertex ID
fn get_corner(vertex_id: u32) -> vec2f {
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
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let particle = particles[instance_idx];

    // Skip dead particles by placing them behind the camera
    if (particle.lifetime <= 0.0) {
        out.clip_position = vec4f(0.0, 0.0, 0.0, -1.0);
        out.uv = vec2f(0.0);
        out.color = vec4f(0.0);
        return out;
    }

    // Get quad corner UV
    let corner = get_corner(vertex_id);
    out.uv = corner;

    // Calculate billboard facing camera
    // Extract right and up vectors from view matrix (camera space)
    let view_right = vec3f(frame_data.view[0][0], frame_data.view[1][0], frame_data.view[2][0]);
    let view_up = vec3f(frame_data.view[0][1], frame_data.view[1][1], frame_data.view[2][1]);

    // Scale the billboard
    let half_size = particle.scale;

    // Calculate world position of this vertex
    let world_offset = (corner.x * view_right + corner.y * view_up) * half_size;
    let world_pos = particle.position + world_offset;

    // Transform to clip space
    out.clip_position = frame_data.proj * frame_data.view * vec4f(world_pos, 1.0);

    // Pass through color and alpha
    out.color = particle.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Discard dead particles
    if (in.color.a <= 0.0) {
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

    // Output color with soft edge (additive blending for fire effect)
    return vec4f(in.color.rgb * in.color.a * alpha, in.color.a * alpha * 0.5);
}
