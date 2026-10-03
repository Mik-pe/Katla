//! Native arithmetic acceptance for the actual scene material shader helpers.

use super::*;

const PROBE: &str = r#"
@group(0) @binding(0) var<storage, read_write> output: array<vec4f>;
@compute @workgroup_size(1)
fn cs_main() {
    let N = vec3f(0.0, 0.0, 1.0);
    output[0] = vec4f(distribution_ggx(N, N, 0.5), distribution_ggx(N, N, 0.25), distribution_ggx(N, N, 0.04), 0.0);
    output[1] = vec4f(pbr_direct_light(N, N, N, vec3f(0.8, 0.2, 0.1), 0.0, 0.5, vec3f(1.0)), 0.0);
    output[2] = vec4f(pbr_direct_light(N, N, normalize(vec3f(1.0, 0.0, 1.0)), vec3f(0.8, 0.2, 0.1), 0.3, 0.4, vec3f(1.0)), 0.0);
    output[3] = vec4f(pbr_direct_light(N, normalize(vec3f(0.5, 0.0, 1.0)), normalize(vec3f(-1.0, 0.0, 0.2)), vec3f(0.8, 0.2, 0.1), 1.0, 0.7, vec3f(1.0)), 0.0);
    output[4] = vec4f(pbr_direct_light(N, N, -N, vec3f(1.0), 0.0, 0.1, vec3f(1.0)), 0.0);
    output[5] = vec4f(pbr_direct_light(N, -N, N, vec3f(1.0), 1.0, 0.1, vec3f(1.0)), 0.0);
    let local_normal = normalize(vec3f(0.0, 1.0, 1.0));
    let local_tangent = vec4f(1.0, 0.0, 0.0, 1.0);
    let model = mat3x3f(vec3f(2.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), vec3f(0.0, 0.0, 0.5));
    let mirrored = mat3x3f(vec3f(-2.0, 0.0, 0.0), model[1], model[2]);
    let a = transformed_tangent_frame(model, local_normal, local_tangent);
    let b = transformed_tangent_frame(mirrored, local_normal, local_tangent);
    output[6] = vec4f(a.normal, 0.0); output[7] = vec4f(a.tangent, 0.0); output[8] = vec4f(a.bitangent, 0.0);
    output[9] = vec4f(b.normal, 0.0); output[10] = vec4f(b.tangent, 0.0); output[11] = vec4f(b.bitangent, 0.0);
    let skin = mat3x3f(vec3f(0.0, 1.0, 0.0), vec3f(-1.0, 0.0, 0.0), vec3f(0.0, 0.0, 1.0));
    let c = transformed_tangent_frame(model * skin, local_normal, local_tangent);
    output[12] = vec4f(c.normal, 0.0); output[13] = vec4f(c.tangent, 0.0); output[14] = vec4f(c.bitangent, 0.0);
    let d = transformed_tangent_frame(mat3x3f(vec3f(0.0), vec3f(0.0), vec3f(0.0)), vec3f(0.0), vec4f(0.0, 0.0, 0.0, 1.0));
    output[15] = vec4f(d.normal, 0.0); output[16] = vec4f(d.tangent, 0.0); output[17] = vec4f(d.bitangent, 0.0);
}
"#;

fn normalized(vector: [f64; 3]) -> [f64; 3] {
    let length = vector.iter().map(|v| v * v).sum::<f64>().sqrt();
    vector.map(|v| v / length)
}

/// Independent f64 scalar BRDF reference for the shader's observable light output.
fn direct_reference(view: [f64; 3], light: [f64; 3], metallic: f64, roughness: f64) -> [f64; 3] {
    let view = normalized(view);
    let light = normalized(light);
    let half = normalized(std::array::from_fn(|i| view[i] + light[i]));
    let no_v = view[2];
    let no_l = light[2];
    let vo_h: f64 = view.into_iter().zip(half).map(|(v, h)| v * h).sum();
    let alpha_sq = roughness.powi(4);
    let d = alpha_sq / (std::f64::consts::PI * (1.0 + (alpha_sq - 1.0) * half[2].powi(2)).powi(2));
    let smith = 2.0 * no_v * no_l
        / (no_l * (alpha_sq + (1.0 - alpha_sq) * no_v.powi(2)).sqrt()
            + no_v * (alpha_sq + (1.0 - alpha_sq) * no_l.powi(2)).sqrt());
    [0.8, 0.2, 0.1].map(|color| {
        let f0 = 0.04 * (1.0 - metallic) + color * metallic;
        let f = f0 + (1.0 - f0) * (1.0 - vo_h).powi(5);
        ((1.0 - f) * (1.0 - metallic) * color / std::f64::consts::PI
            + d * smith * f / (4.0 * no_v * no_l))
            * no_l
    })
}

#[test]
fn test_native_material_brdf_and_static_skinned_tangent_frames() {
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    let mut graph = readback_graph();
    let (output, id) = import_data(
        &mut renderer,
        &mut graph,
        "material samples",
        &[0; 18 * 16],
        BufferUsages::STORAGE | BufferUsages::TRANSFER_SOURCE,
        BufferMemoryPolicy::DeviceLocal,
    );
    let source = format!(
        "{}\n{}\n{PROBE}",
        include_str!("../../../../resources/shaders/common/pbr.wgsl"),
        include_str!("../../../../resources/shaders/common/tangent_frame.wgsl")
    );
    dispatch(&mut graph, "material arithmetic", &source, &[(0, 0, id)]);
    prepare_copies(&mut renderer, &mut graph, &[(id, 0, 18 * 16)]);
    let frame = acquire(&mut renderer);
    renderer.render(&frame, &mut graph, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    let bytes = read_completed(&mut renderer, &graph, frame.slot());
    let rows: Vec<[f32; 4]> = bytes[..18 * 16]
        .as_chunks::<16>()
        .0
        .iter()
        .map(|row| {
            std::array::from_fn(|i| f32::from_ne_bytes(row[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect();
    for (actual, roughness) in rows[0][..3].iter().zip([0.5f64, 0.25, 0.04]) {
        let expected = 1.0 / (std::f64::consts::PI * roughness.powi(4));
        assert!(
            (f64::from(*actual) / expected - 1.0).abs() < 2e-5,
            "GGX peak must use perceptual roughness to the fourth power: {rows:?}"
        );
    }
    for (row, view, light, metallic, roughness) in [
        (1, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 0.0, 0.5),
        (2, [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], 0.3, 0.4),
        (3, [0.5, 0.0, 1.0], [-1.0, 0.0, 0.2], 1.0, 0.7),
    ] {
        for (actual, expected) in rows[row][..3]
            .iter()
            .zip(direct_reference(view, light, metallic, roughness))
        {
            assert!(
                (f64::from(*actual) - expected).abs() < 2e-5,
                "per-light BRDF row {row}: {:?} vs {expected}",
                rows[row]
            );
        }
    }
    assert_eq!(rows[4], [0.0; 4]);
    assert_eq!(rows[5], [0.0; 4]);
    let root5 = 5.0f32.sqrt();
    let root17 = 17.0f32.sqrt();
    for (row, expected) in [
        (6, [0.0, 1.0 / root5, 2.0 / root5]),
        (7, [1.0, 0.0, 0.0]),
        (8, [0.0, 2.0 / root5, -1.0 / root5]),
        (9, [0.0, 1.0 / root5, 2.0 / root5]),
        (10, [-1.0, 0.0, 0.0]),
        (11, [0.0, 2.0 / root5, -1.0 / root5]),
        (12, [-1.0 / root17, 0.0, 4.0 / root17]),
        (13, [0.0, 1.0, 0.0]),
        (14, [-4.0 / root17, 0.0, -1.0 / root17]),
        (15, [0.0, 1.0, 0.0]),
        (16, [0.0, 0.0, 1.0]),
        (17, [1.0, 0.0, 0.0]),
    ] {
        for (actual, expected) in rows[row][..3].iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1e-6,
                "affine tangent frame row {row}: {:?}",
                rows[row]
            );
        }
    }
    graph.cleanup();
    renderer.destroy_buffer(output).unwrap();
    renderer.destroy();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}
