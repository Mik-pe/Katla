//! Deterministic numeric reference for the Odin math port.
use katla_math::{
    AABB, Color, Frustum, Mat2, Mat3, Mat4, Plane, Quat, Ray, Sphere, Vec2, Vec3, Vec4,
};

fn emit(name: &str, values: &[f32]) {
    print!("{name}");
    for v in values {
        print!(" {v:.9e}");
    }
    println!();
}
fn vector(v: Vec3) -> [f32; 3] {
    v.to_array()
}
fn mat3(a: Mat3) -> [f32; 9] {
    std::array::from_fn(|i| a[i / 3][i % 3])
}
fn main() {
    for i in 0..128 {
        let f = i as f32 * 0.037;
        let axis = Vec3::new(
            (i % 7) as f32 + 1.0,
            (i % 5) as f32 - 2.0,
            (i % 3) as f32 + 0.5,
        );
        let q = Quat::from_axis_angle(axis, f - 2.0);
        let qb = Quat::from_axis_angle(Vec3::Y_AXIS, 0.3 - f * 0.2);
        let a = Vec3::new(1.0 + f, -2.0 + f * 0.1, 3.0 - f * 0.2);
        let b = Vec3::new(-2.0, 3.0 + f, 1.0);
        let v2 = Vec2::new(a[0], a[1]);
        let v4 = Vec4::new(a[0], a[1], a[2], 0.5);
        emit(
            "vector_scalar",
            &[
                a.dot(b),
                a.length(),
                a.distance(b),
                a.angle_between(b),
                v2.cross(Vec2::new(3.0, 4.0)),
                v2.angle(),
                v4.length(),
            ],
        );
        emit("normalize", &vector(a.normalize()));
        emit("cross", &vector(a.cross(b)));
        emit("reflect", &vector(a.reflect(b.normalize())));
        emit("project", &vector(a.project(b)));
        emit("rotate", &vector(q * a));
        emit("rotate_method", &vector(q.rotate_vec3(a)));
        emit("quat_matrix", &mat3(q.to_mat3()));
        emit("quat_product", &mat3((q * qb).to_mat3()));
        emit("slerp", &mat3(Quat::slerp(q, qb, 0.37).to_mat3()));
        emit("nlerp", &mat3(Quat::nlerp(q, qb, 0.37).to_mat3()));
        let scale = Vec3::new(1.5 + f * 0.1, 2.0, 0.75);
        let m = Mat4::from_trs(a, q, scale);
        emit("trs", &m.to_array());
        emit("matrix_point", &vector(m * b));
        emit(
            "matrix_inverse",
            &m.inverse().expect("nonzero scale").to_array(),
        );
        emit("matrix_det", &[m.calc_det()]);
        emit(
            "matrix_product",
            &(m * Mat4::from_translation([1.0, 2.0, 3.0])).to_array(),
        );
        let m2 = Mat2::new(1.0 + f, 2.0, 3.0, 5.0);
        if let Some(inv) = m2.inverse() {
            emit(
                "mat2_inverse",
                &[inv[0][0], inv[0][1], inv[1][0], inv[1][1]],
            );
        }
        let m3 = m.to_mat3();
        emit("mat3_inverse", &mat3(m3.inverse().expect("nonzero scale")));
        let p = Mat4::create_proj_reverse_z(60.0 + f, 1.7, 0.1);
        emit("reverse_z", &p.to_array());
        emit(
            "perspective",
            &Mat4::create_proj_perspective(60.0 + f, 1.7, 0.1, 100.0).to_array(),
        );
        let look = Mat4::create_lookat(a, b, Vec3::Y_AXIS);
        emit("lookat", &look.to_array());
        let fr = Frustum::from_proj_and_lookat(&p, &look);
        emit(
            "frustum_near",
            &[
                fr.near.normal[0],
                fr.near.normal[1],
                fr.near.normal[2],
                fr.near.distance,
            ],
        );
        let bx = AABB::from_min_max(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(2.0, 3.0, 4.0));
        let bounds = bx.transform(&m);
        emit(
            "bounds",
            &[
                bounds.center[0],
                bounds.center[1],
                bounds.center[2],
                bounds.extent[0],
                bounds.extent[1],
                bounds.extent[2],
            ],
        );
        let plane = Plane::from_point_normal(a, b);
        let transformed = plane.transform(&m);
        emit(
            "plane",
            &[
                plane.distance_to_point(b),
                transformed.normal[0],
                transformed.normal[1],
                transformed.normal[2],
                transformed.distance,
            ],
        );
        let ray = Ray::from_points(Vec3::new(0.0, 0.0, 4.0 + f), Vec3::new(0.0, 0.0, 0.0));
        let hit = ray
            .intersects_sphere(&Sphere::new(Vec3::new(0.0, 0.0, 0.0), 1.0))
            .expect("central ray");
        emit(
            "ray_sphere",
            &[
                hit.distance,
                hit.point[0],
                hit.point[1],
                hit.point[2],
                hit.normal[0],
                hit.normal[1],
                hit.normal[2],
            ],
        );
        let color = Color::new(
            (i % 11) as f32 / 11.0,
            (i % 13) as f32 / 13.0,
            (i % 17) as f32 / 17.0,
            0.4,
        );
        emit("linear", &color.to_linear().to_array());
        emit("gamma_roundtrip", &color.to_linear().to_srgb().to_array());
        let hsv = color.to_hsv();
        emit("hsv", &[hsv.h, hsv.s, hsv.v]);
        emit("hsv_roundtrip", &Color::from_hsv(hsv).to_array());
    }
}
