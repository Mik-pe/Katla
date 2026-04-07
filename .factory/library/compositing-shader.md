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

#### ViewportRect (stored in objects[i].base_color)

Viewport rectangles are packed as `vec4f(x, y, x+width, y+height)` in the `objects[]` storage buffer (Set 0, Binding 1). Each active viewport `i` has its rectangle at `objects[i].base_color`.

#### Compositing Parameters (stored in frame_data.compositing)

Screen size and viewport count are read from `frame_data.compositing` (a `vec4f` field in `FrameUniforms`):
- `frame_data.compositing.xy`: Screen size (width, height)
- `frame_data.compositing.z`: Viewport count (as f32, cast to u32 in shader)

## Implementation Notes

### Current Implementation

The shader uses the existing `objects[]` storage buffer (Set 0, Binding 1) to pass per-viewport rectangles, and `frame_data.compositing` for screen size and viewport count. This avoids creating a dedicated compositing uniform buffer and changing the descriptor set layout.

Each viewport `i` (0..viewport_count) has its rectangle stored at `objects[i].base_color` as `[x, y, x+width, y+height]`. The compositing pass writes identity model matrices + rect data into `objects[i]` via `update_object_bindless()` before the compositing render pass executes.

The compositing pass runs last in the frame graph, so the `objects[]` array is available (its data from the geometry pass is no longer needed).

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
// frame_data.compositing = vec4f(1920.0, 1080.0, 2.0, 0.0)  // screen_size + count

// Viewport 0: Left half
objects[0].base_color = vec4f(0.0, 0.0, 960.0, 1080.0);

// Viewport 1: Right half
objects[1].base_color = vec4f(960.0, 0.0, 1920.0, 1080.0);
```

### 2x2 Grid Layout (4 Viewports)

```rust
// frame_data.compositing = vec4f(1920.0, 1080.0, 4.0, 0.0)

objects[0].base_color = vec4f(0.0, 0.0, 960.0, 540.0);      // Top-left
objects[1].base_color = vec4f(960.0, 0.0, 1920.0, 540.0);    // Top-right
objects[2].base_color = vec4f(0.0, 540.0, 960.0, 1080.0);    // Bottom-left
objects[3].base_color = vec4f(960.0, 540.0, 1920.0, 1080.0); // Bottom-right
```

### Picture-in-Picture Layout

```rust
// frame_data.compositing = vec4f(1920.0, 1080.0, 2.0, 0.0)

objects[0].base_color = vec4f(0.0, 0.0, 1920.0, 1080.0);      // Fullscreen background
objects[1].base_color = vec4f(1600.0, 800.0, 1900.0, 1050.0); // PiP overlay (semi-transparent)
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

1. **Parameter Passing via objects[] Buffer**: Viewport rectangles are passed through the objects[] storage buffer (Set 0, Binding 1) rather than a dedicated compositing uniform buffer. This works because compositing runs last in the frame graph when the objects[] data from the geometry pass is no longer needed.

## Descriptor Set Layout Ordering Constraint

The compositing descriptor set layout must be placed at **set 2** (not appended after light_culling/shadow). This is because skeleton, compositing, and the empty placeholder descriptor sets are mutually exclusive at set 2, which keeps light culling consistently at set 3 for pipeline layout index stability.

This constraint is enforced in `katla_gfx/src/vulkan/material/compiler.rs`. When adding new descriptor sets to the pipeline layout, do NOT append them after light_culling/shadow — they must go at set 2 if they are mutually exclusive with skeleton/compositing.

**Discovered During:** gfx-bugfixes feature (milestone: gfx-polish)

## References

- **CompositingDescriptorSet**: `katla_gfx/src/render_graph/descriptor_sets/compositing.rs`
- **CompositePass Template**: `katla_gfx/src/render_graph/passes/composite.rs` (future work)
- **Multi-Viewport Architecture**: `.factory/library/multi-viewport-architecture.md`
- **WGSL Spec**: https://www.w3.org/TR/WGSL/
