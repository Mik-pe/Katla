//! CPU cascade contracts previously checked by the native validation executable.

use katla_gfx::shadow::cascade::{CascadeParams, CascadeShadowMap, ShadowFrameData};

fn identity() -> [f32; 16] {
    let mut matrix = [0.0; 16];
    for index in [0, 5, 10, 15] {
        matrix[index] = 1.0;
    }
    matrix
}

fn projection() -> [f32; 16] {
    let focal = 1.0 / (60.0_f32.to_radians() * 0.5).tan();
    [
        focal / (16.0 / 9.0),
        0.0,
        0.0,
        0.0,
        0.0,
        -focal,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        -1.0,
        0.0,
        0.0,
        0.1,
        0.0,
    ]
}

fn cascades(direction: [f32; 3], view: &[f32; 16]) -> ShadowFrameData {
    let mut shadow = CascadeShadowMap::new(CascadeParams {
        shadow_map_size: 1024,
        max_distance: 50.0,
        ..Default::default()
    });
    shadow.update(direction, view, &projection());
    shadow.gpu_data()
}

fn determinant(matrix: &[f32; 16]) -> f32 {
    let minor = |indices: [usize; 9]| {
        let values = indices.map(|index| matrix[index]);
        values[0] * (values[4] * values[8] - values[5] * values[7])
            - values[1] * (values[3] * values[8] - values[5] * values[6])
            + values[2] * (values[3] * values[7] - values[4] * values[6])
    };
    matrix[0] * minor([5, 6, 7, 9, 10, 11, 13, 14, 15])
        - matrix[1] * minor([4, 6, 7, 8, 10, 11, 12, 14, 15])
        + matrix[2] * minor([4, 5, 7, 8, 9, 11, 12, 13, 15])
        - matrix[3] * minor([4, 5, 6, 8, 9, 10, 12, 13, 14])
}

#[test]
fn test_cascades_cover_the_camera_interval_with_invertible_transforms() {
    let data = cascades([0.5, -0.8, -0.3], &identity());
    assert_eq!(data.light_direction[3], 4.0);
    let mut previous_split = 0.1;
    for cascade in &data.cascades {
        assert!(cascade.split_distance > previous_split);
        previous_split = cascade.split_distance;
        assert!(cascade.view_proj.iter().all(|value| value.is_finite()));
        assert!(determinant(&cascade.view_proj).abs() > 1e-10);
        assert!((cascade.texel_size - 1.0 / 1024.0).abs() < 1e-8);
    }
    assert!(previous_split >= 45.0);
    let direction_length = data.light_direction[..3]
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    assert!((direction_length - 1.0).abs() < 1e-5);
}

#[test]
fn test_light_and_camera_changes_change_cascade_transforms() {
    let first = cascades([0.5, -0.8, -0.3], &identity());
    let second = cascades([-0.3, -0.9, 0.1], &identity());
    assert!(
        first
            .cascades
            .iter()
            .zip(second.cascades.iter())
            .any(|(a, b)| a.view_proj != b.view_proj)
    );
    let mut translated = identity();
    translated[14] = 10.0;
    let moved = cascades([0.5, -0.8, -0.3], &translated);
    assert!(
        first
            .cascades
            .iter()
            .zip(moved.cascades.iter())
            .any(|(a, b)| a.view_proj != b.view_proj)
    );
}

#[test]
fn test_unnormalized_direction_produces_the_same_cascades() {
    let raw = [0.3_f32, 1.0, 0.2];
    let length = raw.iter().map(|value| value * value).sum::<f32>().sqrt();
    let first = cascades(raw, &identity());
    let normalized = cascades(raw.map(|value| value / length), &identity());
    for (a, b) in first.cascades.iter().zip(normalized.cascades.iter()) {
        assert!(
            a.view_proj
                .iter()
                .zip(b.view_proj)
                .all(|(a, b)| (*a - b).abs() <= 1e-6)
        );
    }
}
