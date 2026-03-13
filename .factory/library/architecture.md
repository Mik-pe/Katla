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
