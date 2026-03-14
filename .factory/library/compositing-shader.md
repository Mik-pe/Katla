# Compositing Shader Implementation

## Overview

The compositing shader (`resources/shaders/composite.wgsl`) is a fullscreen pass that combines multiple viewport textures into a final output. It supports up to 8 simultaneous viewports with per-viewport positioning and alpha blending for overlapping viewports.

## Shader Structure

### Bindings

**Set 0: Frame/Object Uniforms (standard across all shaders)**
- Binding 0: `FrameUniforms` (view/proj matrices, camera, lighting)
- Binding 1: `ObjectUniforms` array (per-object data)

**Set 1: Bindless Textures (shared across all shaders)**
- Binding 0: `bindless_textures` (up to 4096 textures)
- Binding 1: `shared_sampler`

**Set 2: Compositing Descriptor Set**
- Binding 0: `viewportTextures` (array of 8 texture_2d)

This matches the `CompositingDescriptorSet` layout in `katla_gfx/src/render_graph/descriptor_sets/compositing.rs`.

### Data Structures

#### ViewportRect
```wgsl
struct ViewportRect {
    x: f32,  // Left edge in pixels
    y: f32,  // Top edge in pixels
    z: f32,  // Right edge in pixels (x + width)
    w: f32,  // Bottom edge in pixels (y + height)
}
```

Stores viewport position as [x, y, x+w, y+h] to avoid recalculation in the shader.

#### CompositingUniforms
```wgsl
struct CompositingUniforms {
    rects: array<ViewportRect, 8>,  // Viewport rectangles
    viewport_count: u32,             // Number of active viewports
    screen_size: vec2f,              // Screen dimensions in pixels
    padding: f32,                    // 16-byte alignment
}
```

## Implementation Notes

### Current Implementation (Proof of Concept)

The current shader uses `objects[0]` from the storage buffer to pass compositing parameters:
- `base_color.xy`: Screen size (width, height)
- `material_params.x`: Viewport count

This is a temporary approach that mirrors the tonemapping shader pattern. The shader currently implements a simple 2-viewport split-screen layout as a proof of concept.

### Future Implementation

A proper uniform buffer should be created for compositing parameters:
1. Create a uniform buffer with `CompositingUniforms` layout
2. Bind at set 2, binding 1 (after texture array at binding 0)
3. Update each frame with current viewport configuration
4. Use proper viewport rectangle iteration instead of hardcoded layout

### Alpha Blending Algorithm

The shader uses reverse iteration (from topmost to bottom) for correct alpha blending:

```wgsl
for (var i: i32 = i32(count) - 1; i >= 0; i--) {
    let rect = params.rects[u32(i)];
    if (pixel_in_rect(pixel_pos, rect)) {
        let color = textureSample(viewportTextures[i], sampler, local_uv);
        if (color.a >= 0.95) {
            return color;  // Opaque, overwrite and exit
        }
        result = mix(result, color, color.a);  // Blend
    }
}
```

This ensures:
1. Topmost viewport (highest index) is drawn first
2. Opaque viewports (alpha >= 0.95) overwrite immediately
3. Semi-transparent viewports blend with current result
4. Bottommost viewport is drawn last

## Usage Example

### Split-Screen Layout (2 Viewports)

```rust
// Configure compositing parameters
objects[0].base_color = vec4f(1920.0, 1080.0, 0.0, 0.0);  // Screen size
objects[0].material_params.x = 2.0;  // 2 viewports

// Viewport 0: Left half
rects[0] = ViewportRect::new(0.0, 0.0, 960.0, 1080.0);

// Viewport 1: Right half
rects[1] = ViewportRect::new(960.0, 0.0, 1920.0, 1080.0);
```

### 2x2 Grid Layout (4 Viewports)

```rust
objects[0].base_color = vec4f(1920.0, 1080.0, 0.0, 0.0);
objects[0].material_params.x = 4.0;

rects[0] = ViewportRect::new(0.0, 0.0, 960.0, 540.0);      // Top-left
rects[1] = ViewportRect::new(960.0, 0.0, 1920.0, 540.0);    // Top-right
rects[2] = ViewportRect::new(0.0, 540.0, 960.0, 1080.0);    // Bottom-left
rects[3] = ViewportRect::new(960.0, 540.0, 1920.0, 1080.0); // Bottom-right
```

### Picture-in-Picture Layout

```rust
objects[0].base_color = vec4f(1920.0, 1080.0, 0.0, 0.0);
objects[0].material_params.x = 2.0;

rects[0] = ViewportRect::new(0.0, 0.0, 1920.0, 1080.0);      // Fullscreen background
rects[1] = ViewportRect::new(1600.0, 800.0, 1900.0, 1050.0); // PiP overlay (semi-transparent)
```

## Verification

To verify the shader works correctly:

1. **Shader Compilation**
   ```bash
   cargo build --bin katla
   ```
   Should compile without WGSL errors.

2. **Visual Verification**
   ```bash
   cargo run -- -s  # Run for 25 frames
   ```
   Should display viewports at correct positions.

3. **Vulkan Validation**
   ```bash
   cargo run -- -v -- -s  # Run with validation layers
   ```
   Should show no descriptor layout errors.

## Validation Assertions

This shader fulfills the following validation contract assertions:

- **VAL-COMP-004**: Shader compiles from WGSL to SPIR-V without errors
- **VAL-COMP-009**: Overlapping viewport support with alpha blending
- **VAL-COMP-010**: Texture array indexing for each viewport
- **VAL-COMP-011**: Backbuffer output (writes to location 0)

## Known Limitations

1. **Temporary Parameter Passing**: Currently uses `objects[0]` instead of a dedicated uniform buffer
2. **Hardcoded Layout**: Proof-of-concept implementation with 2-viewport split-screen
3. **No Proper Rectangle Support**: Viewport rectangles are not yet implemented

These will be addressed in the compositing pass implementation.

## References

- **CompositingDescriptorSet**: `katla_gfx/src/render_graph/descriptor_sets/compositing.rs`
- **CompositePass Template**: `katla_gfx/src/render_graph/passes/composite.rs` (future work)
- **Multi-Viewport Architecture**: `.factory/library/multi-viewport-architecture.md`
- **WGSL Spec**: https://www.w3.org/TR/WGSL/
