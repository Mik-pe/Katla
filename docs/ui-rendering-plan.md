# UI Rendering Implementation Plan

## Overview

This document outlines the plan for implementing UI rendering in the Katla engine. The goal is to integrate `katla_ui` into the rendering pipeline using the existing render graph architecture.

## Current Status

- [x] `katla_ui` crate created with immediate mode UI widgets
- [x] `DrawList` generation working (vertices, indices, commands)
- [x] WGSL shaders written (`resources/shaders/ui/ui.wgsl`)
- [x] UI input handling wired up in Application
- [x] Debug overlay UI generating draw lists
- [x] Actual Vulkan rendering of UI draw lists
- [x] UI pass integrated into render graph
- [x] Fixed vertex layout (UiShaderVertex with tight 32-byte packing)
- [x] Window title bar support with proper layout
- [x] Persistent buffer management (no per-frame allocations)
- [ ] Font texture atlas support (text renders as placeholder boxes)
- [ ] White texture fallback for non-textured UI elements
- [ ] Clipping support for nested windows

## Architecture

### Dependency Chain

```
katla_app
    ├── katla_vulkan (render graph, pipelines, materials)
    ├── katla_ui (UI logic, draw list generation)
    ├── katla_ecs (entity system)
    └── katla_math (math types)

katla_ui MUST NOT depend on:
    - ash (Vulkan types should come from katla_vulkan wrappers)
    - katla_app
    - katla_ecs
```

### Rendering Approach

The UI integrates into the render graph as a dedicated pass:

```
[sky_pass] → [geometry_pass] → [ui_pass] → [present]
```

The UI pass:
1. Reads from the swapchain color attachment (no clear)
2. Uses alpha blending (src: SRC_ALPHA, dst: ONE_MINUS_SRC_ALPHA)
3. No depth test/write (UI is always on top)
4. Uses NDC coordinates (pre-transformed from screen space in application layer)

### Critical: Vertex Layout

**IMPORTANT**: `katla_math::Vec2` is 16 bytes (aligned for SIMD), but WGSL `vec2f` is 8 bytes.
The shader-compatible vertex struct `UiShaderVertex` uses `[f32; 2]` directly:

```rust
#[repr(C)]
pub struct UiShaderVertex {
    pub position: [f32; 2],  // 8 bytes (not Vec2!)
    pub uv: [f32; 2],        // 8 bytes
    pub color: [f32; 4],     // 16 bytes
}  // Total: 32 bytes, matching shader and vertex binding
```

## Implementation Steps

### Phase 1: UI Pipeline Setup ✅

1. **Create UiMaterial** (in `katla_app/src/rendering/ui_material.rs`)
   - [x] Create pipeline with alpha blending
   - [x] Configure vertex format (position[2], uv[2], color[4])
   - [x] UiShaderVertex struct with correct memory layout
   - [ ] Create white texture placeholder for font atlas
   - [ ] Create descriptor set for uniforms + texture

2. **UI Uniforms**
   - Currently using pre-transformed NDC coordinates (no uniform needed)
   - Future: may add screen_size uniform for shader-side transformation

### Phase 2: Render Graph Integration ✅

3. **Add UI Pass to Render Graph** (in `katla_vulkan/src/lib.rs`)
   - [x] Add `ui_data` field to VulkanRenderer for passing draw list
   - [x] Add `ui_pipeline` field to VulkanRenderer
   - [x] Create `ui_pass` after `geometry_pass`
   - [x] Load (not clear) color attachment from previous pass
   - [x] Bind UI pipeline and render

4. **Buffer Management** ✅
   - [x] Create UIBuffers struct with persistent vertex/index buffers
   - [x] One buffer set per frame in flight (avoids sync issues)
   - [x] 256KB vertex + 128KB index capacity per frame
   - [x] Update buffers each frame via memory mapping (no allocation)
   - [x] Fallback to temporary buffers if not initialized

### Phase 3: Texture Support (In Progress)

5. **Font Atlas Texture**
   - [x] FontSystem has CPU-side atlas (512x512 RGBA)
   - [x] Glyph rasterization and caching working
   - [x] Create GPU texture for font atlas (UITextures struct)
   - [x] Upload atlas data to GPU on initialization
   - [x] Update atlas texture when new glyphs added (update_font_atlas)
   - [x] Bind atlas to descriptor set in UI pass
   - [x] Wire up atlas updates in application render loop
   - [ ] Load a font file for text rendering (needs TTF/OTF file)

6. **White Texture Fallback**
   - [x] Create 1x1 white texture for non-textured UI elements
   - [x] Included in UITextures struct

7. **Shader Updates for Texturing**
   - [x] Add texture/sampler bindings to ui.wgsl
   - [x] Update fragment shader to sample from texture
   - [x] Multiply texture alpha with vertex color for text

### Phase 4: Widget Improvements

7. **Layout Fixes**
   - [ ] Fix text overflow in window backgrounds
   - [ ] Auto-size windows based on content
   - [ ] Proper clipping for nested elements

### Phase 5: Application Integration ✅

8. **Wire Up in Application**
   - [x] Create UiMaterial in Application
   - [x] Pass UI pipeline to VulkanRenderer
   - [x] Call `set_ui_data()` each frame with draw list
   - [x] Verify rendering works

## Technical Details

### Vertex Format

```rust
#[repr(C)]
struct UiShaderVertex {
    position: [f32; 2],  // NDC coordinates (-1 to 1)
    uv: [f32; 2],        // Texture coordinates
    color: [f32; 4],     // RGBA color
}
```

### Shader (Current - No Uniforms)

```wgsl
@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4f(in.position, 0.0, 1.0);  // Pre-transformed to NDC
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return in.color;  // Solid color, no texturing yet
}
```

### Render State

- Alpha blending: `src * src.a + dst * (1 - src.a)`
- No depth test
- No depth write
- No backface culling
- Counter-clockwise front face

### Buffer Strategy

Current approach (temporary staging buffers):
- Creates new staging buffers each frame
- Works but inefficient (allocations every frame)

Planned approach (persistent buffers):
- Create buffers large enough for max UI elements
- Map and update each frame
- No per-frame allocations

## Files Modified

### katla_vulkan
- `src/lib.rs` - UI pass in render graph, ui_data field, set_ui_data()

### katla_app
- `src/rendering/ui_material.rs` - UI pipeline creation, UiShaderVertex struct
- `src/rendering/mod.rs` - Export UiMaterial
- `src/application/mod.rs` - Wire up UI rendering, transform to NDC
- `src/ui/debug_overlay.rs` - Debug overlay component

### katla_ui
- `src/draw_list.rs` - Draw list with vertex/index data
- `src/context.rs` - UiContext for immediate mode UI
- `src/widgets/` - Window, label, button widgets

## Known Issues

1. **Text Overflow**: Window backgrounds don't account for all text lines properly
2. **Buffer Allocations**: Creating temporary staging buffers each frame (inefficient)
3. **No Texturing**: Font atlas not implemented, solid colors only

## Future Enhancements

- [ ] Font rendering with glyph atlas
- [ ] Text input widget with actual text
- [ ] Clipping support for nested windows
- [ ] Scroll areas
- [ ] Theme system
- [ ] Custom widget styling
- [ ] Persistent buffer management for better performance
