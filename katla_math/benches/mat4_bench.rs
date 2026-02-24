#![cfg_attr(test, allow(dead_code))]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use katla_math::{Mat4, Vec3, Vec4};

fn bench_mat4_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("mat4_creation");

    group.bench_function("new", |b| {
        b.iter(|| black_box(Mat4::new()));
    });

    group.bench_function("default", |b| {
        b.iter(|| black_box(Mat4::default()));
    });

    group.bench_function("identity", |b| {
        b.iter(|| black_box(Mat4::identity()));
    });

    group.bench_function("from_translation", |b| {
        b.iter(|| black_box(Mat4::from_translation([1.0, 2.0, 3.0])));
    });

    group.bench_function("from_rotaxis", |b| {
        b.iter(|| black_box(Mat4::from_rotaxis(&1.5, [0.0, 1.0, 0.0])));
    });

    group.bench_function("create_ortho", |b| {
        b.iter(|| black_box(Mat4::create_ortho(-1.0, 1.0, -1.0, 1.0, 0.1, 100.0)));
    });

    group.bench_function("create_proj", |b| {
        b.iter(|| black_box(Mat4::create_proj(60.0, 16.0 / 9.0, 0.1)));
    });

    group.bench_function("create_lookat", |b| {
        let eye = Vec3::new(0.0, 2.0, 5.0);
        let target = Vec3::new(0.0, 0.0, 0.0);
        let up = Vec3::new(0.0, 1.0, 0.0);
        b.iter(|| black_box(Mat4::create_lookat(eye, target, up)));
    });

    group.finish();
}

fn bench_mat4_arithmetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("mat4_arithmetic");

    let m1 = Mat4::new();
    let m2 = Mat4::from_rotaxis(&1.5, [0.0, 1.0, 0.0]);

    group.bench_function("mul", |b| {
        b.iter(|| black_box(m1.mul(&m2)));
    });

    group.finish();
}

fn bench_mat4_linear_algebra(c: &mut Criterion) {
    let mut group = c.benchmark_group("mat4_linear_algebra");

    let m1 = Mat4::from_rotaxis(&1.5, [0.0, 1.0, 0.0]);

    group.bench_function("calc_det", |b| {
        b.iter(|| black_box(m1.calc_det()));
    });

    group.bench_function("calc_inv_det", |b| {
        b.iter(|| black_box(m1.calc_inv_det()));
    });

    group.bench_function("inverse", |b| {
        b.iter(|| black_box(m1.inverse()));
    });

    group.bench_function("extract_row", |b| {
        b.iter(|| black_box(m1.extract_row(0)));
    });

    group.bench_function("extract_row_all", |b| {
        b.iter(|| {
            black_box(m1.extract_row(0));
            black_box(m1.extract_row(1));
            black_box(m1.extract_row(2));
            black_box(m1.extract_row(3));
        });
    });

    group.finish();
}

fn bench_mat4_indexing(c: &mut Criterion) {
    let mut group = c.benchmark_group("mat4_indexing");

    let m = Mat4::from_rotaxis(&1.5, [0.0, 1.0, 0.0]);

    group.bench_function("index_col", |b| {
        b.iter(|| black_box(&m[0]));
    });

    group.bench_function("index_element", |b| {
        b.iter(|| black_box(m[0][0]));
    });

    group.finish();
}

fn bench_mat4_transformations(c: &mut Criterion) {
    let mut group = c.benchmark_group("mat4_transformations");

    // Simulate a common pattern: creating a transformation matrix
    group.bench_function("trs_combined", |b| {
        let translation = Vec3::new(1.0, 2.0, 3.0);
        let rotation_angle = 1.5;
        let rotation_axis = Vec3::new(0.0, 1.0, 0.0);
        let scale = Vec3::new(2.0, 2.0, 2.0);

        b.iter(|| {
            let t = Mat4::from_translation(translation.0);
            let r = Mat4::from_rotaxis(&rotation_angle, rotation_axis.0);
            let s = Mat4([
                Vec4([scale[0], 0.0, 0.0, 0.0]),
                Vec4([0.0, scale[1], 0.0, 0.0]),
                Vec4([0.0, 0.0, scale[2], 0.0]),
                Vec4([0.0, 0.0, 0.0, 1.0]),
            ]);
            black_box(t.mul(&r.mul(&s)));
        });
    });

    group.finish();
}

fn bench_mat4_inverse_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("mat4_inverse_scenarios");

    group.bench_function("inverse_identity", |b| {
        let m = Mat4::identity();
        b.iter(|| black_box(m.inverse()));
    });

    group.bench_function("inverse_translation", |b| {
        let m = Mat4::from_translation([1.0, 2.0, 3.0]);
        b.iter(|| black_box(m.inverse()));
    });

    group.bench_function("inverse_rotation", |b| {
        let m = Mat4::from_rotaxis(&1.5, [0.0, 1.0, 0.0]);
        b.iter(|| black_box(m.inverse()));
    });

    group.bench_function("inverse_complex", |b| {
        let t = Mat4::from_translation([1.0, 2.0, 3.0]);
        let r = Mat4::from_rotaxis(&1.5, [0.0, 1.0, 0.0]);
        let m = t.mul(&r);
        b.iter(|| black_box(m.inverse()));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_mat4_creation,
    bench_mat4_arithmetic,
    bench_mat4_linear_algebra,
    bench_mat4_indexing,
    bench_mat4_transformations,
    bench_mat4_inverse_scenarios
);
criterion_main!(benches);
