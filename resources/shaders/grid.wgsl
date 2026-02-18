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

// Frame-level uniforms (shared across all passes)
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
}

// Per-object uniforms (not used by grid, but required for descriptor compatibility)
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,
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

struct FragmentOutput {
    @location(0) color: vec4f,
    @builtin(frag_depth) depth: f32,
}

// Grid colors - subtle but visible
const GRID_COLOR_MAJOR: vec3f = vec3f(0.5, 0.5, 0.55);   // Major grid lines (every 10 units)
const GRID_COLOR_MINOR: vec3f = vec3f(0.3, 0.3, 0.35);   // Minor grid lines

// Grid settings - 1 unit = 1 meter, thin subtle lines
const GRID_SIZE_MINOR: f32 = 1.0;      // Minor grid spacing (1m)
const GRID_SIZE_MAJOR: f32 = 10.0;     // Major grid spacing (10m)
const LINE_WIDTH_MINOR: vec2f = vec2f(0.01, 0.01);   // ~1cm lines
const LINE_WIDTH_MAJOR: vec2f = vec2f(0.02, 0.02);   // ~2cm lines
const AA_PIXELS: f32 = 1.5;            // Anti-aliasing width in pixels
const FADE_START: f32 = 50.0;          // Distance where grid starts fading out
const FADE_END: f32 = 100.0;           // Distance where grid fully fades

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

    // Z=0.0 (near plane for depth testing), W=1.0
    // Grid will be depth-tested against geometry
    out.clip_position = vec4f(pos, 0.0001, 1.0);
    out.ndc_pos = pos;

    return out;
}

/// Ben Golus's "Best Darn Grid Shader" algorithm
/// Returns grid intensity (0.0 = no line, 1.0 = full line)
fn grid_function(uv: vec2f, line_width: vec2f) -> f32 {
    // Calculate screen-space derivatives using length (more accurate than fwidth)
    let uv_ddx = dpdx(uv);
    let uv_ddy = dpdy(uv);
    let uv_deriv = vec2f(length(vec2f(uv_ddx.x, uv_ddy.x)), length(vec2f(uv_ddx.y, uv_ddy.y)));

    // Handle line inversion for widths > 0.5 (per axis)
    let invert_line = line_width > vec2f(0.5, 0.5);
    let target_width = select(line_width, vec2f(1.0, 1.0) - line_width, invert_line);

    // Clamp draw width to prevent aliasing at distance
    let draw_width = clamp(target_width, uv_deriv, vec2f(0.5, 0.5));

    // Anti-aliasing width (configurable pixels)
    let line_aa = uv_deriv * AA_PIXELS;

    // Transform UV to triangle wave centered on grid lines
    var grid_uv = abs(fract(uv) * 2.0 - 1.0);
    grid_uv = select(vec2f(1.0, 1.0) - grid_uv, grid_uv, invert_line);

    // Draw lines using smoothstep for anti-aliasing
    var grid2 = smoothstep(draw_width + line_aa, draw_width - line_aa, grid_uv);

    // Phone-wire AA: fade based on intended vs actual width
    grid2 = grid2 * clamp(target_width / draw_width, vec2f(0.0, 0.0), vec2f(1.0, 1.0));

    // Moiré suppression: fade to target_width when grid cells < 1 pixel
    // Starts at uv_deriv=0.5 (lines merge), finishes at uv_deriv=1.0 (cells < pixel)
    grid2 = mix(grid2, target_width, clamp(uv_deriv * 2.0 - 1.0, vec2f(0.0, 0.0), vec2f(1.0, 1.0)));

    // Invert back if we were drawing inverted lines
    grid2 = select(grid2, vec2f(1.0, 1.0) - grid2, invert_line);

    // Combine X and Y axes using premultiplied alpha blend
    return mix(grid2.x, 1.0, grid2.y);
}

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    // Convert NDC to world space direction using inverse VP
    let ndc = vec4f(in.ndc_pos, 1.0, 1.0);
    let world_pos = frame_data.inv_view_proj * ndc;

    // Safe division - avoid divide by zero
    let w = max(abs(world_pos.w), 1e-6);
    let world_dir = normalize(world_pos.xyz / w);

    // Camera position
    let cam_pos = frame_data.camera_position.xyz;

    // Ray-plane intersection with XZ plane at Y=0
    let denominator = world_dir.y;

    // If ray doesn't hit the grid plane, output transparent with far depth
    if (abs(denominator) < 1e-6) {
        var output: FragmentOutput;
        output.color = vec4f(0.0, 0.0, 0.0, 0.0);
        output.depth = 1.0;
        return output;
    }

    let t = -cam_pos.y / denominator;

    // If intersection is behind camera (looking up), output transparent
    if (t < 0.0) {
        var output: FragmentOutput;
        output.color = vec4f(0.0, 0.0, 0.0, 0.0);
        output.depth = 1.0;
        return output;
    }

    // Calculate intersection point on XZ plane
    let intersection = cam_pos + t * world_dir;
    let grid_pos = intersection.xz;

    // Distance from camera for optional fade
    let distance = length(intersection - cam_pos);
    let distance_fade = 1.0 - smoothstep(FADE_START, FADE_END, distance);

    // Calculate grid UVs (world XZ coordinates divided by grid size)
    let uv_minor = grid_pos / GRID_SIZE_MINOR;
    let uv_major = grid_pos / GRID_SIZE_MAJOR;

    // Calculate grid intensities using Ben Golus's algorithm
    // Returns 0.0 (no line/background) to 1.0 (full line)
    // The algorithm naturally handles Moiré suppression and AA via alpha
    let grid_minor = grid_function(uv_minor, LINE_WIDTH_MINOR);
    let grid_major = grid_function(uv_major, LINE_WIDTH_MAJOR);

    // Combine minor and major grids - major lines take priority
    let combined_intensity = max(grid_minor * 0.5, grid_major);

    // Blend between minor and major line colors based on their contributions
    let minor_contribution = grid_minor * 0.5 * (1.0 - grid_major);
    let major_contribution = grid_major;
    let total = minor_contribution + major_contribution;

    let line_color = select(
        (GRID_COLOR_MINOR * minor_contribution + GRID_COLOR_MAJOR * major_contribution) / total,
        GRID_COLOR_MINOR,
        total < 0.001
    );

    // Grid intensity becomes alpha, with optional distance fade
    let final_alpha = combined_intensity * distance_fade;

    // Calculate correct clip-space depth for the intersection point
    let world_intersection = vec4f(intersection, 1.0);
    let clip_pos = frame_data.proj * frame_data.view * world_intersection;
    let corrected_depth = clip_pos.z / clip_pos.w;

    var output: FragmentOutput;
    output.color = vec4f(line_color, final_alpha);
    output.depth = corrected_depth;
    return output;
}
