// Screen-space ambient occlusion (SSAO) sampling.
//
// Lightweight SSAO computed per-fragment in the PBR pass.
// Reads the depth buffer via bindless and samples nearby depth values
// to estimate ambient occlusion. Uses 8 hemisphere samples.
//
// This is integrated directly into the PBR shader as a function call,
// avoiding the need for a separate compute pass.

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>

const SSAO_SAMPLES: u32 = 8u;
const SSAO_RADIUS: f32 = 0.5;
const SSAO_BIAS: f32 = 0.025;

fn sample_ssaO(
    screen_uv: vec2f,
    view_pos: vec3f,
    view_normal: vec3f,
    depth_texture_idx: u32,
) -> f32 {
    if (depth_texture_idx == 0u) {
        return 1.0; // No depth texture bound
    }

    // Tangent frame aligned with view
    let view_dir = normalize(-view_pos);
    let bitangent = normalize(cross(view_dir, view_normal));
    let tangent = normalize(cross(view_normal, bitangent));

    let texel_size = 1.0 / vec2f(
        f32(frame_data.tiles.x) * 16.0,
        f32(frame_data.tiles.y) * 16.0,
    );

    var occlusion = 0.0;

    for (var i = 0u; i < SSAO_SAMPLES; i++) {
        // Generate pseudo-random direction in hemisphere
        let angle1 = fract(sin(f32(i + 1u) * 43758.5453 + screen_uv.x * 12.9898 + screen_uv.y * 78.233) * 6.2831);
        let angle2 = mix(0.2, 1.0, fract(sin(f32(i + 1u) * 23421.631 + screen_uv.x * 43.7585) * 3.1415));
        let r = sqrt(angle2);

        let sample_dir = normalize(vec3f(
            cos(angle1) * r,
            sin(angle1) * r,
            sqrt(max(1.0 - r * r, 0.0))
        ));

        // Transform to view space
        let dir_view = sample_dir.x * tangent + sample_dir.y * view_normal + sample_dir.z * bitangent;
        let sample_pos = view_pos + dir_view * SSAO_RADIUS;

        // Project to screen space
        let clip_pos = frame_data.proj * vec4f(sample_pos + frame_data.camera_position.xyz, 1.0);
        if (clip_pos.w <= 0.0) {
            continue;
        }
        let sample_uv = clip_pos.xy / clip_pos.w * 0.5 + 0.5;
        let sample_depth = sample_depth_buffer(depth_texture_idx, sample_uv);

        if (sample_depth >= 1.0) {
            continue;
        }

        // Reconstruct sample view position
        let sample_view = reconstruct_view_pos(sample_uv, sample_depth);
        let delta = sample_view - view_pos;
        let dist = length(delta);
        let range_check = smoothstep(0.0, 1.0, SSAO_RADIUS / max(dist, 0.001));
        occlusion += range_check;
    }

    let ao = occlusion / f32(SSAO_SAMPLES);
    return clamp(1.0 - ao * 0.5, 0.0, 1.0);
}

// Smoothstep helper
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}
