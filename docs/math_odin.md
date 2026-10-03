# Katla math in Odin

`odin/math` ports the public numeric and geometric responsibilities of
`katla_math`, without external dependencies or an ECS dependency. It is part of
the [progressive Odin tree](../odin/README.md). Rendering still uses Rust math;
this package is a native CPU port, not a foreign representation of Rust values.

## A single rotation contract

Rotations use normalized xyzw `Quat` values. Construct with `quat_axis_angle`,
compose with `quat_mul`, interpolate with `quat_slerp` / `quat_nlerp`, and derive
matrices with `quat_to_mat3` / `quat_to_mat4`. `mat4_rotaxis` calls the same quaternion
constructor, including normalization of its axis. There are no Euler, yaw/pitch
or Euler extraction APIs in this package: an application can compose explicit
axis rotations for camera controls without establishing another math convention.

The coordinate system is right-handed. Camera forward is local `-Z`, up `+Y`,
right `+X`. `parent * child` applies the child first. Quaternion composition and
matrix multiplication follow this same order and produce the same transformed
vectors. A zero axis produces identity; quaternion products and rotation matrix
construction require normalized input. `quat_conjugate` is an inverse only for a
unit quaternion. `quat_rotate_vec3` retains the Rust method's general algebraic
behavior; the rotation operation `quat_transform_vector` assumes unit length.

Matrices store columns: `m[column][row]`. Use `matrix_mul` for matrix composition
and `matrix_vector` for homogeneous column vectors. Array arithmetic is used for
vectors and scalar element arithmetic; it does not substitute for matrix product.
`transform_point` supplies `w=1`, `transform_direction` supplies `w=0`, and neither
performs a perspective divide. `mat4_to_array` serializes columns for uploads.

`transform()` defaults to position zero, unit scale and identity rotation.
`transform_vector` and `transform_to_mat4` both apply local scale, then rotation,
then translation (`T * R * S`). `transform_compose` returns an exact `Mat4` because
rotated nonuniform scales can create shear. `transform_inverse` also returns a
matrix and an `ok` flag. `mat4_decompose_approx` is explicitly approximate: it
extracts positive column lengths, loses reflection signs and shear, and rejects
zero scale. Retain matrices in hierarchy, rendering and bounds calculations.
`transform_look_at` preserves position and scale, orienting local `-Z` to its target.

```odin
import km "path/to/odin/math"

q := km.quat_axis_angle(km.VEC3_Y, km.FRAC_PI_2)
tr := km.transform(km.Vec3{1,2,3}, q, km.Vec3{2,3,4})
p := km.transform_vector(tr, km.Vec3{1,0,0})
// Same result via km.transform_point(km.transform_to_mat4(tr), ...).
```

## Inventory and language mapping

| Rust responsibility | Odin API |
| --- | --- |
| Vec2/3/4 arithmetic, indexing, arrays, axes, zero/one | `Vec2/3/4`, native array operations/indexing, `VEC*` constants; swizzles are array literals |
| Vector norm/dot/lerp/distance | `normalize`, `is_normalized`, `is_zero`, `length`, `length_squared`, `dot`, `lerp`, `distance`, `distance_squared` |
| Cross, 2D angle/perpendicular, reflect/project/reject, clamp, spherical | `cross`, `cross2`, `angle`, `from_angle`, `perpendicular`, `reflect`, `project`, `reject`, `angle_between`, `clamp_length*`, `from_spherical` |
| Vec4 xyz/homogeneous construction | `xyz`, `vec4(v,w)` (default `w=0`; supply `w=1` for points) |
| Mat2/3/4 identity, multiplication, row, transpose, determinant, inverse | `identity(Mat2/3/4)`, `matrix_mul`, `matrix_vector`, `extract_row`, `transpose`, `determinant`, `inverse` |
| Scale/rotation/translation/projection/look-at, matrix conversions | `scale_matrix`, `mat2_rotation`, `mat2_to_rotation`, `mat2_to_scale`, `mat4_scale`, `mat4_rotaxis`, `mat4_translation`, `mat4_trs`, projection functions, `mat4_lookat`, `mat4_to_mat3`, `mat3_to_mat4`, `mat4_extract_*`, `mat4_to_array` |
| Quat axis rotation, vector rotation, shortest rotation, interpolation, matrix conversion | `quat_axis_angle`, `quat_rotation_between`, `quat_mul`, `quat_transform_vector`, `quat_rotate_vec3`, `quat_normalize`, `quat_is_normalized`, `quat_dot`, `quat_conjugate`, `quat_*lerp`, `quat_*mat*` |
| Transform defaults/builders, directions, interpolation, matrix composition | `TRANSFORM_IDENTITY`, `transform` named arguments, value field assignment, `transform_to_mat4`, `transform_vector`, `transform_compose`, `transform_inverse`, `transform_lerp`, `transform_look_*`, `transform_forward/up/right` |
| AABB, Sphere, Rect2D and vertex bounds | Struct literals plus `aabb_*`, `sphere_*`, `rect_*`, `compute_bounds`; rectangle width/height are `rect_size(r)[0/1]`, position is `r.min`, empty-at is `Rect2D{point,point}` |
| Plane, PlaneSide, Ray, RayIntersection | `Plane`, `Plane_Side`, `Ray`, `Ray_Intersection`, `plane_*`, `ray_*` |
| Infinite reverse-Z Frustum and finite visualization bounds | `frustum_from_camera`, `frustum_from_proj_and_lookat/view`, `frustum_contains_*`, `frustum_intersects_*`, `frustum_corners/center/bounding_sphere` with optional finite distance |
| Color, HSV, named colors, hex/bytes, arithmetic, gamma and HSV | Struct literals, `COLOR_*`, `color_*`; HSV hue is degrees, alpha is unchanged by gamma conversion |
| Mathematical constants | `PI`, `TAU`, `FRAC_PI_*`, `DEG_TO_RAD`, `RAD_TO_DEG`, `GOLDEN_RATIO`, `SQRT_3` |

Optional numeric results return `(value, ok)`, including matrix inverses and
intersections. Zero-initialized Odin matrices, quaternions and transforms are
not identity; use the constructors/constants. Colors default to transparent
zero under Odin initialization; use `COLOR_WHITE` when Rust's default is intended.
The package owns no allocations. Geometry inputs follow normal mathematical
preconditions: ordered bounds, nonnegative radii, unit plane normals/directions
when distances are intended as world units, valid projection ranges and finite
values. Empty vertex bounds preserve the reference's sentinel extrema. Projection
and length underflow/overflow behavior is not a general validation service.

Vectors are compact scalar arrays (`Vec2` 8 bytes, `Vec3` 12 bytes, `Vec4` 16 bytes),
matrices are arrays of those columns and quaternions are distinct four-float arrays.
Rust's padded Vec2/Vec3 and 16-byte Vec4/Quat alignment are not an Odin ABI promise.
The port uses scalar inline operations with LLVM optimization; explicit Rust SSE
intrinsics are not translated. Native runtime/SIMD performance is not benchmarked
by the numeric comparison. Any future SIMD implementation must preserve these
contracts and receive its own platform evidence.

## Deliberate differences from the Rust reference

The port has one rotation convention and no Euler functions. Rust's quaternion
Euler constructor uses Y*X*Z while its matrix constructors use X*Y*Z. Those two
paths are not retained. Additional differences resolve inconsistent geometry:

- TRS vector operations use the same local-scale order as their matrices. Exact
  composition/inversion returns matrices; no approximate TRS operator hides shear.
- Look-direction uses camera-to-world rotation consistently with `mat4_lookat`;
  look-at retains the caller's position and scale.
- Orthographic projection places translation in column three, uses Vulkan depth
  `[0,1]` and flips Y, consistently with the perspective constructors. The Rust
  constructor places translation in row three and uses `[-1,1]` depth.
- Finite frustum visualization corners extend forward from the near plane. Rust's
  subtraction places their finite plane behind the near plane. Frustum extraction
  remains specifically infinite reverse-Z, including its disabled far plane.
- Ray/AABB normals point outward, including the correct exit face for inside rays.
  Zero direction returns a miss, and parallel rays on a slab boundary avoid `0*inf`.
- Slerp clamps the normalized dot product before acos; zero-axis rotation produces
  identity; approximate decomposition reports zero scale as failure.

These changes affect only Odin. The Rust engine's production math is unchanged.
Four preexisting Clippy warnings in Rust frustum tests were fixed with iterator
and range expressions; their assertions and numeric behavior remain the same.

## Validation

```sh
python3 scripts/validate_odin.py
odin test odin/math -out:target/odin-math-asan -sanitize:address -debug -vet -strict-style
odin test odin/math -out:target/odin-math-release -o:speed -vet -strict-style
python3 scripts/compare_math_port.py --report docs/benchmarks/math-odin-parity.json
cargo test -p katla_math --locked
cargo check -p katla_math --all-targets --locked
cargo clippy -p katla_math --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

The 25 Odin tests validate matrix/quaternion agreement, column layout, nonsymmetric
inverses, reverse-Z/finite/orthographic clip coordinates, exact hierarchy shear,
negative/zero scales, quaternion interpolation and half-turns, camera orientation,
all eight transformed bounds corners, plane inverse-transpose normals, ray hits,
parallel slab edges, rectangles, gamma/HSV and color bytes. They run with strict
vet/style and test allocation tracking, native AddressSanitizer and optimization.
The ECS/math example advances an actual typed world and transforms its component.
The Rust reference passes 253 tests including doctests, all-target check, strict
all-target Clippy and formatting. An additional `linux_amd64` Odin check verifies
typechecking for x86; execution and sanitizer acceptance remain native arm64.

The paired Rust and Odin consumers run 128 deterministic cases in both dev and
release. Each profile checks 3,712 operation records and 27,008 finite scalar
outputs across 29 operations, including quaternions, arbitrary axes, nonsymmetric
matrix inverse, TRS matrix, bounds, projections, camera, planes and color.
Absolute tolerance is 2e-5, relative tolerance 5e-5. The deliberate differences
above receive independent geometric tests instead of an equality assertion against
the old behavior. [The parity receipt](benchmarks/math-odin-parity.json) records
toolchain versions, per-profile maxima, output hashes and exact source hashes.
This is native CPU evidence on Apple Silicon; it does not establish x86 SIMD
performance, FFI compatibility, rendering acceptance or full-engine migration.
