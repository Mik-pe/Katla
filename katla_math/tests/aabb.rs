use katla_math::Vec3;
use katla_math::AABB;

#[test]
fn test_expand() {
    let verts = vec![
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, -10.0, 0.0),
        Vec3::new(0.0, 10.0, -10.0),
    ];
    let aabb = AABB::create_from_verts(&verts);

    assert_eq!(aabb.extent[0], 0.5);
    assert_eq!(aabb.extent[1], 10.0);
    assert_eq!(aabb.extent[2], 5.0);

    assert_eq!(aabb.center[0], 0.5);
    assert_eq!(aabb.center[1], 0.0);
    assert_eq!(aabb.center[2], -5.0);
}
