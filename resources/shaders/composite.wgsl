// Compositing shader for multi-viewport rendering.
//
// Fullscreen pass that samples from multiple viewport textures and composites
// them onto the final output. Supports:
// - Up to 8 simultaneous viewports
// - Per-viewport positioning via rectangles
// - Alpha blending for overlapping viewports
// - Proper depth ordering (reverse iteration for topmost-last)
//
// Viewport rectangles are passed via objects[i].base_color (Set 0, Binding 1).
// Each object uniform's base_color stores [x, y, x+width, y+height].

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

fn pixel_in_rect(pixel_pos: vec2f, rect: vec4f) -> bool {
    return pixel_pos.x >= rect.x &&
           pixel_pos.x <= rect.z &&
           pixel_pos.y >= rect.y &&
           pixel_pos.y <= rect.w;
}

@fragment
fn fs_main(@builtin(position) clip_position: vec4f, @location(0) uv: vec2f) -> @location(0) vec4f {
    let screen_size = frame_data.compositing.xy;
    let viewport_count = u32(frame_data.compositing.z);

    let pixel_pos = uv * screen_size;

    var result = vec4f(0.0, 0.0, 0.0, 0.0);

    if (viewport_count == 0u) {
        return result;
    }

    // Iterate viewports in reverse order (topmost drawn first)
    for (var i: i32 = i32(viewport_count) - 1; i >= 0; i--) {
        let rect = objects[u32(i)].base_color;

        if (!pixel_in_rect(pixel_pos, rect)) {
            continue;
        }

        // Map pixel position to local UV within the viewport rectangle
        let vp_size = vec2f(rect.z - rect.x, rect.w - rect.y);
        let local_uv = (pixel_pos - vec2f(rect.x, rect.y)) / vp_size;

        let viewport_color = textureSample(viewportTextures[u32(i)], shared_sampler, local_uv);

        if (viewport_color.a >= 0.95) {
            return viewport_color;
        }

        result = mix(result, viewport_color, viewport_color.a);
    }

    return result;
}
