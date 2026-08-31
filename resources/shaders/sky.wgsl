// Sky shader - Camera-relative procedural sky
//
// Renders a fullscreen triangle with a procedural sky gradient.
// Uses inverse view-projection matrix to convert screen coords to world rays.

#include <frame_uniforms.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) ndc_pos: vec2f,
}

const ZENITH_COLOR = vec3f(0.3, 0.55, 1.2);
const HORIZON_COLOR = vec3f(0.9, 0.95, 1.1);
const GROUND_COLOR = vec3f(0.4, 0.45, 0.5);

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    let uv = vec2f(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );
    let ndc = uv * 2.0 - 1.0;

    // The scene projection stores geometry Y-flipped in the render target
    // relative to raw NDC (the UI's texture display undoes it). The sky
    // bypasses the projection, so it must flip Y itself or it displays
    // upside-down: the below-horizon gradient lands above the horizon and
    // reads as a pale band at the top of the viewport.
    out.clip_position = vec4f(ndc.x, -ndc.y, 0.0, 1.0);
    out.ndc_pos = ndc;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let ndc = vec4f(in.ndc_pos, 0.0, 1.0);
    let world_pos = frame_data.inv_view_proj * ndc;

    let w = max(abs(world_pos.w), 1e-6);
    let world_dir = normalize(world_pos.xyz / w);

    let up = world_dir.y;

    var sky_color: vec3f;

    if (up > 0.0) {
        let t = pow(up, 0.7);
        sky_color = mix(HORIZON_COLOR, ZENITH_COLOR, t);
    } else {
        let t = pow(1.0 + up, 0.5);
        sky_color = mix(HORIZON_COLOR, GROUND_COLOR, t);
    }

    let sun_dir = normalize(frame_data.light_direction.xyz);
    let sun_dot = max(0.0, dot(world_dir, sun_dir));
    let sun_glow = pow(sun_dot, 256.0) * 8.0;
    let sun_halo = pow(sun_dot, 8.0) * 0.5;
    sky_color = sky_color + frame_data.light_color.rgb * (sun_glow + sun_halo) * frame_data.light_intensity.x;

    return vec4f(sky_color, 1.0);
}
