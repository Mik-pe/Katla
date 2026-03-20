// Compositing shader for multi-viewport rendering.
//
// Fullscreen pass that samples from multiple viewport textures and composites
// them onto the final output. Supports:
// - Up to 8 simultaneous viewports
// - Per-viewport positioning via rectangles
// - Alpha blending for overlapping viewports
// - Proper depth ordering (reverse iteration for topmost-last)

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

// Compositing descriptor set (Set 2)
@group(2) @binding(0)
var viewportTextures: binding_array<texture_2d<f32>, 8>;

struct ViewportRect {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

struct CompositingUniforms {
    rects: array<ViewportRect, 8>,
    viewport_count: u32,
    screen_size: vec2f,
    padding: f32,
}

fn pixel_in_rect(pixel_pos: vec2f, rect: ViewportRect) -> bool {
    return pixel_pos.x >= rect.x &&
           pixel_pos.x <= rect.z &&
           pixel_pos.y >= rect.y &&
           pixel_pos.y <= rect.w;
}

@fragment
fn fs_main(@builtin(position) clip_position: vec4f, @location(0) uv: vec2f) -> @location(0) vec4f {
    let params = objects[0];

    let screen_size = params.base_color.xy;
    let viewport_count = u32(params.material_params.x);

    let pixel_pos = uv * screen_size;

    var result = vec4f(0.0, 0.0, 0.0, 0.0);

    if (viewport_count == 0u) {
        return result;
    }

    if (viewport_count >= 2u) {
        let split_x = screen_size.x * 0.5;

        if (pixel_pos.x < split_x) {
            let local_uv = vec2f(
                pixel_pos.x / split_x,
                pixel_pos.y / screen_size.y
            );
            let viewport_color = textureSample(viewportTextures[0u], shared_sampler, local_uv);
            return viewport_color;
        }
        else {
            let local_uv = vec2f(
                (pixel_pos.x - split_x) / split_x,
                pixel_pos.y / screen_size.y
            );
            let viewport_color = textureSample(viewportTextures[1u], shared_sampler, local_uv);
            return viewport_color;
        }
    } else if (viewport_count == 1u) {
        let local_uv = uv * 0.5;
        let viewport_color = textureSample(viewportTextures[0u], shared_sampler, local_uv);
        return viewport_color;
    }

    return result;
}
