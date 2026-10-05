// Screen-space contact shadow raymarching.
//
// Short-range (0.5-1.5m) raymarch from the surface toward the light direction,
// checking the depth buffer for nearby occluders that CSM cascades might miss
// (e.g., grass, small debris, thin geometry).
//
// Uses the depth buffer via bindless texture (registered as texture_2d<f32>).
// Depth is read as a single-channel float from the R component.

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>

// Contact shadow parameters (passed via objects[0].material_params for fullscreen,
// or hardcoded for PBR integration).
const CONTACT_SHADOW_STEPS: u32 = 6u;
const CONTACT_SHADOW_MAX_DIST: f32 = 1.0;
const CONTACT_SHADOW_THICKNESS: f32 = 0.02;
const CONTACT_SHADOW_JITTER: f32 = 0.1;

// Reconstruct view-space position from screen UV and depth.
fn reconstruct_view_pos(uv: vec2f, depth: f32) -> vec3f {
    // Convert UV from [0,1] to NDC [-1,1]
    let ndc = vec4f(uv * 2.0 - 1.0, depth, 1.0);
    let world_pos = frame_data.inv_view_proj * ndc;
    return world_pos.xyz / world_pos.w;
}

// Sample depth buffer at a screen position (returns linear depth, 0 = near, 1 = far).
// depth_texture_idx is the bindless slot for the current frame's depth texture.
fn sample_depth_buffer(depth_texture_idx: u32, uv: vec2f) -> f32 {
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0; // Out of bounds = far plane
    }
    return textureSample(bindless_textures[depth_texture_idx], shared_sampler, uv).r;
}

// Screen-space contact shadow test.
// Returns 0.0 (fully shadowed) to 1.0 (fully lit).
// world_pos: surface world position
// light_dir: normalized light direction
// view_z: camera-space depth of the fragment
// depth_texture_idx: bindless slot for depth buffer
fn sample_contact_shadow(
    world_pos: vec3f,
    light_dir: vec3f,
    view_z: f32,
    depth_texture_idx: u32,
) -> f32 {
    if (depth_texture_idx == 0u) {
        return 1.0; // No depth texture bound
    }

    // Skip for very distant fragments (contact shadows are short-range)
    if (view_z > 50.0) {
        return 1.0;
    }

    // Jitter ray start to reduce banding
    let jitter = fract(sin(dot(world_pos.xy, vec2f(12.9898, 78.233)) + frame_data.camera_position.x) * 43758.5453);
    let ray_offset = jitter * CONTACT_SHADOW_JITTER * 0.01;

    // March along the light direction in world space
    let step_size = CONTACT_SHADOW_MAX_DIST / f32(CONTACT_SHADOW_STEPS);
    var visibility = 1.0;

    for (var i = 1u; i <= CONTACT_SHADOW_STEPS; i++) {
        let t = f32(i) * step_size + ray_offset;
        let sample_pos = world_pos + light_dir * t;

        // Project sample position to screen space
        let clip = frame_data.proj * frame_data.view * vec4f(sample_pos, 1.0);
        if (clip.w <= 0.0) {
            continue; // Behind camera
        }
        let screen_uv = clip.xy / clip.w * 0.5 + 0.5;
        let sample_depth = clip.z / clip.w;

        // Read scene depth at this screen position
        let scene_depth = sample_depth_buffer(depth_texture_idx, screen_uv);

        // Compare: if scene depth is closer than our ray sample, we hit geometry
        let depth_diff = scene_depth - sample_depth;
        if (depth_diff > 0.0 && depth_diff < CONTACT_SHADOW_THICKNESS) {
            visibility = 0.0;
            break;
        }
    }

    return visibility;
}
