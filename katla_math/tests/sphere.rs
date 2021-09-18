use katla_math::Sphere;
use katla_math::Vec3;

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

    let mut sphere = Sphere::new(Vec3::new(1.0f32, 0.0f32, 0.0f32), radius);

    let point_inside = Vec3::new(0.1, 0.0, 0.0);
    sphere.maybe_expand(point_inside);
    assert_eq!(radius, sphere.radius);

    let point_outside = Vec3::new(-0.1, 0.0, 0.0);
    sphere.maybe_expand(point_outside);
    assert_ne!(radius, sphere.radius);
}

#[test]
fn test_inside() {
    let sphere = Sphere::new(Vec3::new(100.0f32, 0.0f32, 0.0f32), 100.0);
    let point = Vec3::new(90.0, 0.0, 0.0);
    assert_eq!(sphere.point_inside(point), true);
    let point = Vec3::new(0.0, 0.0, 0.0);
    assert_eq!(sphere.point_inside(point), true);
    let point = Vec3::new(-100.0, 0.0, 0.0);
    assert_eq!(sphere.point_inside(point), false);
}

#[test]
fn test_intersect() {
    let sphere1 = Sphere::new(Vec3::new(0.0, 0.0, 0.0), 10.0);
    let sphere2 = Sphere::new(Vec3::new(10.0, 0.0, 0.0), 1.0);
    assert_eq!(sphere1.intersects(&sphere2), true);
    let sphere2 = Sphere::new(Vec3::new(0.0, 11.0, 0.0), 1.0);
    assert_eq!(sphere1.intersects(&sphere2), true);
    let sphere2 = Sphere::new(Vec3::new(0.0, 0.0, 12.0), 1.0);
    assert_eq!(sphere1.intersects(&sphere2), false);
}

#[test]
fn test_into() {
    let list_of_verts: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
    let sphere = Sphere::create_from_verts(&list_of_verts);
    assert_eq!(sphere.radius, 0.5);
}
