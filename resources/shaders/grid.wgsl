// Grid shader - Ben Golus's "Best Darn Grid Shader"
//
// Renders an infinite grid on the XZ plane at Y=0 using fullscreen triangle.
// Uses ray-plane intersection to determine world position, then applies
// the "Best Darn Grid Shader" algorithm for anti-aliased, perspective-correct grid lines.
//
// Only renders the grid LINES - background areas are discarded so sky/ground shows through.
// Renders AFTER geometry so objects properly occlude the grid.
//
// Reference: https://bgolus.medium.com/the-best-darn-grid-shader-yet-727f9278b9d8

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

struct FragmentOutput {
    @location(0) color: vec4f,
    @builtin(frag_depth) depth: f32,
}

const GRID_COLOR_MAJOR: vec3f = vec3f(0.5, 0.5, 0.55);
const GRID_COLOR_MINOR: vec3f = vec3f(0.3, 0.3, 0.35);

const GRID_SIZE_MINOR: f32 = 1.0;
const GRID_SIZE_MAJOR: f32 = 10.0;
const LINE_WIDTH_MINOR: vec2f = vec2f(0.01, 0.01);
const LINE_WIDTH_MAJOR: vec2f = vec2f(0.02, 0.02);
const AA_PIXELS: f32 = 1.5;
const FADE_START: f32 = 50.0;
const FADE_END: f32 = 100.0;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    let uv = vec2f(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );
    let ndc = uv * 2.0 - 1.0;

    out.clip_position = vec4f(ndc, 0.0001, 1.0);
    out.ndc_pos = ndc;

    return out;
}

fn grid_function(uv: vec2f, line_width: vec2f) -> f32 {
    let uv_ddx = dpdx(uv);
    let uv_ddy = dpdy(uv);
    let uv_deriv = vec2f(length(vec2f(uv_ddx.x, uv_ddy.x)), length(vec2f(uv_ddx.y, uv_ddy.y)));

    let invert_line = line_width > vec2f(0.5, 0.5);
    let target_width = select(line_width, vec2f(1.0, 1.0) - line_width, invert_line);

    let draw_width = clamp(target_width, uv_deriv, vec2f(0.5, 0.5));

    let line_aa = uv_deriv * AA_PIXELS;

    var grid_uv = abs(fract(uv) * 2.0 - 1.0);
    grid_uv = select(vec2f(1.0, 1.0) - grid_uv, grid_uv, invert_line);

    var grid2 = smoothstep(draw_width + line_aa, draw_width - line_aa, grid_uv);

    grid2 = grid2 * clamp(target_width / draw_width, vec2f(0.0, 0.0), vec2f(1.0, 1.0));

    grid2 = mix(grid2, target_width, clamp(uv_deriv * 2.0 - 1.0, vec2f(0.0, 0.0), vec2f(1.0, 1.0)));

    grid2 = select(grid2, vec2f(1.0, 1.0) - grid2, invert_line);

    return mix(grid2.x, 1.0, grid2.y);
}

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    let ndc = vec4f(in.ndc_pos, 1.0, 1.0);
    let world_pos = frame_data.inv_view_proj * ndc;

    let w = max(abs(world_pos.w), 1e-6);
    let world_dir = normalize(world_pos.xyz / w);

    let cam_pos = frame_data.camera_position.xyz;

    let denominator = world_dir.y;

    if (abs(denominator) < 1e-6) {
        var output: FragmentOutput;
        output.color = vec4f(0.0, 0.0, 0.0, 0.0);
        output.depth = 1.0;
        return output;
    }

    let t = -cam_pos.y / denominator;

    if (t < 0.0) {
        var output: FragmentOutput;
        output.color = vec4f(0.0, 0.0, 0.0, 0.0);
        output.depth = 1.0;
        return output;
    }

    let intersection = cam_pos + t * world_dir;
    let grid_pos = intersection.xz;

    let distance = length(intersection - cam_pos);
    let distance_fade = 1.0 - smoothstep(FADE_START, FADE_END, distance);

    let uv_minor = grid_pos / GRID_SIZE_MINOR;
    let uv_major = grid_pos / GRID_SIZE_MAJOR;

    let grid_minor = grid_function(uv_minor, LINE_WIDTH_MINOR);
    let grid_major = grid_function(uv_major, LINE_WIDTH_MAJOR);

    let combined_intensity = max(grid_minor * 0.5, grid_major);

    let minor_contribution = grid_minor * 0.5 * (1.0 - grid_major);
    let major_contribution = grid_major;
    let total = minor_contribution + major_contribution;

    let line_color = select(
        (GRID_COLOR_MINOR * minor_contribution + GRID_COLOR_MAJOR * major_contribution) / total,
        GRID_COLOR_MINOR,
        total < 0.001
    );

    let final_alpha = combined_intensity * distance_fade;

    let world_intersection = vec4f(intersection, 1.0);
    let clip_pos = frame_data.proj * frame_data.view * world_intersection;
    let corrected_depth = clip_pos.z / clip_pos.w;

    var output: FragmentOutput;
    output.color = vec4f(line_color, final_alpha);
    output.depth = corrected_depth;
    return output;
}
