// UI fragment shader
// Samples texture and applies vertex color
// Supports multiple textures via UV encoding:
//   - UV.x < 1.0: Sample from font atlas
//   - UV.x >= 1.0: Sample from viewport texture (UV.x - 1.0 gives actual UV)

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

@group(0) @binding(1)
var font_atlas: texture_2d<f32>;

@group(0) @binding(2)
var font_sampler: sampler;

@group(0) @binding(3)
var viewport_texture: texture_2d<f32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    var tex_color: vec4f;

    if (in.uv.x >= 1.0) {
        // Sample from viewport texture (shift UV back)
        let viewport_uv = vec2f(in.uv.x - 1.0, in.uv.y);
        tex_color = textureSample(viewport_texture, font_sampler, viewport_uv);
    } else {
        // Sample from font atlas
        tex_color = textureSample(font_atlas, font_sampler, in.uv);
    }

    // Multiply texture with vertex color
    return in.color * tex_color;
}
