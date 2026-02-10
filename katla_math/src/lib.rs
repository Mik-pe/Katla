#![allow(dead_code)]

pub mod aabb;
pub mod color;
pub mod constants;
pub mod frustum;
pub mod mat2;
pub mod mat3;
pub mod mat4;
pub mod plane;
pub mod quat;
pub mod ray;
pub mod rect2d;
pub mod sphere;
pub mod transform;
pub mod utils;

// Vector implementation modules
// Always compile scalar implementations (used for Vec2, Vec3, Vec4, Mat4, Quat, and non-SSE platforms)
mod scalar {
    pub mod vec2;
    pub mod vec3;
    pub mod vec4;
    pub mod quat;
    pub mod mat4;
}

// SSE implementations (only on x86/x86_64, used for Vec4, Mat4, Quat)
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod sse {
    pub mod vec4;
    pub mod quat;
    pub mod mat4;
}

// Vector type re-exports
pub mod vec2;
pub mod vec3;
pub mod vec4;

pub use self::aabb::AABB;
pub use self::color::{Color, HSV};
pub use self::constants::*;
pub use self::frustum::Frustum;
pub use self::mat2::Mat2;
pub use self::mat3::Mat3;
pub use self::mat4::Mat4;
pub use self::plane::{Plane, PlaneSide};
pub use self::quat::Quat;
pub use self::ray::{Ray, RayIntersection};
pub use self::rect2d::Rect2D;
pub use self::sphere::Sphere;
pub use self::transform::Transform;
pub use self::utils::*;
pub use self::vec2::Vec2;
pub use self::vec3::Vec3;
pub use self::vec4::Vec4;

pub fn mat4_mul_vec3(a: &Mat4, b: &Vec3) -> Vec3 {
    let row0 = a.extract_row(0);
    let row1 = a.extract_row(1);
    let row2 = a.extract_row(2);
    Vec3::new(
        b.dot(Vec3::new(row0.x(), row0.y(), row0.z())) + row0.w(),
        b.dot(Vec3::new(row1.x(), row1.y(), row1.z())) + row1.w(),
        b.dot(Vec3::new(row2.x(), row2.y(), row2.z())) + row2.w(),
    )
}

pub fn mat4_mul_vec4(a: &Mat4, b: &Vec4) -> Vec4 {
    let row0 = a.extract_row(0);
    let row1 = a.extract_row(1);
    let row2 = a.extract_row(2);
    let row3 = a.extract_row(3);
    Vec4::new(
        Vec4::dot(&row0, b),
        Vec4::dot(&row1, b),
        Vec4::dot(&row2, b),
        Vec4::dot(&row3, b),
    )
}
