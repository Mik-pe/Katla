// Forward+ tile-based light culling compute shader.
//
// Divides the screen into 16x16 pixel tiles and determines which point lights
// affect each tile by testing screen-space AABB overlap of projected light spheres.
//
// Uses a proper view-space sphere projection that accounts for the full
// screen-space extent of the light (not just the depth direction), ensuring
// correct culling regardless of camera orientation.
//
// Descriptor Sets:
//   Set 0, Binding 0: point_lights (storage buffer, read)
//   Set 0, Binding 1: tile_light_indices (storage buffer, read_write)
//   Set 0, Binding 2: tile_light_counts (storage buffer, read_write)
//   Set 0, Binding 3: frame_data (uniform buffer, read)

const TILE_SIZE: u32 = 16u;
const MAX_LIGHTS_PER_TILE: u32 = 128u;
const MAX_POINT_LIGHTS: u32 = 256u;

// Point light data (32 bytes, must match PointLightGPU in Rust)
struct PointLightGPU {
    position: vec3f,
    range: f32,
    color: vec3f,
    intensity: f32,
}

// Frame data for light culling (160 bytes, must match LightCullFrameData in Rust)
struct LightCullFrameData {
    view_matrix: mat4x4f,
    proj_matrix: mat4x4f,
    light_count: u32,
    tiles_x: u32,
    tiles_y: u32,
    screen_width: u32,
    screen_height: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0)
var<storage, read> point_lights: array<PointLightGPU, MAX_POINT_LIGHTS>;

@group(0) @binding(1)
var<storage, read_write> tile_light_indices: array<u32>;

@group(0) @binding(2)
var<storage, read_write> tile_light_counts: array<atomic<u32>>;

@group(0) @binding(3)
var<uniform> frame_data: LightCullFrameData;

/// Convert clip-space XY to pixel coordinates.
/// Handles both positive and negative w (behind camera).
fn clip_to_pixel(clip_xy: vec2f, clip_w: f32, screen_size: vec2f) -> vec2f {
    if (clip_w <= 0.0) {
        // Behind camera: push to screen edge
        let sign_x = select(1.0, -1.0, clip_xy.x >= 0.0);
        let sign_y = select(1.0, -1.0, clip_xy.y >= 0.0);
        return (vec2f(sign_x, sign_y) * 0.5 + 0.5) * screen_size;
    }
    let ndc = clip_xy / clip_w;
    return (ndc * 0.5 + 0.5) * screen_size;
}

/// Compute the screen-space AABB of a view-space sphere.
///
/// Projects the view-space center and uses the analytical formula for
/// the screen-space radius of a sphere to build an axis-aligned bounding box.
fn project_sphere_aabb(
    view_center: vec3f,
    radius: f32,
    proj_mat: mat4x4f,
    screen_size: vec2f,
) -> vec4f {
    // Project center
    let clip = proj_mat * vec4f(view_center, 1.0);

    // Check if sphere potentially intersects the near plane
    let z = view_center.z; // negative = in front of camera (Vulkan convention)
    let intersects_near = (-z - radius) < 0.0; // sphere reaches behind camera

    if (intersects_near) {
        // Sphere straddles the near plane - project to cover the entire screen
        // to be safe (conservative but correct)
        return vec4f(0.0, 0.0, screen_size.x, screen_size.y);
    }

    if (clip.w <= 0.0) {
        // Center is behind camera but sphere doesn't reach near plane
        // (shouldn't happen given the check above, but be safe)
        return vec4f(-1.0, -1.0, -1.0, -1.0);
    }

    let center_px = clip_to_pixel(clip.xy, clip.w, screen_size);

    // Analytical screen-space radius of a sphere:
    // Project a point offset by radius perpendicular to the view direction.
    // We use the fact that for a symmetric perspective projection,
    // the screen-space radius depends on the distance and the projection scale.
    let abs_z = abs(z);
    if (abs_z < 0.001) {
        return vec4f(0.0, 0.0, screen_size.x, screen_size.y);
    }

    // Scale factors from the projection matrix
    // proj_mat[0][0] = f/aspect, proj_mat[1][1] = -f (or f, depending on convention)
    let scale_x = abs(proj_mat[0][0]);
    let scale_y = abs(proj_mat[1][1]);

    // Screen-space radius = (radius / abs_z) * scale * (screen_size / 2)
    let half_screen = screen_size * 0.5;
    let screen_radius_x = (radius / abs_z) * scale_x * half_screen.x;
    let screen_radius_y = (radius / abs_z) * scale_y * half_screen.y;

    // Add one tile of padding to avoid missing tiles at the boundary
    let pad = f32(TILE_SIZE);

    let min_x = max(center_px.x - screen_radius_x - pad, 0.0);
    let min_y = max(center_px.y - screen_radius_y - pad, 0.0);
    let max_x = min(center_px.x + screen_radius_x + pad, screen_size.x);
    let max_y = min(center_px.y + screen_radius_y + pad, screen_size.y);

    return vec4f(min_x, min_y, max_x, max_y);
}

@compute @workgroup_size(TILE_SIZE, TILE_SIZE, 1)
fn cs_main(
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
    @builtin(local_invocation_index) local_idx: u32,
) {
    let view_mat = frame_data.view_matrix;
    let proj_mat = frame_data.proj_matrix;
    let light_count = frame_data.light_count;
    let tiles_x = frame_data.tiles_x;
    let screen_width = frame_data.screen_width;
    let screen_height = frame_data.screen_height;

    let tile_idx = workgroup_id.y * tiles_x + workgroup_id.x;
    let base_offset = tile_idx * MAX_LIGHTS_PER_TILE;

    // Tile bounds in pixels
    let tile_min_x = f32(workgroup_id.x * TILE_SIZE);
    let tile_min_y = f32(workgroup_id.y * TILE_SIZE);
    let tile_max_x = tile_min_x + f32(TILE_SIZE);
    let tile_max_y = tile_min_y + f32(TILE_SIZE);

    // Each thread tests lights with stride loop
    let threads_per_tile = TILE_SIZE * TILE_SIZE;
    for (var i = local_idx; i < light_count; i += threads_per_tile) {
        let light = point_lights[i];
        let light_pos = light.position;
        let light_range = light.range;

        if (light_range <= 0.0) {
            continue;
        }

        // Transform to view space
        let view_pos = view_mat * vec4f(light_pos, 1.0);

        // Compute screen-space AABB of the light sphere
        let aabb = project_sphere_aabb(
            view_pos.xyz,
            light_range,
            proj_mat,
            vec2f(f32(screen_width), f32(screen_height)),
        );

        // Skip lights with invalid AABB (fully behind camera)
        if (aabb.x < 0.0) {
            continue;
        }

        // AABB overlap test: tile rect vs light AABB
        if (tile_max_x < aabb.x || tile_min_x > aabb.z ||
            tile_max_y < aabb.y || tile_min_y > aabb.w) {
            continue;
        }

        // Atomically claim a slot in the tile's light list
        let slot = atomicAdd(&tile_light_counts[tile_idx], 1u);
        if (slot < MAX_LIGHTS_PER_TILE) {
            tile_light_indices[base_offset + slot] = i;
        }
    }

    // Clamp the final count (in case multiple threads exceeded MAX_LIGHTS_PER_TILE)
    if (local_idx == 0u) {
        let count = atomicLoad(&tile_light_counts[tile_idx]);
        if (count > MAX_LIGHTS_PER_TILE) {
            atomicStore(&tile_light_counts[tile_idx], MAX_LIGHTS_PER_TILE);
        }
    }
}
