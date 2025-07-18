#![allow(dead_code)]

pub mod aabb;
pub mod mat4;
pub mod quat;
pub mod sphere;
pub mod transform;
pub mod vec3;
pub mod vec4;

pub use self::aabb::AABB;
pub use self::mat4::Mat4;
pub use self::quat::Quat;
pub use self::sphere::Sphere;
pub use self::transform::Transform;
pub use self::vec3::Vec3;
pub use self::vec4::Vec4;

//Assume lower row is 0_0_0_1
pub fn mat4_mul_vec3(a: &Mat4, b: &Vec3) -> Vec3 {
    let row0 = Mat4::extract_row(a, 0);
    let row1 = Mat4::extract_row(a, 1);
    let row2 = Mat4::extract_row(a, 2);
    //TODO: Don't create a new object here:
    Vec3([
        b.dot(Vec3([row0[0], row0[1], row0[2]])) + row0[3],
        b.dot(Vec3([row1[0], row1[1], row1[2]])) + row1[3],
        b.dot(Vec3([row2[0], row2[1], row2[2]])) + row2[3],
    ])
}

pub fn mat4_mul_vec4(a: &Mat4, b: &Vec4) -> Vec4 {
    let row0 = Mat4::extract_row(a, 0);
    let row1 = Mat4::extract_row(a, 1);
    let row2 = Mat4::extract_row(a, 2);
    let row3 = Mat4::extract_row(a, 3);
    Vec4([
        Vec4::dot(&row0, b),
        Vec4::dot(&row1, b),
        Vec4::dot(&row2, b),
        Vec4::dot(&row3, b),
    ])
}
