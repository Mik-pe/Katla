//! SSE-accelerated implementation of Mat4
//!
//! This uses SSE intrinsics for high-performance matrix operations.
//! Only available on x86/x86_64 with SSE2 support.

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;

use crate::{Mat4, Vec4};

// SSE shuffle control masks for _mm_shuffle_ps(dest, src, mask)
// Mask format: [dest[3] dest[2] src[1] src[0]] where each nibble selects an element
const SHUFFLE_X: i32 = 0b00_00_00_00; // Broadcast element 0 (x)
const SHUFFLE_Y: i32 = 0b01_01_01_01; // Broadcast element 1 (y)
const SHUFFLE_Z: i32 = 0b10_10_10_10; // Broadcast element 2 (z)
const SHUFFLE_W: i32 = 0b11_11_11_11; // Broadcast element 3 (w)

impl Mat4 {
    /// SIMD-optimized row extraction using shuffle operations
    pub fn extract_row(&self, index: usize) -> Vec4 {
        unsafe {
            match index {
                0 => {
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_X);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_X);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_X);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_X);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                1 => {
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_Y);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_Y);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_Y);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_Y);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                2 => {
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_Z);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_Z);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_Z);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_Z);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                3 => {
                    let c0 = _mm_shuffle_ps(self[0].0, self[0].0, SHUFFLE_W);
                    let c1 = _mm_shuffle_ps(self[1].0, self[1].0, SHUFFLE_W);
                    let c2 = _mm_shuffle_ps(self[2].0, self[2].0, SHUFFLE_W);
                    let c3 = _mm_shuffle_ps(self[3].0, self[3].0, SHUFFLE_W);
                    let tmp01 = _mm_unpacklo_ps(c0, c1);
                    let tmp23 = _mm_unpacklo_ps(c2, c3);
                    Vec4(_mm_movelh_ps(tmp01, tmp23))
                }
                _ => panic!("INDEXING OUT_OF_BOUNDS in Mat4"),
            }
        }
    }

    /// SIMD-optimized transpose using unpack operations
    pub fn transpose(&self) -> Self {
        unsafe {
            let col0 = self[0].0;
            let col1 = self[1].0;
            let col2 = self[2].0;
            let col3 = self[3].0;

            let tmp0 = _mm_unpacklo_ps(col0, col1);
            let tmp2 = _mm_unpackhi_ps(col0, col1);
            let tmp1 = _mm_unpacklo_ps(col2, col3);
            let tmp3 = _mm_unpackhi_ps(col2, col3);

            let res0 = _mm_movelh_ps(tmp0, tmp1);
            let res1 = _mm_movehl_ps(tmp1, tmp0);
            let res2 = _mm_movelh_ps(tmp2, tmp3);
            let res3 = _mm_movehl_ps(tmp3, tmp2);

            Self([Vec4(res0), Vec4(res1), Vec4(res2), Vec4(res3)])
        }
    }
}
