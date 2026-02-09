#![cfg_attr(test, allow(dead_code))]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use katla_math::{Quat, Transform, Vec3};

fn bench_transform_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("transform_creation");

    group.bench_function("new", |b| {
        b.iter(|| black_box(Transform::new()));
    });

    group.bench_function("default", |b| {
        b.iter(|| black_box(Transform::default()));
    });

    group.bench_function("new_from_rotation", |b| {
        let rotation = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5);
        b.iter(|| black_box(Transform::new_from_rotation(rotation)));
    });

    group.bench_function("new_from_position", |b| {
        let position = Vec3::new(1.0, 2.0, 3.0);
        b.iter(|| black_box(Transform::new_from_position(position)));
    });

    group.bench_function("new_from_scale", |b| {
        let scale = Vec3::new(2.0, 2.0, 2.0);
        b.iter(|| black_box(Transform::new_from_scale(scale)));
    });

    group.finish();
}

fn bench_transform_arithmetic(c: &mut Criterion) {
    let mut group = c.benchmark_group("transform_arithmetic");

    let t1 = Transform {
        position: Vec3::new(1.0, 2.0, 3.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5),
    };

    let t2 = Transform {
        position: Vec3::new(4.0, 5.0, 6.0),
        scale: Vec3::new(2.0, 2.0, 2.0),
        rotation: Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 0.5),
    };

    group.bench_function("mul_transform", |b| {
        b.iter(|| black_box(t1 * t2));
    });

    group.bench_function("mul_vec3", |b| {
        let v = Vec3::new(1.0, 0.0, 0.0);
        b.iter(|| black_box(t1 * v));
    });

    group.finish();
}

fn bench_transform_make_mat4(c: &mut Criterion) {
    let mut group = c.benchmark_group("transform_make_mat4");

    let identity = Transform::new();

    let translation_only = Transform {
        position: Vec3::new(1.0, 2.0, 3.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::new(),
    };

    let rotation_only = Transform {
        position: Vec3::new(0.0, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5),
    };

    let scale_only = Transform {
        position: Vec3::new(0.0, 0.0, 0.0),
        scale: Vec3::new(2.0, 2.0, 2.0),
        rotation: Quat::new(),
    };

    let full_transform = Transform {
        position: Vec3::new(1.0, 2.0, 3.0),
        scale: Vec3::new(2.0, 2.0, 2.0),
        rotation: Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5),
    };

    group.bench_function("identity", |b| {
        b.iter(|| black_box(identity.make_mat4()));
    });

    group.bench_function("translation_only", |b| {
        b.iter(|| black_box(translation_only.make_mat4()));
    });

    group.bench_function("rotation_only", |b| {
        b.iter(|| black_box(rotation_only.make_mat4()));
    });

    group.bench_function("scale_only", |b| {
        b.iter(|| black_box(scale_only.make_mat4()));
    });

    group.bench_function("full_transform", |b| {
        b.iter(|| black_box(full_transform.make_mat4()));
    });

    group.finish();
}

fn bench_transform_hierarchy(c: &mut Criterion) {
    let mut group = c.benchmark_group("transform_hierarchy");

    // Simulate a common scene graph pattern:
    // grandparent -> parent -> child -> local_point

    let grandparent = Transform {
        position: Vec3::new(0.0, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.5),
    };

    let parent = Transform {
        position: Vec3::new(1.0, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), 0.3),
    };

    let child = Transform {
        position: Vec3::new(0.0, 1.0, 0.0),
        scale: Vec3::new(0.5, 0.5, 0.5),
        rotation: Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), 0.2),
    };

    let local_point = Vec3::new(0.5, 0.5, 0.5);

    group.bench_function("three_level_hierarchy", |b| {
        b.iter(|| {
            let world_transform = grandparent * (parent * child);
            black_box(world_transform * local_point);
        });
    });

    group.finish();
}

fn bench_transform_composition(c: &mut Criterion) {
    let mut group = c.benchmark_group("transform_composition");

    let base = Transform {
        position: Vec3::new(1.0, 2.0, 3.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 1.5),
    };

    let offset = Transform {
        position: Vec3::new(0.5, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: Quat::new(),
    };

    group.bench_function("compose", |b| {
        b.iter(|| black_box(base * offset));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_transform_creation,
    bench_transform_arithmetic,
    bench_transform_make_mat4,
    bench_transform_hierarchy,
    bench_transform_composition
);
criterion_main!(benches);
