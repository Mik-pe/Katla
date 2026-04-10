use super::placement;
use katla_ecs::scene_tool::SceneOp;

#[test]
fn test_scatter_count() {
    let result = placement::scatter(10, [0.0, 0.0, 0.0], [5.0, 0.0, 5.0], 0.0, "Obj");
    assert_eq!(result.ops.len(), 10);
    assert_eq!(result.count_requested, 10);
    assert_eq!(result.count_placed, 10);
}

#[test]
fn test_scatter_within_bounds() {
    let center = [0.0, 0.0, 0.0];
    let bounds = [10.0, 0.0, 10.0];
    let result = placement::scatter(20, center, bounds, 0.0, "Obj");

    for op in &result.ops {
        if let SceneOp::SpawnEntity { position, .. } = op {
            assert!(
                (position[0] - center[0]).abs() <= bounds[0] + 1.0,
                "x={} out of bounds",
                position[0]
            );
            assert!(
                (position[2] - center[2]).abs() <= bounds[2] + 1.0,
                "z={} out of bounds",
                position[2]
            );
        }
    }
}

#[test]
fn test_scatter_zero_count() {
    let result = placement::scatter(0, [0.0, 0.0, 0.0], [5.0, 0.0, 5.0], 0.0, "Obj");
    assert!(result.ops.is_empty());
    assert_eq!(result.count_requested, 0);
    assert_eq!(result.count_placed, 0);
}

#[test]
fn test_grid_dimensions() {
    let ops = placement::place_grid(3, 4, [0.0, 0.0, 0.0], [2.0, 2.0], "Grid");
    assert_eq!(ops.len(), 12);
}

#[test]
fn test_grid_spacing() {
    let ops = placement::place_grid(3, 1, [0.0, 0.0, 0.0], [2.0, 3.0], "Grid");

    // 3 columns, 1 row with spacing 2.0 in x
    // Positions should be at -2, 0, 2 (centered)
    let positions: Vec<[f32; 3]> = ops
        .iter()
        .map(|op| {
            if let SceneOp::SpawnEntity { position, .. } = op {
                *position
            } else {
                panic!("Expected SpawnEntity");
            }
        })
        .collect();

    assert_eq!(positions.len(), 3);
    // Check x-spacing
    for i in 1..positions.len() {
        let dx = positions[i][0] - positions[i - 1][0];
        assert!((dx - 2.0).abs() < 0.01, "Expected spacing 2.0, got {dx}");
    }
}

#[test]
fn test_grid_centered() {
    let ops = placement::place_grid(3, 3, [5.0, 0.0, 5.0], [1.0, 1.0], "Grid");

    // Center of grid should be approximately (5, 0, 5)
    let center_idx = 4; // middle of 3x3 = index 4
    if let SceneOp::SpawnEntity { position, .. } = &ops[center_idx] {
        assert!((position[0] - 5.0).abs() < 0.01);
        assert!((position[2] - 5.0).abs() < 0.01);
    }
}

#[test]
fn test_ring_count() {
    let ops = placement::place_ring(8, [0.0, 0.0, 0.0], 5.0, "Ring");
    assert_eq!(ops.len(), 8);
}

#[test]
fn test_ring_radius() {
    let center = [1.0, 2.0, 3.0];
    let radius = 4.0;
    let ops = placement::place_ring(6, center, radius, "Ring");

    for op in &ops {
        if let SceneOp::SpawnEntity { position, .. } = op {
            let dx = position[0] - center[0];
            let dz = position[2] - center[2];
            let dist = f32::sqrt(dx * dx + dz * dz);
            assert!(
                (dist - radius).abs() < 0.01,
                "Expected radius {radius}, got {dist}"
            );
        }
    }
}

#[test]
fn test_ring_zero_count() {
    let ops = placement::place_ring(0, [0.0, 0.0, 0.0], 5.0, "Ring");
    assert!(ops.is_empty());
}

#[test]
fn test_cluster_count() {
    let ops = placement::place_cluster(20, [0.0, 0.0, 0.0], 3.0, "Cluster");
    assert_eq!(ops.len(), 20);
}

#[test]
fn test_cluster_within_radius() {
    let center = [0.0, 0.0, 0.0];
    let radius = 5.0;
    let ops = placement::place_cluster(50, center, radius, "Cluster");

    for op in &ops {
        if let SceneOp::SpawnEntity { position, .. } = op {
            let dx = position[0] - center[0];
            let dy = position[1] - center[1];
            let dz = position[2] - center[2];
            let dist = f32::sqrt(dx * dx + dy * dy + dz * dz);
            assert!(
                dist <= radius + 0.01,
                "Position {dist} exceeds radius {radius}"
            );
        }
    }
}

#[test]
fn test_path_spacing() {
    let points = [[0.0, 0.0, 0.0], [0.0, 0.0, 5.0]];
    let ops = placement::place_along_path(&points, 1.0, "Path");

    // 5 units / 1.0 spacing = 6 points (at 0, 1, 2, 3, 4, 5)
    assert_eq!(ops.len(), 6);
}

#[test]
fn test_path_empty() {
    let ops = placement::place_along_path(&[], 1.0, "Path");
    assert!(ops.is_empty());
}

#[test]
fn test_path_single_point() {
    let points = [[1.0, 2.0, 3.0]];
    let ops = placement::place_along_path(&points, 1.0, "Path");
    // Total length is 0, which is less than spacing, so no entities
    assert!(ops.is_empty());
}

#[test]
fn test_scatter_min_spacing_fewer_placed() {
    // Small bounds with large min_spacing: not all requested entities can fit.
    let result = placement::scatter(20, [0.0, 0.0, 0.0], [1.0, 0.0, 1.0], 10.0, "Obj");

    assert_eq!(result.count_requested, 20);
    assert!(
        result.count_placed < result.count_requested,
        "Expected fewer placed than requested with large min_spacing, got {} placed of {} requested",
        result.count_placed,
        result.count_requested
    );
    assert_eq!(result.ops.len(), result.count_placed);

    // Verify all placed entities respect min_spacing
    let positions: Vec<[f32; 3]> = result
        .ops
        .iter()
        .map(|op| {
            if let SceneOp::SpawnEntity { position, .. } = op {
                *position
            } else {
                panic!("Expected SpawnEntity");
            }
        })
        .collect();

    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            let dx = positions[i][0] - positions[j][0];
            let dz = positions[i][2] - positions[j][2];
            let dist = f32::sqrt(dx * dx + dz * dz);
            assert!(
                dist >= 10.0 - 0.01,
                "Entities {i} and {j} too close: {dist} < 10.0"
            );
        }
    }
}
