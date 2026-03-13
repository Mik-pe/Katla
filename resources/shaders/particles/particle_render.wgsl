// GPU Particle Rendering Shaders
//
// Renders particles as billboard quads facing the camera.
// Uses instanced rendering where each instance is a particle.
//
// Vertex shader generates a quad per particle (no vertex buffer needed).
// Fragment shader draws soft-edged circles with color from particle data.
//
// Descriptor Set Layout:
// Set 0 (Per-Frame, Shared):
//   Binding 0: Storage buffer (frame uniforms - camera, lights)
// Set 1 (Per-Emitter):
//   Binding 0: Storage buffer (particle data)

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

// Set 1: Particle buffer (per-emitter)
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

    // Only render first 1000 particles
    if (instance_idx >= 1000u) {
        out.clip_position = vec4f(0.0, 0.0, 0.0, -1.0);
        out.uv = vec2f(0.0);
        out.color = vec4f(0.0);
        return out;
    }

    let particle = particles[instance_idx];

    // Skip dead particles
    if (particle.lifetime <= 0.0) {
        out.clip_position = vec4f(0.0, 0.0, 0.0, -1.0);
        out.uv = vec2f(0.0);
        out.color = vec4f(0.0);
        return out;
    }

    let corner = get_corner(vertex_id);
    out.uv = corner;

    // Extract camera right/up vectors from view matrix
    let view_right = vec3f(frame_data.view[0][0], frame_data.view[1][0], frame_data.view[2][0]);
    let view_up = vec3f(frame_data.view[0][1], frame_data.view[1][1], frame_data.view[2][1]);

    // Calculate billboard offset in world space
    let half_size = particle.scale;
    let billboard_offset = (corner.x * view_right + corner.y * view_up) * half_size;

    // Particle position in world space
    let particle_pos = vec3f(particle.position[0], particle.position[1], particle.position[2]);

    // Final world position
    let world_pos = particle_pos + billboard_offset;

    // Transform to view space, then clip space
    let view_pos = frame_data.view * vec4f(world_pos, 1.0);
    out.clip_position = frame_data.proj * view_pos;

    // Pass through particle color
    out.color = particle.color;

    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    // Discard dead particles
    if (input.color.a <= 0.0) {
        discard;
    }

    let uv = input.uv;

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
    let rgb = input.color.rgb * input.color.a * alpha;
    let a = input.color.a * alpha * 0.5;  // Reduce overall alpha for transparency

    return vec4f(rgb, a);
}
