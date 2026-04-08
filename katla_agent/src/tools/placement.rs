use katla_ecs::scene_tool::SceneOp;
use std::f32::consts::PI;
use std::f32::consts::TAU;

/// Scatter entities randomly in a rectangular area using a grid + jitter approach.
/// Returns one `SpawnEntity` op per entity.
pub fn scatter(
    count: usize,
    center: [f32; 3],
    bounds: [f32; 3],
    min_spacing: f32,
    name_prefix: &str,
) -> Vec<SceneOp> {
    if count == 0 {
        return vec![];
    }

    let side = f32::ceil(f32::sqrt(count as f32)) as usize;
    let spacing_x = if side > 1 {
        2.0 * bounds[0] / (side - 1) as f32
    } else {
        0.0
    };
    let spacing_z = if side > 1 {
        2.0 * bounds[2] / (side - 1) as f32
    } else {
        0.0
    };

    let mut positions = Vec::with_capacity(count);
    let mut idx = 0usize;

    for row in 0..side {
        for col in 0..side {
            if idx >= count {
                break;
            }

            let base_x = -bounds[0] + col as f32 * spacing_x;
            let base_z = -bounds[2] + row as f32 * spacing_z;

            // Deterministic jitter using sin/cos of index
            let jitter_x = f32::sin(idx as f32 * 7.23) * spacing_x * 0.1;
            let jitter_z = f32::cos(idx as f32 * 11.37) * spacing_z * 0.1;

            let x = center[0] + base_x + jitter_x;
            let z = center[2] + base_z + jitter_z;

            // Enforce min_spacing if specified
            if min_spacing > 0.0 {
                let too_close = positions.iter().any(|p: &[f32; 3]| {
                    let dx = p[0] - x;
                    let dz = p[2] - z;
                    f32::sqrt(dx * dx + dz * dz) < min_spacing
                });
                if too_close {
                    continue;
                }
            }

            positions.push([x, center[1], z]);
            idx += 1;
        }
        if idx >= count {
            break;
        }
    }

    positions
        .into_iter()
        .enumerate()
        .map(|(i, pos)| SceneOp::SpawnEntity {
            position: pos,
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            name: Some(format!("{name_prefix}_{i}")),
        })
        .collect()
}

/// Place entities along a polyline path with even spacing.
pub fn place_along_path(points: &[[f32; 3]], spacing: f32, name_prefix: &str) -> Vec<SceneOp> {
    if points.is_empty() || spacing <= 0.0 {
        return vec![];
    }

    // Compute cumulative distances along the path
    let mut segments: Vec<([f32; 3], f32)> = Vec::with_capacity(points.len());
    segments.push((points[0], 0.0));
    for i in 1..points.len() {
        let dx = points[i][0] - points[i - 1][0];
        let dy = points[i][1] - points[i - 1][1];
        let dz = points[i][2] - points[i - 1][2];
        let seg_len = f32::sqrt(dx * dx + dy * dy + dz * dz);
        let prev_dist = segments[i - 1].1;
        segments.push((points[i], prev_dist + seg_len));
    }

    let total_length = segments.last().map(|(_, d)| *d).unwrap_or(0.0);
    if total_length < spacing {
        return vec![];
    }

    let mut ops = Vec::new();
    let mut distance = 0.0f32;
    let mut idx = 0usize;

    while distance <= total_length {
        // Find which segment this distance falls on
        let pos = interpolate_along_path(&segments, distance);
        ops.push(SceneOp::SpawnEntity {
            position: pos,
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            name: Some(format!("{name_prefix}_{idx}")),
        });
        distance += spacing;
        idx += 1;
    }

    ops
}

fn interpolate_along_path(segments: &[([f32; 3], f32)], distance: f32) -> [f32; 3] {
    if segments.len() == 1 {
        return segments[0].0;
    }

    // Find the segment that contains this distance
    for i in 1..segments.len() {
        if distance <= segments[i].1 {
            let seg_start = segments[i - 1];
            let seg_end = segments[i];
            let seg_length = seg_end.1 - seg_start.1;
            if seg_length == 0.0 {
                return seg_start.0;
            }
            let t = (distance - seg_start.1) / seg_length;
            return [
                seg_start.0[0] + t * (seg_end.0[0] - seg_start.0[0]),
                seg_start.0[1] + t * (seg_end.0[1] - seg_start.0[1]),
                seg_start.0[2] + t * (seg_end.0[2] - seg_start.0[2]),
            ];
        }
    }

    // Past the end, return last point
    segments.last().map(|&(p, _)| p).unwrap_or([0.0; 3])
}

/// Place entities in a regular grid pattern.
pub fn place_grid(
    count_x: usize,
    count_z: usize,
    center: [f32; 3],
    spacing: [f32; 2],
    name_prefix: &str,
) -> Vec<SceneOp> {
    let mut ops = Vec::with_capacity(count_x * count_z);

    let offset_x = if count_x > 1 {
        (count_x - 1) as f32 * spacing[0] / 2.0
    } else {
        0.0
    };
    let offset_z = if count_z > 1 {
        (count_z - 1) as f32 * spacing[1] / 2.0
    } else {
        0.0
    };

    for row in 0..count_z {
        for col in 0..count_x {
            let x = center[0] - offset_x + col as f32 * spacing[0];
            let z = center[2] - offset_z + row as f32 * spacing[1];

            ops.push(SceneOp::SpawnEntity {
                position: [x, center[1], z],
                rotation: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                name: Some(format!("{name_prefix}_{row}_{col}")),
            });
        }
    }

    ops
}

/// Place entities in a circle/ring pattern.
pub fn place_ring(count: usize, center: [f32; 3], radius: f32, name_prefix: &str) -> Vec<SceneOp> {
    if count == 0 {
        return vec![];
    }

    (0..count)
        .map(|i| {
            let angle = TAU * i as f32 / count as f32;
            let x = center[0] + radius * f32::cos(angle);
            let z = center[2] + radius * f32::sin(angle);

            SceneOp::SpawnEntity {
                position: [x, center[1], z],
                rotation: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                name: Some(format!("{name_prefix}_{i}")),
            }
        })
        .collect()
}

/// Place entities in a cluster using Fibonacci sphere sampling.
pub fn place_cluster(
    count: usize,
    center: [f32; 3],
    radius: f32,
    name_prefix: &str,
) -> Vec<SceneOp> {
    if count == 0 {
        return vec![];
    }

    let golden_ratio = (1.0 + f32::sqrt(5.0)) / 2.0;
    let golden_angle = TAU / golden_ratio;

    (0..count)
        .map(|i| {
            // Fibonacci sphere sampling
            let t = i as f32 / count as f32;
            let inclination = f32::acos(1.0 - 2.0 * t);
            let azimuth = golden_angle * i as f32;

            // Map from unit sphere to radius
            let r = radius * f32::cbrt(f32::sqrt(t)); // cube root for volume-uniform distribution
            let x = center[0] + r * f32::sin(inclination) * f32::cos(azimuth);
            let y = center[1] + r * f32::sin(inclination) * f32::sin(azimuth);
            let z = center[2] + r * f32::cos(inclination);

            SceneOp::SpawnEntity {
                position: [x, y, z],
                rotation: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                name: Some(format!("{name_prefix}_{i}")),
            }
        })
        .collect()
}

/// 2π constant (full turn).
/// This is re-exported for convenience since we use TAU above.
const _TAU: f32 = 2.0 * PI;
