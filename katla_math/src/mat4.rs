//! 4x4 Matrix using SIMD (platform intrinsics)
//!
//! Mat4 uses platform-specific SIMD intrinsics for high-performance operations.
//! Uses SSE on x86/x86_64, scalar fallback on other platforms.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use crate::sse::mat4::Mat4;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub use crate::scalar::mat4::Mat4;
