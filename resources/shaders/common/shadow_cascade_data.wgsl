// Shared shadow cascade data structures.
// Used by both shadow depth rendering and shadow sampling shaders.

struct ShadowCascadeData {
    view_proj: mat4x4f,
    split_distance: f32,
    texel_size: f32,
    _pad: vec2f,
}
