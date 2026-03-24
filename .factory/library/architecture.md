# Architecture

Architectural decisions, patterns, and conventions for the Katla engine.

**What belongs here:** Architectural decisions, module organization, coding patterns.

## Workspace Structure

- `katla_math` - Math library (vectors, matrices, quaternions) - NO dependencies on other crates
- `katla_gfx` - Graphics API layer - NO dependencies on katla_math, katla_ecs, katla_app
- `katla_ecs` - Entity Component System - NO dependencies on other katla crates
- `katla_ui` - Immediate mode UI - CAN depend on katla_math, katla_gfx
- `katla_app` - Application framework - can depend on all other crates
- `katla_derive` - Proc macros for ECS

## Dependency Rules (CRITICAL)

```
katla_math  ← (nothing)
katla_gfx   ← (nothing)
katla_ecs   ← (nothing) — NOTE: previously violated by katla_math dep, fixed in cleanup mission
katla_ui    ← katla_math, katla_gfx
katla_app   ← katla_math, katla_gfx, katla_ecs, katla_ui
```

## Workspace Dependencies

All shared dependencies are centralized in root `Cargo.toml` under `[workspace.dependencies]`. Crates reference them with `{ workspace = true }`. No wildcard `"*"` versions allowed.

## katla_math Specifics

### SSE Backend Coverage

Only `Vec4`, `Quat`, and `Mat4` have SSE backends (`katla_math/src/sse/`). `Vec2` and `Vec3` are scalar-only. API standardization work only needs dual-backend changes for `Vec4`/`Quat`/`Mat4`.

### SSE Vec4 Const Limitation

`Vec4` on x86_64 uses SSE intrinsics (`__m128`) which cannot be used in `const` contexts. Therefore `Vec4::ZERO`/`ONE` etc exist only as const associated constants on the scalar implementation, while the SSE implementation provides `fn zero()`/`fn one()` methods. Tests targeting `Vec4` must use method calls since x86_64 uses the SSE path.

### AABB vs Sphere create_from_verts Input Types

`AABB::create_from_verts` accepts `&[Vec3]` while `Sphere::create_from_verts` accepts `&[f32; 3]`. This API inconsistency limits shared helper dedup without allocation (Sphere currently heap-allocates a `Vec<Vec3>` to call the shared `compute_bounds` helper).

## ECS Component Trait

The `Component` trait in `katla_ecs` is a marker trait bounded by `Any`:
```rust
pub trait Component: Any {}
```
The `#[derive(Component)]` macro generates an empty `impl<T> Component for T {}`. The derive macro's only value is ergonomic `#[derive(Component)]` syntax — the trait bound `T: Any` is what enables type-erased downcasting via `AnyComponentStorage`. The `as_any`/`as_any_mut` methods live on `AnyComponentStorage`, not on `Component`.

## Code Conventions

- Visibility: `pub(crate)` by default, `pub` only when necessary
- No backwards compatibility - remove old code when replacing
- No hybrid implementations - single way to do things
- Error handling: Use `Option<T>`, `Result<T, E>`, avoid `unwrap()` in production

## UI Architecture

- Immediate mode UI pattern
- `UiContext::begin()` → widget calls → `UiContext::end()` returns `DrawList`
- `DrawList` converted to `UIDrawList` for GPU rendering
- Font atlas with white pixel at (0,0) for solid color rendering

## Graphics Rendering Concepts

### Texture Color Space Formats

**SRGB vs UNORM Texture Formats:**
- **SRGB format** (`rgba8_srgb`): Texture data is in sRGB color space. GPU automatically converts to linear space during sampling. Required for UI/font textures that contain color data meant for display.
- **UNORM format** (`rgba8_unorm`): Texture data is in linear color space. No automatic conversion during sampling. Generally used for non-color data (normal maps, roughness maps, etc.).

**Critical for rendering correctness:** When a texture contains display colors (like font atlases), it must be created with SRGB format. If created with UNORM format, the color values will be interpreted incorrectly:

- White pixel [255,255,255,255] in SRGB space → samples as pure white (1.0, 1.0, 1.0, 1.0)
- Same pixel in UNORM (linear) space → interpreted as linear white, which appears semi-transparent when blended with sRGB render targets

**Code example:** Font atlas creation
```rust
// CORRECT - Font atlas with color data
let font_atlas = device.create_texture(TextureDescriptor {
    format: TextureFormat::rgba8_srgb(),  // sRGB for color data
    // ...
});

// WRONG - Would cause transparency issues
let font_atlas = device.create_texture(TextureDescriptor {
    format: TextureFormat::rgba8_unorm(),  // Linear space - wrong for fonts
    // ...
});
```

### Shader Texture Modulation Pattern

**Solid color rendering via white pixel sampling:**

UI shaders use a common pattern for efficient solid color rendering:
```
output_color = vertex_color * texture_sample(texture, uv)
```

**How it works:**
1. Solid color quads set UV coordinates to (0, 0)
2. Font atlas has a white pixel [255,255,255,255] at UV (0, 0)
3. Shader samples white pixel: (1.0, 1.0, 1.0, 1.0)
4. Shader multiplies by vertex color: `vertex_color * (1.0, 1.0, 1.0, 1.0) = vertex_color`
5. Result: Efficient solid color rendering without special cases

**Why this matters:** The white pixel must be in the correct color space (SRGB) for the multiplication to work correctly. If the texture format is UNORM, the white pixel samples as linear white which renders semi-transparent.

## Render Graph Synchronization

### Pass Dependencies

Every render pass must declare its resource dependencies explicitly:
- `.read("resource_name")` - Pass samples from this texture (requires `SHADER_READ_ONLY_OPTIMAL`)
- `.write("resource_name")` - Pass writes to this texture/color attachment

**Why this matters:** The render graph uses these declarations to automatically insert Vulkan pipeline barriers with correct stage and access masks. Missing dependencies can cause:
- Race conditions between passes
- Visual flickering when framerate varies
- Vulkan validation errors

### Example: UI Pass Sampling Tonemapped Scene

```rust
// CORRECT - Declares read dependency
.add_pass(UIPass::new("ui")
    .read("ldr_color")       // UI samples tonemapped scene
    .write("backbuffer")     // UI writes to swapchain
    .material(ui_material))

// WRONG - Missing read dependency causes sync issues
.add_pass(UIPass::new("ui")
    .write("backbuffer")     // No read declared - barrier not inserted!
    .material(ui_material))
```

When a pass samples a transient texture via bindless, the read dependency MUST be declared so the render graph inserts the correct barrier:
- `srcStage = COLOR_ATTACHMENT_OUTPUT` (previous pass writes)
- `dstStage = FRAGMENT_SHADER` (this pass samples)
- `srcAccess = COLOR_ATTACHMENT_WRITE`
- `dstAccess = SHADER_READ`
