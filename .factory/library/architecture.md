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
katla_ecs   ← (nothing)
katla_ui    ← katla_math, katla_gfx
katla_app   ← katla_math, katla_gfx, katla_ecs, katla_ui
```

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
