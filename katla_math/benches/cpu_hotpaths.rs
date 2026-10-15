//! Matrix and bounds batches; setup and allocation are outside sampling.

#[path = "../../scripts/cpu_bench_support.rs"]
mod support;

use katla_math::{AABB, Mat4, Quat, Transform, Vec3, Vec4};
use std::hint::black_box;

fn main() {
    println!("scenario,entities,threads,matches,sample,iterations,ns_per_iteration,checksum");
    let transforms: Vec<_> = (0..4096)
        .map(|i| {
            let x = i as f32 * 0.001;
            Transform::from_position_rotation_scale(
                Vec3::new(x, -2.0 * x, 3.0),
                Quat::from_euler(x, 0.7, -0.3),
                Vec3::new(1.0 + x, -0.5, 2.0),
            )
        })
        .collect();
    let matrices: Vec<_> = transforms.iter().map(Transform::make_mat4).collect();
    let rhs = Mat4::create_proj_perspective(65.0, 1.7, 0.1, 100.0);
    let vector = Vec4::new(0.4, -0.2, 0.7, 1.0);
    let bounds = AABB::from_min_max(Vec3::new(-2.0, -1.0, -0.5), Vec3::new(3.0, 1.5, 2.0));
    support::measure("mat4_mul", matrices.len(), || {
        black_box(&matrices)
            .iter()
            .map(|m| black_box(m.mul(black_box(&rhs)))[0][0] as f64)
            .sum()
    });
    support::measure("mat4_vec4", matrices.len(), || {
        black_box(&matrices)
            .iter()
            .map(|m| black_box(*m * black_box(vector)).x() as f64)
            .sum()
    });
    support::measure("mat4_point", matrices.len(), || {
        black_box(&matrices)
            .iter()
            .map(|m| black_box(*m * black_box(vector.xyz())).x() as f64)
            .sum()
    });
    support::measure("transform_matrix", transforms.len(), || {
        black_box(&transforms)
            .iter()
            .map(|t| black_box(t.make_mat4())[0][0] as f64)
            .sum()
    });
    support::measure("aabb_transform", matrices.len(), || {
        black_box(&matrices)
            .iter()
            .map(|m| black_box(bounds.transform(black_box(m))).extent.x() as f64)
            .sum()
    });
}
