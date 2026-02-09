#![cfg_attr(test, allow(dead_code))]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use katla_math::Vec3;

fn bench_vec3_arithmetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec3_arithmetic");

    let v1 = Vec3::new(1.0, 2.0, 3.0);
    let v2 = Vec3::new(4.0, 5.0, 6.0);

    group.bench_function("add", |b| {
        b.iter(|| black_box(v1 + v2));
    });

    group.bench_function("sub", |b| {
        b.iter(|| black_box(v1 - v2));
    });

    group.bench_function("mul_scalar", |b| {
        b.iter(|| black_box(v1 * 2.5));
    });

    group.bench_function("mul_vector", |b| {
        b.iter(|| black_box(v1 * v2));
    });

    group.bench_function("div_scalar", |b| {
        b.iter(|| black_box(v1 / 2.5));
    });

    group.bench_function("div_vector", |b| {
        b.iter(|| black_box(v1 / v2));
    });

    group.bench_function("neg", |b| {
        b.iter(|| black_box(-v1));
    });

    group.finish();
}

fn bench_vec3_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec3_operations");

    let v1 = Vec3::new(1.0, 2.0, 3.0);
    let v2 = Vec3::new(4.0, 5.0, 6.0);

    group.bench_function("dot", |b| {
        b.iter(|| black_box(v1.dot(v2)));
    });

    group.bench_function("cross", |b| {
        b.iter(|| black_box(v1.cross(v2)));
    });

    group.bench_function("length", |b| {
        b.iter(|| black_box(v1.length()));
    });

    group.bench_function("length_squared", |b| {
        b.iter(|| black_box(v1.length_squared()));
    });

    group.bench_function("normalize", |b| {
        b.iter(|| black_box(v1.normalize()));
    });

    group.bench_function("lerp", |b| {
        b.iter(|| black_box(Vec3::lerp(v1, v2, 0.5)));
    });

    group.finish();
}

fn bench_vec3_accessors(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec3_accessors");

    let v = Vec3::new(1.0, 2.0, 3.0);

    group.bench_function("x", |b| {
        b.iter(|| black_box(v.x()));
    });

    group.bench_function("y", |b| {
        b.iter(|| black_box(v.y()));
    });

    group.bench_function("z", |b| {
        b.iter(|| black_box(v.z()));
    });

    group.bench_function("index_0", |b| {
        b.iter(|| black_box(v[0]));
    });

    group.bench_function("index_1", |b| {
        b.iter(|| black_box(v[1]));
    });

    group.bench_function("index_2", |b| {
        b.iter(|| black_box(v[2]));
    });

    group.finish();
}

fn bench_vec3_assignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec3_assignment");

    let v1 = Vec3::new(1.0, 2.0, 3.0);
    let v2 = Vec3::new(4.0, 5.0, 6.0);

    group.bench_function("add_assign", |b| {
        let mut v = v1;
        b.iter(|| {
            v += v2;
            black_box(v);
            v = v1; // Reset
        });
    });

    group.bench_function("sub_assign", |b| {
        let mut v = v1;
        b.iter(|| {
            v -= v2;
            black_box(v);
            v = v1; // Reset
        });
    });

    group.bench_function("mul_assign_scalar", |b| {
        let mut v = v1;
        b.iter(|| {
            v *= 2.5;
            black_box(v);
            v = v1; // Reset
        });
    });

    group.bench_function("div_assign_scalar", |b| {
        let mut v = v1;
        b.iter(|| {
            v /= 2.5;
            black_box(v);
            v = v1; // Reset
        });
    });

    group.finish();
}

fn bench_vec3_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec3_comparison");

    let v1 = Vec3::new(1.0, 2.0, 3.0);
    let v2 = Vec3::new(1.0, 2.0, 3.0);
    let v3 = Vec3::new(4.0, 5.0, 6.0);

    group.bench_function("eq_equal", |b| {
        b.iter(|| black_box(v1 == v2));
    });

    group.bench_function("eq_not_equal", |b| {
        b.iter(|| black_box(v1 == v3));
    });

    group.finish();
}

fn bench_vec3_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec3_creation");

    group.bench_function("new", |b| {
        b.iter(|| black_box(Vec3::new(1.0, 2.0, 3.0)));
    });

    group.bench_function("default", |b| {
        b.iter(|| black_box(Vec3::default()));
    });

    group.bench_function("x_axis", |b| {
        b.iter(|| black_box(Vec3::X_AXIS));
    });

    group.bench_function("y_axis", |b| {
        b.iter(|| black_box(Vec3::Y_AXIS));
    });

    group.bench_function("z_axis", |b| {
        b.iter(|| black_box(Vec3::Z_AXIS));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_vec3_arithmetic,
    bench_vec3_operations,
    bench_vec3_accessors,
    bench_vec3_assignment,
    bench_vec3_comparison,
    bench_vec3_creation
);
criterion_main!(benches);
