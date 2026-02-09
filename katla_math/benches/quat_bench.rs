#![cfg_attr(test, allow(dead_code))]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use katla_math::{Quat, Vec3};

fn bench_quat_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("quat_creation");

    group.bench_function("new", |b| {
        b.iter(|| black_box(Quat::new()));
    });

    group.bench_function("default", |b| {
        b.iter(|| black_box(Quat::default()));
    });

    group.bench_function("new_from_xyzw", |b| {
        b.iter(|| black_box(Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0)));
    });

    group.bench_function("from_axis_angle", |b| {
        let axis = Vec3::new(0.0, 1.0, 0.0);
        let angle = 1.5;
        b.iter(|| black_box(Quat::from_axis_angle(axis, angle)));
    });

    group.bench_function("from_rotation_between", |b| {
        let from = Vec3::new(1.0, 0.0, 0.0);
        let to = Vec3::new(0.0, 1.0, 0.0);
        b.iter(|| black_box(Quat::from_rotation_between(from, to)));
    });

    group.bench_function("new_from_yaw_pitch", |b| {
        b.iter(|| black_box(Quat::new_from_yaw_pitch(1.5, 0.5)));
    });

    group.finish();
}

fn bench_quat_arithmetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("quat_arithmetic");

    let q1 = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5);
    let q2 = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 0.5);

    group.bench_function("mul", |b| {
        b.iter(|| black_box(q1 * q2));
    });

    group.finish();
}

fn bench_quat_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("quat_operations");

    let q1 = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5);
    let q2 = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 0.5);

    group.bench_function("dot", |b| {
        b.iter(|| black_box(q1.dot(q2)));
    });

    group.bench_function("inverse", |b| {
        b.iter(|| black_box(q1.inverse()));
    });

    group.bench_function("is_normalized", |b| {
        b.iter(|| black_box(q1.is_normalized()));
    });

    group.bench_function("normalize", |b| {
        b.iter(|| {
            let mut q = q1;
            q.normalize();
            black_box(q);
        });
    });

    group.finish();
}

fn bench_quat_rotation(c: &mut Criterion) {
    let mut group = c.benchmark_group("quat_rotation");

    let q = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5);
    let v = Vec3::new(1.0, 0.0, 0.0);

    group.bench_function("rotate_vec3", |b| {
        b.iter(|| black_box(q.rotate_vec3(v)));
    });

    group.bench_function("mul_vec3", |b| {
        b.iter(|| black_box(q * v));
    });

    group.finish();
}

fn bench_quat_interpolation(c: &mut Criterion) {
    let mut group = c.benchmark_group("quat_interpolation");

    let q1 = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.0);
    let q2 = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5);

    group.bench_function("slerp_0", |b| {
        b.iter(|| black_box(Quat::slerp(q1, q2, 0.0)));
    });

    group.bench_function("slerp_0_5", |b| {
        b.iter(|| black_box(Quat::slerp(q1, q2, 0.5)));
    });

    group.bench_function("slerp_1", |b| {
        b.iter(|| black_box(Quat::slerp(q1, q2, 1.0)));
    });

    group.finish();
}

fn bench_quat_matrix_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("quat_matrix_conversion");

    let q = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5);

    group.bench_function("make_mat4", |b| {
        b.iter(|| black_box(q.make_mat4()));
    });

    group.finish();
}

fn bench_quat_indexing(c: &mut Criterion) {
    let mut group = c.benchmark_group("quat_indexing");

    let q = Quat::new_from_xyzw(1.0, 2.0, 3.0, 4.0);

    group.bench_function("index_0", |b| {
        b.iter(|| black_box(q[0]));
    });

    group.bench_function("index_1", |b| {
        b.iter(|| black_box(q[1]));
    });

    group.bench_function("index_2", |b| {
        b.iter(|| black_box(q[2]));
    });

    group.bench_function("index_3", |b| {
        b.iter(|| black_box(q[3]));
    });

    group.bench_function("xyzw", |b| {
        b.iter(|| black_box(q.xyzw()));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_quat_creation,
    bench_quat_arithmetic,
    bench_quat_operations,
    bench_quat_rotation,
    bench_quat_interpolation,
    bench_quat_matrix_conversion,
    bench_quat_indexing
);
criterion_main!(benches);
