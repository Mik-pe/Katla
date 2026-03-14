// Compositing shader for multi-viewport rendering.
//
// Fullscreen pass that samples from multiple viewport textures and composites
// them onto the final output. Supports:
// - Up to 8 simultaneous viewports
// - Per-viewport positioning via rectangles
// - Alpha blending for overlapping viewports
// - Proper depth ordering (reverse iteration for topmost-last)
//
// # Bindings
//
// ## Set 0: Frame/Object Uniforms (standard across all shaders)
// - Binding 0: FrameUniforms (view/proj matrices, camera, lighting)
// - Binding 1: ObjectUniforms array (per-object data)
//
// ## Set 1: Bindless Textures (shared across all shaders)
// - Binding 0: bindless_textures (up to 4096 textures)
// - Binding 1: shared_sampler
//
// ## Set 2: Compositing Descriptor Set (specific to this shader)
// - Binding 0: viewportTextures (array of 8 texture_2d)
//
// # Viewport Rectangle Uniform
//
// Viewport rectangles are passed via a uniform buffer with this layout:
//
// ```wgsl
// struct ViewportRect {
//     x: f32,  // Left edge in pixels
//     y: f32,  // Top edge in pixels
//     z: f32,  // Right edge in pixels (x + width)
//     w: f32,  // Bottom edge in pixels (y + height)
// }
//
// struct CompositingUniforms {
//     rects: array<ViewportRect, 8>,  // Viewport rectangles
//     viewport_count: u32,             // Number of active viewports
//     padding: vec3<u32>,              // Alignment padding
// }
// ```
//
// The application should update the uniform buffer each frame with the current
// viewport positions. Viewport rectangles are stored as [x, y, x+w, y+h] to
// avoid width/height recalculation in the shader.
//
// # Alpha Blending
//
// Viewports are composited in reverse order (from topmost to bottom):
// - Iterate from viewport_count-1 down to 0
// - If pixel is within viewport rect, sample texture
// - If alpha >= 0.95: opaque, return immediately (no blending)
// - If alpha < 0.95: blend with current result using mix()
//
// This ensures that overlapping viewports blend correctly with proper
// depth ordering. The "topmost" viewport (highest index) is drawn first.
//
// # Example
//
// ```ignore
// // Split-screen layout (2 viewports side-by-side)
// uniforms.rects[0] = vec4f(0.0, 0.0, 960.0, 1080.0);      // Left viewport
// uniforms.rects[1] = vec4f(960.0, 0.0, 1920.0, 1080.0);  // Right viewport
// uniforms.viewport_count = 2u;
//
// // PiP layout (small viewport in corner)
// uniforms.rects[0] = vec4f(0.0, 0.0, 1920.0, 1080.0);     // Fullscreen background
// uniforms.rects[1] = vec4f(1600.0, 800.0, 1900.0, 1050.0); // PiP overlay
// uniforms.viewport_count = 2u;
// ```

//=============================================================================
// Standard Uniforms (shared across all shaders)
//=============================================================================

struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
}

struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,  // x=metallic, y=roughness, z=ao, w=emission_idx
    texture_indices: vec4<u32>,  // x=albedo, y=normal, z=metallic_roughness, w=ao
}

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

//=============================================================================
// Bindless Textures (shared across all shaders)
//=============================================================================

@group(1) @binding(0)
var bindless_textures: binding_array<texture_2d<f32>, 4096>;

@group(1) @binding(1)
var shared_sampler: sampler;

//=============================================================================
// Compositing Textures
//=============================================================================

/// Viewport texture array at set 2, binding 0.
///
/// This is the compositing descriptor set with fixed texture array bindings.
/// Each viewport pass renders to one of these textures, which are then
/// composited together by this shader.
///
/// The CompositingDescriptorSet creates a descriptor set with:
/// - Set 2, Binding 0: texture_2d array (max 8 textures)
///
/// This matches the WGSL binding declaration below.
@group(2) @binding(0)
var viewportTextures: binding_array<texture_2d<f32>, 8>;

//=============================================================================
// Compositing-Specific Uniforms
//=============================================================================

/// Viewport rectangle: [x, y, x+w, y+h] in pixels.
///
/// Stored as vec4 for alignment and efficient shader access.
/// The z and w components are pre-computed (x+width, y+height)
/// to avoid repeated addition in the fragment shader.
struct ViewportRect {
    x: f32,  // Left edge
    y: f32,  // Top edge
    z: f32,  // Right edge (x + width)
    w: f32,  // Bottom edge (y + height)
}

/// Compositing uniform buffer (updated per frame).
///
/// Contains viewport rectangles and count. Must be updated by
/// the application each frame before executing the compositing pass.
///
/// This should be bound at set 2, binding 1 (or via push constants).
/// For this implementation, we'll use object[0] in the storage buffer
/// to pass viewport parameters, similar to how tonemapping passes parameters.
struct CompositingUniforms {
    /// Viewport rectangles (max 8)
    rects: array<ViewportRect, 8>,
    /// Number of active viewports (1-8)
    viewport_count: u32,
    /// Screen size (width, height) in pixels
    screen_size: vec2f,
    /// Padding for 16-byte alignment
    padding: f32,
}

//=============================================================================
// Vertex Shader
//=============================================================================

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
}

/// Fullscreen triangle vertex shader.
///
/// Generates a single triangle that covers the entire screen using
/// the standard fullscreen triangle technique. This avoids the need
/// for vertex buffer input.
///
/// Vertex layout:
/// - Vertex 0: position=(-1, -1), uv=(0, 0)
/// - Vertex 1: position=(3, -1),  uv=(2, 0)
/// - Vertex 2: position=(-1, 3),  uv=(0, 2)
///
/// This generates a large triangle that extends beyond the screen bounds,
/// ensuring full coverage after the perspective divide.
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Generate fullscreen triangle UV coordinates
    let uv = vec2f(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );

    // Convert UV to clip space: [0, 2] -> [-1, 1]
    out.clip_position = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uv;

    return out;
}

//=============================================================================
// Fragment Shader
//=============================================================================

/// Check if a pixel is within a viewport rectangle.
///
/// # Arguments
/// - `pixel_pos`: Pixel position in screen space
/// - `rect`: Viewport rectangle [x, y, x+w, y+h]
///
/// # Returns
/// true if the pixel is within the rectangle (including edges)
fn pixel_in_rect(pixel_pos: vec2f, rect: ViewportRect) -> bool {
    return pixel_pos.x >= rect.x &&
           pixel_pos.x <= rect.z &&
           pixel_pos.y >= rect.y &&
           pixel_pos.y <= rect.w;
}

/// Compositing fragment shader.
///
/// Samples from viewport textures based on pixel position and composites
/// them with alpha blending. Iterates viewports in reverse order (topmost
/// first) to ensure correct blending for overlapping viewports.
///
/// # Algorithm
/// 1. Start with transparent black (0, 0, 0, 0)
/// 2. For each viewport (from top to bottom):
///    a. Check if current pixel is within viewport rect
///    b. If yes, sample texture at local UV coordinates
///    c. If alpha >= 0.95: opaque, return immediately
///    d. If alpha < 0.95: blend with current result
/// 3. Return final compositing color
///
/// # Alpha Blending
/// Uses standard alpha blending: result = mix(result, color, color.a)
/// This implements the "over" operator for Porter-Duff compositing.
/// Compositing fragment shader.
///
/// Samples from viewport textures based on pixel position and composites
/// them with alpha blending. Iterates viewports in reverse order (topmost
/// first) to ensure correct blending for overlapping viewports.
///
/// # Algorithm
/// 1. Start with transparent black (0, 0, 0, 0)
/// 2. For each viewport (from top to bottom):
///    a. Check if current pixel is within viewport rect
///    b. If yes, sample texture at local UV coordinates
///    c. If alpha >= 0.95: opaque, return immediately
///    d. If alpha < 0.95: blend with current result
/// 3. Return final compositing color
///
/// # Alpha Blending
/// Uses standard alpha blending: result = mix(result, color, color.a)
/// This implements the "over" operator for Porter-Duff compositing.
///
/// # Viewport Parameters
/// Viewport rectangles are passed via objects[0].base_color and material_params:
/// - base_color.rgb: Screen size (width, height, unused)
/// - material_params.x: Viewport count
/// - Viewport rectangles: Passed via a separate mechanism (TODO)
///
/// For now, we use a simple hardcoded layout that will be replaced with
/// proper uniform buffer management in the compositing pass implementation.
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Get compositing parameters from objects[0] (fullscreen/post-processing slot)
    let params = objects[0];

    // Extract screen size and viewport count
    // Encoding: base_color.rg = screen_size (width, height)
    //           material_params.x = viewport_count
    let screen_size = params.base_color.xy;
    let viewport_count = u32(params.material_params.x);

    // Convert UV to pixel position
    let pixel_pos = in.uv * screen_size;

    // Start with transparent black
    var result = vec4f(0.0, 0.0, 0.0, 0.0);

    // TODO: Pass viewport rectangles via proper uniform buffer
    // For now, use a simple hardcoded layout for testing
    // This will be replaced when the compositing pass is fully implemented

    // Example: 2-viewport split screen
    // Left viewport: [0, 0, screen_width/2, screen_height]
    // Right viewport: [screen_width/2, 0, screen_width, screen_height]

    if (viewport_count == 0u) {
        // No viewports, return black
        return result;
    }

    // For now, implement a simple 2-viewport split-screen as a proof of concept
    // This will be replaced with proper viewport rectangle handling
    if (viewport_count >= 2u) {
        // Split screen layout
        let split_x = screen_size.x * 0.5;

        // Left viewport (index 0)
        if (pixel_pos.x < split_x) {
            let local_uv = vec2f(
                pixel_pos.x / split_x,
                pixel_pos.y / screen_size.y
            );
            let viewport_color = textureSample(viewportTextures[0u], shared_sampler, local_uv);
            return viewport_color;
        }
        // Right viewport (index 1)
        else {
            let local_uv = vec2f(
                (pixel_pos.x - split_x) / split_x,
                pixel_pos.y / screen_size.y
            );
            let viewport_color = textureSample(viewportTextures[1u], shared_sampler, local_uv);
            return viewport_color;
        }
    } else if (viewport_count == 1u) {
        // Single viewport, fullscreen
        let viewport_color = textureSample(viewportTextures[0u], shared_sampler, in.uv);
        return viewport_color;
    }

    return result;
}
