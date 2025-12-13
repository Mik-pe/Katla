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

#[test]
fn test_aabb_intersects() {
    let aabb1 = AABB {
        center: Vec3([0.0, 0.0, 0.0]),
        extent: Vec3([1.0, 1.0, 1.0]),
    };

    let aabb2 = AABB {
        center: Vec3([0.5, 0.5, 0.5]),
        extent: Vec3([1.0, 1.0, 1.0]),
    };

    assert!(aabb1.intersects(&aabb2));
}

#[test]
fn test_aabb_does_not_intersect() {
    let aabb1 = AABB {
        center: Vec3([0.0, 0.0, 0.0]),
        extent: Vec3([1.0, 1.0, 1.0]),
    };

    let aabb2 = AABB {
        center: Vec3([-2.0, -2.0, -2.0]),
        extent: Vec3([0.9, 0.9, 0.9]),
    };

    assert!(!aabb1.intersects(&aabb2));
}

#[test]
fn test_aabb_create_from_verts() {
    let verts = vec![
        Vec3([-1.0, -1.0, -1.0]),
        Vec3([1.0, -1.0, -1.0]),
        Vec3([1.0, 1.0, -1.0]),
        Vec3([-1.0, 1.0, -1.0]),
        Vec3([-1.0, -1.0, 1.0]),
        Vec3([1.0, -1.0, 1.0]),
        Vec3([1.0, 1.0, 1.0]),
        Vec3([-1.0, 1.0, 1.0]),
    ];

    let aabb = AABB::create_from_verts(&verts);

    assert_eq!(aabb.center, Vec3([0.0, 0.0, 0.0]));
    assert_eq!(aabb.extent, Vec3([1.0, 1.0, 1.0]));
}
