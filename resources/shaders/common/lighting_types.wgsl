// Shared lighting types and constants (must match Rust side).

const TILE_SIZE: u32 = 16u;
const MAX_LIGHTS_PER_TILE: u32 = 128u;
const MAX_POINT_LIGHTS: u32 = 256u;

// Point light data (must match PointLightGPU in Rust)
struct PointLightGPU {
    position: vec3f,
    range: f32,
    color: vec3f,
    intensity: f32,
}
