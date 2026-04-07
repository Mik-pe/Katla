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
mod scalar {
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    pub mod mat4;

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    pub mod quat;
    pub mod vec2;
    pub mod vec3;
    pub mod vec4;
}

// SSE implementations (only on x86/x86_64, used for Vec4, Mat4, Quat)
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod sse {
    pub mod mat4;
    pub mod quat;
    pub mod vec4;
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
