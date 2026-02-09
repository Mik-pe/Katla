# katla_math Implementation Plan

This document outlines improvements and additions to the katla_math crate for a complete 3D game math library.

## ✅ Phase 0 Complete: Benchmark Infrastructure

**Status**: Benchmark suite is set up and ready for use!

Before making ANY optimizations, run benchmarks to establish baselines:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark suite
cargo bench --bench vec3_bench
cargo bench --bench mat4_bench
cargo bench --bench quat_bench
cargo bench --bench transform_bench

# Save baseline for comparison
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main
```

**HTML Reports**: Generated in `target/criterion/reports/index.html`

See `benches/README.md` for detailed documentation.

## Priority Legend

- 🔴 **HIGH** - Critical for 3D rendering, commonly used operations
- 🟡 **MEDIUM** - Useful features that improve API completeness
- 🟢 **LOW** - Nice-to-have features for specific use cases

---

## Part 1: Complete Existing Types

### Vec4 Completion 🔴

**Current State**: Minimal implementation - only indexing and basic conversion

**Missing Operations**:
```rust
// Arithmetic traits (Vec3 has these)
impl Add for Vec4
impl Add<&Vec4> for Vec4
impl Add<Vec4> for &Vec4
impl Add<&Vec4> for &Vec4
impl Sub for Vec4
impl Mul<f32> for Vec4
impl Mul<Vec4> for Vec4  // component-wise
impl Div<f32> for Vec4
impl Div<Vec4> for Vec4  // component-wise
impl Neg for Vec4

// Assignment traits
impl AddAssign for Vec4
impl SubAssign for Vec4
impl MulAssign<f32> for Vec4
impl DivAssign<f32> for Vec4

// Instance methods
pub fn length(&self) -> f32
pub fn length_squared(&self) -> f32
pub fn normalize(&self) -> Vec4
pub fn is_normalized(&self) -> bool
pub fn is_zero(&self) -> bool
pub fn dot(&self, other: &Vec4) -> f32
pub fn lerp(&self, other: &Vec4, t: f32) -> Vec4
pub fn xyz(&self) -> Vec3  // extract first 3 components

// Constants
pub const ZERO: Vec4
pub const ONE: Vec4
pub const X_AXIS: Vec4
pub const Y_AXIS: Vec4
pub const Z_AXIS: Vec4
pub const W_AXIS: Vec4

// Comparison traits (Vec2 has these, Vec3 should too)
impl PartialEq for Vec4
impl PartialOrd for Vec4  // based on length
```

**Tests Needed**:
- Arithmetic operations (add, sub, mul, div)
- Normalization (zero vector, unit vector, non-zero vector)
- Dot product properties (commutative, distributive)
- Lerp interpolation
- xyz() extraction
- Edge cases (NaN, infinity)

---

### Mat4 Improvements 🔴

**Missing Operations**:

```rust
// Trait implementations for matrix-vector multiplication
impl Mul<Vec3> for Mat4
impl Mul<&Vec3> for &Mat4
impl Mul<Vec4> for Mat4
impl Mul<&Vec4> for &Mat4

// New methods
pub fn transpose(&self) -> Mat4
pub fn transpose_mut(&mut self) -> &mut Self

pub fn from_scale(scale: Vec3) -> Self
pub fn from_rotation(quat: Quat) -> Self
pub fn from_euler_angles(pitch: f32, yaw: f32, roll: f32) -> Self
pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self

// Decomposition methods
pub fn extract_translation(&self) -> Vec3
pub fn extract_rotation(&self) -> Quat
pub fn extract_scale(&self) -> Vec3

// Optimized determinant using LU decomposition or cofactor expansion
// Current: 24-term brute force expansion
// Improved: Use cofactor expansion along row/column with most zeros

// Truncate to Mat3 (for normal matrix calculations)
pub fn to_mat3(&self) -> Mat3  // requires Mat3 type
```

**Rationale**:
- Matrix-vector multiplication via traits is more ergonomic than free functions
- Transpose needed for normal matrix calculations
- Decomposition needed for transform manipulation
- TRS composition is common in scene graphs

**Tests Needed**:
- Transpose: double transpose = original
- Scale matrix: verify diagonal elements
- Euler to matrix: test gimbal lock scenarios
- Decomposition: extract and recompose to verify
- Optimized determinant: verify matches current implementation

---

### Quat Improvements 🔴

**Missing Operations**:

```rust
// Matrix conversion
impl From<Mat4> for Quat  // extract rotation from 3x3 portion
impl From<Mat3> for Quat  // requires Mat3 type

// Full Euler angle support
pub fn from_euler(pitch: f32, yaw: f32, roll: f32) -> Self
pub fn to_euler(&self) -> (f32, f32, f32)  // (pitch, yaw, roll)

// Conjugation (more efficient than inverse for unit quaternions)
pub fn conjugate(&self) -> Quat

// Integration with Mat4
pub fn to_mat3(&self) -> Mat3  // 3x3 rotation matrix
```

**Notes**:
- Conjugate is faster than inverse for rotation quaternions (no normalization)
- Matrix to quaternion requires handling of non-uniform scaling (should be warned or documented)
- Euler angles are lossy (gimbal lock), should document limitations

**Tests Needed**:
- From matrix: known rotation matrices to quaternions
- To matrix: round-trip quat → mat → quat
- Euler conversions: test common angles, gimbal lock
- Conjugate: verify q * conjugate(q) = identity (for unit quaternions)

---

### Transform Improvements 🟡

**Missing Operations**:

```rust
// Inverse transform
pub fn inverse(&self) -> Transform

// Decomposition (for inspecting transforms)
pub fn decomposition(&self) -> (Vec3, Quat, Vec3)  // (pos, rot, scale)

// LookAt constructor
pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self

// Lerp between transforms
pub fn lerp(&self, other: &Transform, t: f32) -> Transform

// Apply transform hierarchy
pub fn apply_to_parent(&self, parent: &Transform) -> Transform
```

**Notes**:
- Inverse requires inverting scale (1.0/scale), negating rotation, negating position
- LookAt is useful for camera controllers
- Lerp is useful for animation blending

**Tests Needed**:
- Inverse: transform * inverse = identity
- LookAt: verify target is in front of eye
- Lerp: verify t=0 returns self, t=1 returns other

---

### Vec3 Additions 🟡

**Missing Useful Methods**:

```rust
// Reflection (for physics, mirrors)
pub fn reflect(&self, normal: Vec3) -> Vec3

// Projection onto another vector
pub fn project(&self, onto: Vec3) -> Vec3

// Rejection (perpendicular component)
pub fn reject(&self, from: Vec3) -> Vec3

// Distance calculations
pub fn distance(&self, other: &Vec3) -> f32
pub fn distance_squared(&self, other: &Vec3) -> f32

// Angle between vectors
pub fn angle_between(&self, other: &Vec3) -> f32

// Clamp to magnitude
pub fn clamp_length(&self, max: f32) -> Vec3
pub fn clamp_length_min_max(&self, min: f32, max: f32) -> Vec3

// Swizzle operations (GLSL-style)
pub fn xxx(&self) -> Vec3
pub fn xyx(&self) -> Vec3
pub fn yzx(&self) -> Vec3
// ... (all 27 permutations, or use macro)

// Direction vectors from spherical coordinates
pub fn from_spherical(phi: f32, theta: f32) -> Vec3
```

**Rationale**:
- Reflect/project/reject are essential for physics and lighting calculations
- Distance and angle are common operations
- Swizzling improves ergonomics when matching GLSL shaders

**Tests Needed**:
- Reflect: angle of incidence = angle of reflection
- Project: verify perpendicularity with rejection
- Distance: verify triangle inequality
- Angle: known vectors, parallel/orthogonal cases

---

### Vec2 Additions 🟡

**Missing Useful Methods**:

```rust
// Matching Vec3 API
pub fn length(&self) -> f32
pub fn length_squared(&self) -> f32
pub fn normalize(&self) -> Vec2
pub fn is_normalized(&self) -> bool
pub fn is_zero(&self) -> bool
pub fn dot(&self, other: &Vec2) -> f32
pub fn lerp(&self, other: &Vec2, t: f32) -> Vec2
pub fn cross(&self, other: &Vec2) -> f32  // 2D cross product returns scalar (z-component)

// Perpendicular vector (rotated 90 degrees)
pub fn perpendicular(&self) -> Vec2

// Angle
pub fn angle(&self) -> f32  // angle from +X axis
pub fn from_angle(angle: f32) -> Vec2

// Distance
pub fn distance(&self, other: &Vec2) -> f32
pub fn distance_squared(&self, other: &Vec2) -> f32

// Swizzling
pub fn xx(&self) -> Vec2
pub fn yx(&self) -> Vec2
pub fn yy(&self) -> Vec2
```

**Tests Needed**:
- Perpendicular: verify 90-degree rotation
- Angle: known angles (0, 45, 90, 180 degrees)
- Cross: verify with Vec3 cross product (z-component)

---

### Color Additions 🟢

**Missing Operations**:

```rust
// Color spaces
pub fn to_hsl(&self) -> (f32, f32, f32)
pub fn from_hsl(h: f32, s: f32, l: f32) -> Color

// Premultiplied alpha
pub fn premultiply(&self) -> Color

// Color distance (for palette quantization)
pub fn distance_euclidean(&self, other: &Color) -> f32
pub fn distance_delta_e(&self, other: &Color) -> f32  // perceptually uniform

// Grayscale conversion
pub fn to_grayscale(&self) -> Color

// Color temperature
pub fn from_temperature(kelvin: f32) -> Color

// Gamma correction
pub fn with_gamma(&self, gamma: f32) -> Color
```

**Tests Needed**:
- HSL round-trip: color → HSL → color ≈ original
- Premultiplied: verify alpha blending correctness
- Grayscale: verify luminance weights

---

## Part 2: New Types

### Mat3 🔴

**Purpose**: 3x3 matrix for 3D rotations, scales, and normal transformations

**Structure**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3(pub [Vec3; 3]);

// Represents:
// | m00 m01 m02 |
// | m10 m11 m12 |
// | m20 m21 m22 |
```

**Required Methods**:
```rust
// Creation
pub fn new(m00: f32, m01: f32, m02: f32, ...) -> Self
pub fn identity() -> Self
pub fn from_scale(scale: Vec3) -> Self
pub fn from_rotation(quat: Quat) -> Self
pub fn from_euler_angles(pitch: f32, yaw: f32, roll: f32) -> Self

// Operations
pub fn mul(&self, other: &Mat3) -> Mat3
pub fn transpose(&self) -> Mat3
pub fn determinant(&self) -> f32
pub fn inverse(&self) -> Option<Mat3>  // None if singular

// Vector multiplication
impl Mul<Vec3> for Mat3
impl Mul<&Vec3> for &Mat3

// Conversions
impl From<Quat> for Mat3
impl From<Mat4> for Mat3  // extract 3x3 portion
pub fn to_mat4(&self) -> Mat4  // embed in 4x4

// Traits
impl Default for Mat3  // identity
```

**Use Cases**:
- Normal matrix for vertex shaders (transpose(inverse(model_matrix)))
- Rotation/scale without translation
- 2D affine transformations in 3D space

**Tests Needed**:
- Identity: mul with identity = original
- Determinant: known matrices, singular matrix = 0
- Inverse: mat * inverse = identity (if non-singular)
- Round-trip: Mat4 → Mat3 → Mat4 (3x3 portion preserved)

---

### Mat2 🟡

**Purpose**: 2x2 matrix for 2D rotations, scales, and shears

**Structure**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat2(pub [Vec2; 2]);

// Represents:
// | m00 m01 |
// | m10 m11 |
```

**Required Methods**:
```rust
// Creation
pub fn new(m00: f32, m01: f32, m10: f32, m11: f32) -> Self
pub fn identity() -> Self
pub fn from_rotation(angle: f32) -> Self
pub fn from_scale(x: f32, y: f32) -> Self

// Operations
pub fn determinant(&self) -> f32
pub fn transpose(&self) -> Mat2
pub fn inverse(&self) -> Option<Mat2>

// Vector multiplication
impl Mul<Vec2> for Mat2

// Conversions
impl From<Mat3> for Mat2  // extract 2x2 portion
pub fn to_mat3(&self) -> Mat3  // embed in 3x3

// Traits
impl Default for Mat2  // identity
```

**Use Cases**:
- 2D sprite transformations
- 2D rotation and scale
- Texture coordinate transformations

**Tests Needed**:
- Rotation matrix: verify axis-aligned vectors rotate correctly
- Determinant: det = area scale factor
- Inverse: mat * inverse = identity

---

### Plane 🔴

**Purpose**: Mathematical plane defined by normal and distance from origin, for collision and culling

**Structure**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub distance: f32,  // distance from origin along normal
}
```

**Required Methods**:
```rust
// Creation
pub fn new(normal: Vec3, distance: f32) -> Self
pub fn from_point_normal(point: Vec3, normal: Vec3) -> Self
pub fn from_points(a: Vec3, b: Vec3, c: Vec3) -> Self  // 3 points define a plane

// Point operations
pub fn distance_to_point(&self, point: Vec3) -> f32
pub fn contains_point(&self, point: Vec3, tolerance: f32) -> bool
pub fn closest_point(&self, point: Vec3) -> Vec3

// Side operations (for frustum culling)
pub fn which_side(&self, point: Vec3) -> PlaneSide  // Front, Back, Intersecting

// Intersection tests
pub fn intersects_aabb(&self, aabb: &AABB) -> bool
pub fn intersects_sphere(&self, sphere: &Sphere) -> bool
pub fn intersects_ray(&self, ray: &Ray) -> Option<f32>  // distance along ray

// Transform
pub fn transform(&self, matrix: &Mat4) -> Plane  // transform by matrix

// Normalization (if normal isn't unit length)
pub fn normalize(&self) -> Plane
```

**Supporting Type**:
```rust
pub enum PlaneSide {
    Front,
    Back,
    Intersecting,
}
```

**Use Cases**:
- Frustum culling (6 planes define a frustum)
- Collision detection
- Half-space tests
- Reflection planes

**Tests Needed**:
- From points: verify all 3 points lie on plane
- Distance: verify with known points
- Side tests: point in front, behind, on plane
- Intersection: ray parallel, perpendicular, intersecting

---

### Ray 🔴

**Purpose**: Ray with origin and direction for ray casting and intersection tests

**Structure**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,  // should be normalized
}
```

**Required Methods**:
```rust
// Creation
pub fn new(origin: Vec3, direction: Vec3) -> Self
pub fn from_points(start: Vec3, end: Vec3) -> Self

// Point at distance
pub fn at(&self, distance: f32) -> Vec3

// Intersection tests
pub fn intersects_plane(&self, plane: &Plane) -> Option<Vec3>  // intersection point
pub fn intersects_aabb(&self, aabb: &AABB) -> Option<RayIntersection>
pub fn intersects_sphere(&self, sphere: &Sphere) -> Option<RayIntersection>
pub fn intersects_triangle(&self, a: Vec3, b: Vec3, c: Vec3) -> Option<Vec3>

// Distance
pub fn distance_to_point(&self, point: Vec3) -> f32

// Transform
pub fn transform(&self, matrix: &Mat4) -> Ray
```

**Supporting Type**:
```rust
pub struct RayIntersection {
    pub point: Vec3,
    pub distance: f32,
    pub normal: Vec3,  // surface normal at intersection
}
```

**Use Cases**:
- Mouse picking (ray from camera through mouse position)
- Physics raycasting
- Line of sight checks
- Hit testing

**Tests Needed**:
- At distance: verify point is on ray
- Plane intersection: parallel, perpendicular, intersecting
- AABB intersection: ray inside, outside, through corner
- Sphere intersection: tangent, through center, miss

---

### Frustum 🔴

**Purpose**: View frustum for frustum culling (optimization to skip off-screen objects)

**Structure**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    pub left: Plane,
    pub right: Plane,
    pub top: Plane,
    pub bottom: Plane,
    pub near: Plane,
    pub far: Plane,
}
```

**Required Methods**:
```rust
// Creation
pub fn new(left: Plane, right: Plane, top: Plane, bottom: Plane, near: Plane, far: Plane) -> Self
pub fn from_projection_view_matrix(proj: &Mat4, view: &Mat4) -> Self
pub fn from_camera(position: Vec3, target: Vec3, up: Vec3, fov: f32, aspect: f32, near: f32, far: f32) -> Self

// Intersection tests
pub fn contains_point(&self, point: Vec3) -> bool
pub fn intersects_aabb(&self, aabb: &AABB) -> bool
pub fn intersects_sphere(&self, sphere: &Sphere) -> bool
pub fn contains_aabb(&self, aabb: &AABB) -> bool  // fully inside

// Corner extraction (for debugging/visualization)
pub fn corners(&self) -> [Vec3; 8]

// Center and bounds
pub fn center(&self) -> Vec3
pub fn bounding_sphere(&self) -> Sphere
```

**Use Cases**:
- Frustum culling (skip rendering off-screen objects)
- Portal rendering
- Occlusion culling pre-filter

**Tests Needed**:
- From matrix: verify with known camera configuration
- Contains: point inside, outside, on boundary
- Intersects: AABB fully inside, partially inside, fully outside
- Corners: verify 8 corners form valid frustum

---

### Rect2D 🟡

**Purpose**: 2D axis-aligned rectangle for UI, viewport management, and 2D culling

**Structure**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect2D {
    pub min: Vec2,
    pub max: Vec2,
}

// Alternative representation (use whichever is more ergonomic):
pub struct Rect2D {
    pub origin: Vec2,  // bottom-left corner
    pub size: Vec2,    // width and height
}
```

**Required Methods**:
```rust
// Creation
pub fn new(min: Vec2, max: Vec2) -> Self
pub fn from_origin_size(origin: Vec2, size: Vec2) -> Self
pub fn from_center(center: Vec2, size: Vec2) -> Self

// Properties
pub fn width(&self) -> f32
pub fn height(&self) -> f32
pub fn size(&self) -> Vec2
pub fn center(&self) -> Vec2
pub fn area(&self) -> f32

// Containment
pub fn contains_point(&self, point: Vec2) -> bool
pub fn contains_rect(&self, other: &Rect2D) -> bool

// Intersection
pub fn intersects(&self, other: &Rect2D) -> bool
pub fn intersection(&self, other: &Rect2D) -> Option<Rect2D>
pub fn union(&self, other: &Rect2D) -> Rect2D

// Expansion
pub fn expand(&self, amount: Vec2) -> Rect2D
pub fn expand_to_include(&self, point: Vec2) -> Rect2D

// Conversions
impl From<(Vec2, Vec2)> for Rect2D
impl From<[f32; 4]> for Rect2D  // [x, y, width, height]
```

**Use Cases**:
- UI element bounding boxes
- Viewport and scissor rectangles
- Texture atlas regions
- Sprite bounding boxes

**Tests Needed**:
- Contains: point inside, outside, on edge
- Intersects: adjacent, overlapping, disjoint
- Union: verify bounds
- Expansion: verify increased size

---

### Angle 🟢

**Purpose**: Type-safe angle wrapper to prevent radian/degree confusion

**Structure**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Angle {
    pub radians: f32,
}
```

**Required Methods**:
```rust
// Creation
pub fn radians(radians: f32) -> Self
pub fn degrees(degrees: f32) -> Self
pub fn from revolutions(revolutions: f32) -> Self  // full rotations

// Conversions
pub fn to_radians(&self) -> f32
pub fn to_degrees(&self) -> f32
pub fn to_revolutions(&self) -> f32

// Operations
impl Add for Angle
impl Sub for Angle
impl Mul<f32> for Angle  // scale angle
impl Div<f32> for Angle

// Trigonometry (wraps std functions for clarity)
pub fn sin(&self) -> f32
pub fn cos(&self) -> f32
pub fn tan(&self) -> f32

pub fn asin(value: f32) -> Angle
pub fn acos(value: f32) -> Angle
pub fn atan2(y: f32, x: f32) -> Angle

// Normalization
pub fn normalized(&self) -> Angle  // wrap to [0, 2π)
pub fn normalized_signed(&self) -> Angle  // wrap to [-π, π)

// Lerp
pub fn lerp(&self, other: Angle, t: f32) -> Angle

// Conversion helpers
pub fn to_vec2(&self) -> Vec2  // unit vector at this angle
pub fn from_vec2(v: Vec2) -> Angle  // angle of vector
```

**Use Cases**:
- API clarity (degrees vs radians is explicit)
- Preventing unit confusion in game code
- cleaner trigonometric code

**Tests Needed**:
- Round-trip: degrees → Angle → degrees = original
- Normalization: verify wrapping
- Trig: verify with known angles
- Vec2 conversion: verify angle matches vector direction

---

## Part 3: Utility Functions

### Math Constants Module 🟡

**Purpose**: Centralize mathematical constants

```rust
pub mod constants {
    pub const PI: f32 = std::f32::consts::PI;
    pub const TAU: f32 = 2.0 * PI;
    pub const DEG_TO_RAD: f32 = PI / 180.0;
    pub const RAD_TO_DEG: f32 = 180.0 / PI;
    pub const EPSILON: f32 = 1e-6;
    pub const FLOAT_MAX: f32 = f32::MAX;
    pub const FLOAT_MIN: f32 = f32::MIN;

    // Golden ratio
    pub const PHI: f32 = 1.618033988749895;

    // Square roots
    pub const SQRT_2: f32 = 1.4142135623730951;
    pub const SQRT_3: f32 = 1.7320508075688772;

    // Useful angles in radians
    pub const DEG_0: f32 = 0.0;
    pub const DEG_30: f32 = PI / 6.0;
    pub const DEG_45: f32 = PI / 4.0;
    pub const DEG_60: f32 = PI / 3.0;
    pub const DEG_90: f32 = PI / 2.0;
    pub const DEG_120: f32 = 2.0 * PI / 3.0;
    pub const DEG_135: f32 = 3.0 * PI / 4.0;
    pub const DEG_150: f32 = 5.0 * PI / 6.0;
    pub const DEG_180: f32 = PI;
    pub const DEG_270: f32 = 3.0 * PI / 2.0;
    pub const DEG_360: f32 = 2.0 * PI;
}
```

### Common Functions Module 🟡

**Purpose**: Reusable math operations

```rust
pub mod utils {
    // Clamping
    pub fn clamp<T>(value: T, min: T, max: T) -> T
    where T: PartialOrd

    // Range checking
    pub fn in_range(value: f32, min: f32, max: f32) -> bool

    // Comparison with tolerance
    pub fn approx_equal(a: f32, b: f32, epsilon: f32) -> bool

    // Sign functions
    pub fn sign(value: f32) -> f32  // returns -1.0, 0.0, or 1.0
    pub fn sign_nonzero(value: f32) -> f32  // returns -1.0 or 1.0

    // Interpolation
    pub fn remap(value: f32, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> f32
    pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32
    pub fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32

    // Float manipulation
    pub fn fract(value: f32) -> f32  // fractional part
    pub fn floor(value: f32) -> f32
    pub fn ceil(value: f32) -> f32
    pub fn round(value: f32) -> f32
    pub fn trunc(value: f32) -> f32

    // Power functions
    pub fn sqrt(value: f32) -> f32
    pub fn inverse_sqrt(value: f32) -> f32  // fast inverse sqrt
    pub fn pow(base: f32, exp: f32) -> f32

    // Min/Max
    pub fn min(a: f32, b: f32) -> f32
    pub fn max(a: f32, b: f32) -> f32
    pub fn min3(a: f32, b: f32, c: f32) -> f32
    pub fn max3(a: f32, b: f32, c: f32) -> f32

    // Absolute value
    pub fn abs(value: f32) -> f32

    // Modulo
    pub fn mod_f(value: f32, modulus: f32) -> f32
}
```

---

## Part 4: Performance Optimizations

### Inline Attributes 🔴

**Action**: Add `#[inline]` to hot path methods

**Candidates**:
```rust
// Vec3
#[inline] pub fn length(&self) -> f32
#[inline] pub fn length_squared(&self) -> f32
#[inline] pub fn dot(&self, other: &Vec3) -> f32
#[inline] pub fn x(&self) -> f32
#[inline] pub fn y(&self) -> f32
#[inline] pub fn z(&self) -> f32

// Vec4
#[inline] pub fn dot(&self, other: &Vec4) -> f32

// Quat
#[inline] pub fn dot(&self, other: &Quat) -> f32

// Mat4
#[inline] pub fn mul(&self, other: &Mat4) -> Mat4
```

**Rationale**: Small, frequently-called methods benefit from inlining

### Mat4 Determinant Optimization 🟡

**Current**: Brute-force 24-term expansion

**Improved**: Use cofactor expansion or LU decomposition

```rust
// Option 1: Cofactor expansion along first row
pub fn calc_det(&self) -> f32 {
    // 4 determinants of 3x3 matrices
    // Each 3x3 det is 3 terms (Sarrus' rule)
    // Total: 12 multiplications vs 24 currently
}

// Option 2: LU decomposition (better for repeated determinants)
// Only worthwhile if computing many determinants
```

**Expected Improvement**: ~2x speedup for determinant calculation

### SIMD Support (Future Enhancement) 🟢

**Note**: SIMD is desired but deferred to after the scalar API is complete and benchmarked.

**Action**: Plan for SIMD implementation with feature flag

**Approach**:
- Start with scalar implementation for API clarity and correctness
- Add benchmark suite to identify actual bottlenecks (not premature optimization)
- Use `#[cfg(feature = "simd")]` for SIMD implementations
- Leverage Rust's portable SIMD (`std::simd`) when stable, or use `wide` crate as interim
- Ensure SIMD and scalar implementations produce identical results (within floating-point precision)

**Candidates for SIMD**:
- Vec3/Vec4: arithmetic operations, dot product, normalize
- Mat4: matrix multiplication, transpose
- Quat: multiplication, slerp
- Transform: matrix generation

**Rationale**:
- Must have benchmarks first to prove optimization value
- SIMD adds complexity (alignment, shuffle operations, platform differences)
- Rust's portable SIMD is still maturing
- Feature flag allows users to opt-in based on their target platform

---

## Part 5: Documentation Improvements

### Module Documentation 🟡

**Action**: Add `//!` module-level documentation

**Template**:
```rust
//! # katla_math
//!
//! A linear algebra library for 3D graphics and game development.
//!
//! ## Features
//!
//! - Vector types: [`Vec2`], [`Vec3`], [`Vec4`]
//! - Matrix types: [`Mat2`], [`Mat3`], [`Mat4`]
//! - Quaternion: [`Quat`]
//! - Transform: [`Transform`]
//! - Geometric types: [`AABB`], [`Sphere`], [`Plane`], [`Ray`], [`Frustum`]
//! - Color: [`Color`]
//!
//! ## Design Principles
//!
//! - **Scalar-first**: Clean, correct scalar implementation with SIMD opt-in via feature flag
//! - **Benchmark-driven**: Optimizations guided by actual performance data, not assumptions
//! - **Ergonomic**: Operator overloads for natural math syntax
//! - **Tested**: Comprehensive unit tests for all operations
//!
//! ## Example
//!
//! ```rust
//! use katla_math::{Vec3, Quat, Transform};
//!
//! let position = Vec3::new(1.0, 2.0, 3.0);
//! let rotation = Quat::from_axis_angle(Vec3::Y_AXIS, std::f32::consts::PI / 4.0);
//! let transform = Transform::new(position, Vec3::ONE, rotation);
//! ```

## Usage

See individual type documentation for details.
```

### Struct Documentation 🟡

**Action**: Add usage examples to major types

**Example for Vec3**:
```rust
/// A 3-dimensional vector.
///
/// # Examples
///
/// ```
/// use katla_math::Vec3;
///
/// let a = Vec3::new(1.0, 2.0, 3.0);
/// let b = Vec3::new(4.0, 5.0, 6.0);
///
/// // Arithmetic
/// let sum = a + b;
/// let scaled = a * 2.0;
///
/// // Vector operations
/// let dot = a.dot(&b);
/// let cross = a.cross(&b);
/// let normalized = a.normalize();
///
/// // Interpolation
/// let lerped = a.lerp(&b, 0.5);
/// ```
```

---

## Part 6: Testing Infrastructure

### Property-Based Testing 🟢

**Action**: Consider adding `proptest` for property-based tests

**Examples**:
```rust
// Vec3 addition should be commutative
prop_assert_eq!(a + b, b + a, "addition is commutative");

// Normalized vector should have length 1
let normalized = a.normalize();
prop_assert!(normalized.is_normalized(), "normalize produces unit vector");

// Dot product is distributive
prop_assert_eq!(a.dot(&b) + a.dot(&c), a.dot(&(b + c)), "dot is distributive");
```

### Benchmark Suite 🟢

**Action**: Add `criterion` benchmarks for performance-critical operations

**Candidates**:
- Vec3: normalize, cross, dot
- Mat4: mul, inverse, determinant
- Quat: mul, slerp, rotate_vec3
- Transform: mul, make_mat4

**Purpose**: Detect performance regressions, guide optimization efforts

---

## Implementation Priority Order

### ✅ Phase 0: Infrastructure (COMPLETE)
1. ~~**Benchmark suite setup** - MUST be done before any optimizations~~ ✅
   - ~~Add `criterion` dev dependency~~ ✅
   - ~~Create benchmarks for existing hot paths (Vec3, Mat4, Quat, Transform)~~ ✅
   - ~~Establish baseline metrics~~ ✅
   - ~~Document benchmark running procedure~~ ✅
   - This ensures we can verify that optimizations actually improve performance

2. ~~**Update CLAUDE.md** - Correct SIMD stance~~ ✅
   - ~~Change "NO SIMD planned" to "SIMD planned but deferred"~~ ✅
   - ~~Document that benchmarks will guide SIMD implementation~~ ✅

**Status**: COMPLETED
- Benchmark suite created in `benches/` directory
- Criterion dependency added to Cargo.toml
- Benchmarks for Vec3, Mat4, Quat, and Transform created
- Documentation in `benches/README.md`
- All benchmarks compile and run successfully
- CLAUDE.md updated to reflect SIMD plans

**Run benchmarks with**:
```bash
cargo bench                    # Run all benchmarks
cargo bench --bench vec3_bench # Run specific benchmark
```

### Phase 1: Critical for Rendering (HIGH)
1. Vec4 completion - required for homogeneous coordinates
2. Mat4 improvements (transpose, matrix-vector traits) - needed for shaders
3. Mat3 - needed for normal matrix calculations
4. Plane - needed for frustum culling
5. Ray - needed for mouse picking
6. Frustum - needed for performance optimization
7. **Run benchmarks after each major addition** - track performance impact

### Phase 2: API Completeness (MEDIUM)
1. Vec3 additions (reflect, project, angle, etc.)
2. Vec2 additions (perpendicular, angle, etc.)
3. Quat improvements (matrix conversion, full Euler)
4. Transform improvements (inverse, LookAt)
5. Mat2 - useful for 2D operations
6. Rect2D - useful for UI/viewport
7. Math constants and utilities module

### Phase 3: Optimizations (MEDIUM-HIGH)
**IMPORTANT**: Only optimize after benchmarks prove the need

1. **Add `#[inline]` attributes** - After benchmarks show which functions benefit
2. **Mat4 determinant optimization** - Only if benchmarks show it's a bottleneck
3. **Algorithmic improvements** - Based on actual profiling data, not assumptions

### Phase 4: Enhanced Features (LOW)
1. Color additions (HSL, perceptual distance)
2. Angle type - prevents unit confusion
3. Swizzling operations - improves ergonomics
4. Property-based testing with `proptest`

### Phase 5: SIMD Implementation (FUTURE)
1. **Re-run benchmarks** to identify remaining bottlenecks
2. Add `simd` feature flag
3. Implement SIMD versions of critical paths identified by benchmarks
4. Verify SIMD implementations match scalar results
5. Benchmark SIMD vs scalar to prove improvement
6. Document platform-specific behavior and performance differences

### Phase 6: Documentation and Polish
1. Module and type documentation
2. Usage examples
3. Performance guide with benchmark results
4. SIMD feature documentation

---

## Testing Checklist

For each new type or method, ensure tests cover:

- [ ] Basic functionality (happy path)
- [ ] Edge cases (zero, identity, NaN, infinity)
- [ ] Mathematical properties (commutativity, associativity, etc.)
- [ ] Round-trip conversions (type A → type B → type A)
- [ ] Known values (compare with hand-calculated results)
- [ ] Boundary conditions (empty, singular, degenerate)
- [ ] Performance (if operation is O(n) or worse)

---

## Migration Guide

When adding these features, ensure:

1. **No breaking changes** to existing API
2. **Backward compatibility** - all existing code continues to work
3. **Deprecation warnings** for replacing free functions with trait methods (e.g., `mat4_mul_vec3` → `Mul<Vec3> for Mat4`)
4. **Feature flags** for experimental or optional features (if applicable)

---

## Notes

- All new types should follow existing patterns (tuple struct vs named fields)
- All arithmetic should use `f32` for consistency (document this decision)
- All methods should handle edge cases gracefully (document behavior)
- Prefer returning `Option<T>` for fallible operations (e.g., singular matrix inverse)
- Use `#[must_use]` attribute on methods that have side-effect-free return values
- **CRITICAL**: Set up benchmarks in Phase 0 before ANY optimization work
- SIMD will be implemented via feature flag after identifying bottlenecks through profiling
- All optimizations must be verified with benchmarks - no premature optimization
