//! Quaternion using SIMD (platform intrinsics)
//!
//! Quat uses platform-specific SIMD intrinsics for high-performance operations.
//! Uses SSE on x86/x86_64, scalar fallback on other platforms.
//!
//! Quat is well-suited for SIMD since it perfectly fills a 128-bit register.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use crate::sse::quat::Quat;

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub use crate::scalar::quat::Quat;
