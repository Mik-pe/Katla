use approx::assert_relative_eq;
use katla_math::{Sphere, Vec3};

#[test]
fn test_expand() {
    let radius = 1.0f32;
    let mut sphere = Sphere::new(Vec3::new(0.0f32, 0.0f32, 0.0f32), radius);

    let point_inside = Vec3::new(0.9, 0.0, 0.0);
    sphere.maybe_expand(point_inside);
    assert_eq!(radius, sphere.radius);

    let point_outside = Vec3::new(1.1, 0.0, 0.0);
    sphere.maybe_expand(point_outside);
    assert_ne!(radius, sphere.radius);
    assert_relative_eq!(sphere.radius, (point_outside - sphere.center).length());

    let mut sphere = Sphere::new(Vec3::new(1.0f32, 0.0f32, 0.0f32), radius);

    let point_inside = Vec3::new(0.1, 0.0, 0.0);
    sphere.maybe_expand(point_inside);
    assert_eq!(radius, sphere.radius);

    let point_outside = Vec3::new(-0.1, 0.0, 0.0);
    sphere.maybe_expand(point_outside);
    assert_ne!(radius, sphere.radius);
    assert_relative_eq!(sphere.radius, (point_outside - sphere.center).length());
}

#[test]
fn test_inside() {
    let sphere = Sphere::new(Vec3::new(100.0f32, 0.0f32, 0.0f32), 100.0);

    let point = Vec3::new(90.0, 0.0, 0.0);
    assert!(sphere.point_inside(point));

    let point = Vec3::new(0.0, 0.0, 0.0);
    assert!(sphere.point_inside(point));

    let point = Vec3::new(-100.0, 0.0, 0.0);
    assert!(!sphere.point_inside(point));

    let point = Vec3::new(200.0, 0.0, 0.0);
    assert!(sphere.point_inside(point));

    let point = Vec3::new(200.1, 0.0, 0.0);
    assert!(!sphere.point_inside(point));
}

#[test]
fn test_intersect() {
    let sphere1 = Sphere::new(Vec3::new(0.0, 0.0, 0.0), 10.0);

    let sphere2 = Sphere::new(Vec3::new(10.0, 0.0, 0.0), 1.0);
    assert!(sphere1.intersects(&sphere2));

    let sphere2 = Sphere::new(Vec3::new(0.0, 11.0, 0.0), 1.0);
    assert!(sphere1.intersects(&sphere2));

    let sphere2 = Sphere::new(Vec3::new(0.0, 0.0, 12.0), 1.0);
    assert!(!sphere1.intersects(&sphere2));

    let sphere2 = Sphere::new(Vec3::new(5.0, 5.0, 5.0), 10.0);
    assert!(sphere1.intersects(&sphere2));

    let sphere2 = Sphere::new(Vec3::new(0.0, 0.0, 0.0), 10.0);
    assert!(sphere1.intersects(&sphere2));
}

#[test]
fn test_create_from_verts() {
    let verts: Vec<[f32; 3]> = vec![
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.0, 2.0, 0.0],
        [0.0, 0.0, 2.0],
    ];
    let sphere = Sphere::create_from_verts(&verts);

    assert_relative_eq!(sphere.center.x(), 1.0);
    assert_relative_eq!(sphere.center.y(), 1.0);
    assert_relative_eq!(sphere.center.z(), 1.0);
    assert_relative_eq!(sphere.radius, 1.73205f32, epsilon = 0.001);
}

#[test]
fn test_create_from_verts_single_point() {
    let verts = [[1.0, 2.0, 3.0]];
    let sphere = Sphere::create_from_verts(&verts);

    assert_relative_eq!(sphere.center.x(), 1.0);
    assert_relative_eq!(sphere.center.y(), 2.0);
    assert_relative_eq!(sphere.center.z(), 3.0);
    assert_eq!(sphere.radius, 0.0);
}

#[test]
fn test_create_from_verts_cube() {
    let verts: Vec<[f32; 3]> = vec![
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.0, 2.0, 0.0],
        [0.0, 0.0, 2.0],
        [2.0, 2.0, 2.0],
    ];
    let sphere = Sphere::create_from_verts(&verts);

    assert_relative_eq!(sphere.center.x(), 1.0);
    assert_relative_eq!(sphere.center.y(), 1.0);
    assert_relative_eq!(sphere.center.z(), 1.0);
    assert_relative_eq!(sphere.radius, 1.73205f32, epsilon = 0.001);
}

#[test]
fn test_create_from_verts_negative_coordinates() {
    let verts: Vec<[f32; 3]> = vec![[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]];
    let sphere = Sphere::create_from_verts(&verts);

    assert_relative_eq!(sphere.center.x(), 0.0);
    assert_relative_eq!(sphere.center.y(), 0.0);
    assert_relative_eq!(sphere.center.z(), 0.0);
    assert_relative_eq!(sphere.radius, 1.73205f32, epsilon = 0.001);
}

#[test]
fn test_sphere_contains_all_verts() {
    let verts: Vec<[f32; 3]> = vec![
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.0, 2.0, 0.0],
        [0.0, 0.0, 2.0],
    ];
    let sphere = Sphere::create_from_verts(&verts);

    for vert in &verts {
        let point = Vec3::new(vert[0], vert[1], vert[2]);
        assert!(sphere.point_inside(point));
    }
}

#[test]
fn test_sphere_new() {
    let center = Vec3::new(1.0, 2.0, 3.0);
    let radius = 5.0;
    let sphere = Sphere::new(center, radius);

    assert_eq!(sphere.center.x(), 1.0);
    assert_eq!(sphere.center.y(), 2.0);
    assert_eq!(sphere.center.z(), 3.0);
    assert_eq!(sphere.radius, 5.0);
}

#[test]
fn test_sphere_clone() {
    let sphere1 = Sphere::new(Vec3::new(1.0, 2.0, 3.0), 5.0);
    let sphere2 = sphere1.clone();

    assert_eq!(sphere1.center, sphere2.center);
    assert_eq!(sphere1.radius, sphere2.radius);
}
