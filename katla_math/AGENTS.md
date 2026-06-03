# katla_math

SIMD math library. Zero dependencies on other crates.

## Rules

- **Column-major only.** `Mat4(pub [Vec4; 4])` — each Vec4 is a column. `m[col][row]`. Do NOT transpose or swap rows/columns. This matches Vulkan/GLSL.
- Vec2/Vec3 are scalar (SSE not worth it for 3 components). Vec4/Mat4/Quat use SSE on x86/x86_64.
- Hot-path functions are `#[inline]`. Follow this pattern for new operations.

## Conventions

- Use `Transform` for position/rotation/scale — it has `make_mat4()` for the composed matrix.
- Colors in spawning functions are sRGB, converted to linear internally.
- Read `memory-bank/systemPatterns.md` for the full type listing.
