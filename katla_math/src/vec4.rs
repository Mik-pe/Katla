//! 4-dimensional vector using SIMD (platform intrinsics)
//!
//! Vec4 uses platform-specific SIMD intrinsics for high-performance operations.
//! Uses SSE on x86/x86_64, scalar fallback on other platforms.
//!
//! Vec4 is well-suited for SIMD since it perfectly fills a 128-bit register.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use crate::sse::vec4::Vec4;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub use crate::scalar::vec4::Vec4;
