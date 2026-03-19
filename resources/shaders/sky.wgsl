// Sky shader - Camera-relative procedural sky
//
// Renders a fullscreen triangle with a procedural sky gradient.
// Uses inverse view-projection matrix to convert screen coords to world rays.

// Frame-level uniforms (shared across all passes)
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
    tiles: vec4<u32>,
}

// Per-object uniforms (not used by sky, but required for descriptor compatibility)
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,      // x=metallic, y=roughness, z=ao, w=emission_idx
    texture_indices: vec4<u32>,  // bindless indices (unused in legacy mode)
}

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) ndc_pos: vec2f,
}

// Sky colors - HDR values (can go above 1.0)
const ZENITH_COLOR = vec3f(0.3, 0.55, 1.2);      // Deep blue, slightly brighter
const HORIZON_COLOR = vec3f(0.9, 0.95, 1.1);     // Bright horizon
const GROUND_COLOR = vec3f(0.4, 0.45, 0.5);      // Ground fog

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Fullscreen triangle vertices in NDC
    let positions = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f(3.0, -1.0),
        vec2f(-1.0, 3.0),
    );

    let pos = positions[vertex_index];

    // Set depth to far plane (0.0 in reverse-Z, so geometry appears in front)
    out.clip_position = vec4f(pos, 0.0, 1.0);
    out.ndc_pos = pos;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Convert NDC to world space direction using inverse VP
    // Use Z=0.0 for reverse-Z (far plane)
    let ndc = vec4f(in.ndc_pos, 0.0, 1.0);
    let world_pos = frame_data.inv_view_proj * ndc;

    // Safe division - avoid divide by zero
    let w = max(abs(world_pos.w), 1e-6);
    let world_dir = normalize(world_pos.xyz / w);

    // Use the up component (Y in world space) for gradient
    let up = world_dir.y;

    var sky_color: vec3f;

    if (up > 0.0) {
        // Looking up: horizon to zenith
        let t = pow(up, 0.7);
        sky_color = mix(HORIZON_COLOR, ZENITH_COLOR, t);
    } else {
        // Looking down: ground to horizon
        let t = pow(1.0 + up, 0.5);
        sky_color = mix(HORIZON_COLOR, GROUND_COLOR, t);
    }

    // Add sun glow in the direction of the light (HDR bright)
    let sun_dir = normalize(frame_data.light_direction.xyz);
    let sun_dot = max(0.0, dot(world_dir, sun_dir));
    let sun_glow = pow(sun_dot, 256.0) * 8.0;   // Bright sun disk
    let sun_halo = pow(sun_dot, 8.0) * 0.5;     // Soft halo
    sky_color = sky_color + frame_data.light_color.rgb * (sun_glow + sun_halo) * frame_data.light_intensity.x;

    return vec4f(sky_color, 1.0);
}
