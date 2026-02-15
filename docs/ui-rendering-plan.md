# UI Rendering Implementation Plan

## Overview

This document outlines the plan for implementing UI rendering in the Katla engine. The goal is to integrate `katla_ui` into the rendering pipeline using the existing render graph architecture.

## Current Status

- [x] `katla_ui` crate created with immediate mode UI widgets
- [x] `DrawList` generation working (vertices, indices, commands)
- [x] WGSL shaders written (`resources/shaders/ui/ui.wgsl`)
- [x] UI input handling wired up in Application
- [x] Debug overlay UI generating draw lists
- [ ] Actual Vulkan rendering of UI draw lists

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

The UI should integrate into the render graph as a dedicated pass:

```
[sky_pass] → [geometry_pass] → [ui_pass] → [present]
```

The UI pass:
1. Reads from the swapchain color attachment (no clear)
2. Uses alpha blending (src: SRC_ALPHA, dst: ONE_MINUS_SRC_ALPHA)
3. No depth test/write (UI is always on top)
4. Uses screen-space coordinates

## Implementation Steps

### Phase 1: UI Pipeline Setup

1. **Create UiMaterial** (in `katla_app/src/rendering/ui_material.rs`)
   - [x] Create pipeline with alpha blending
   - [x] Configure vertex format (position[2], uv[2], color[4])
   - [ ] Create white texture placeholder for font atlas
   - [ ] Create descriptor set for uniforms + texture

2. **UI Uniforms**
   - Uniform buffer with `screen_size: vec2`
   - Updated each frame before rendering

### Phase 2: Render Graph Integration

3. **Add UI Pass to Render Graph** (in `katla_vulkan/src/lib.rs`)
   - [ ] Add `ui_data` field to VulkanRenderer for passing draw list
   - [ ] Add `ui_pipeline` field to VulkanRenderer
   - [ ] Create `ui_pass` after `geometry_pass`
   - [ ] Load (not clear) color attachment from previous pass
   - [ ] Bind UI pipeline and render

4. **Buffer Management**
   - [ ] Create dynamic vertex buffer for UI vertices
   - [ ] Create dynamic index buffer for UI indices
   - [ ] Update buffers each frame with draw list data

### Phase 3: Texture Support

5. **Font Atlas Texture**
   - [ ] Create texture atlas in UiRenderer
   - [ ] Update atlas when new glyphs are rasterized
   - [ ] Bind atlas to descriptor set

6. **White Texture Fallback**
   - [ ] Create 1x1 white texture for non-textured UI elements
   - [ ] Use as default when no font is loaded

### Phase 4: Application Integration

7. **Wire Up in Application**
   - [ ] Create UiMaterial in Application
   - [ ] Pass UI pipeline to VulkanRenderer
   - [ ] Call `set_ui_data()` each frame with draw list
   - [ ] Verify rendering works

## Technical Details

### Vertex Format

```rust
#[repr(C)]
struct UiVertex {
    position: [f32; 2],  // Screen space pixels
    uv: [f32; 2],        // Texture coordinates
    color: [f32; 4],     // RGBA color
}
```

### Shader Bindings

```wgsl
@group(0) @binding(0) var<uniform> screen_size: vec2f;
@group(0) @binding(1) var font_atlas: texture_2d<f32>;
@group(0) @binding(2) var font_sampler: sampler;
```

### Render State

- Alpha blending: `src * src.a + dst * (1 - src.a)`
- No depth test
- No depth write
- No backface culling
- Clockwise front face (for 2D)

### Buffer Strategy

Two approaches:

1. **Dynamic Buffers (Simple)**
   - Create buffers large enough for max UI elements
   - Map and update each frame
   - Simple but may cause synchronization stalls

2. **Ring Buffer (Optimized)**
   - Multiple buffers in flight (one per frame)
   - No synchronization needed
   - More complex but better performance

Start with approach 1, optimize to 2 if needed.

## Files to Modify

### katla_vulkan
- `src/lib.rs` - Add UI pass to render graph, ui_data field
- `src/render_graph/compiled.rs` - May need pass ordering tweaks

### katla_app
- `src/rendering/ui_material.rs` - UI pipeline creation (NEW)
- `src/rendering/mod.rs` - Export UiMaterial
- `src/application/mod.rs` - Wire up UI rendering
- `src/ui/debug_overlay.rs` - Debug overlay component

### katla_ui
- `src/renderer/mod.rs` - Skeleton renderer (no ash dependency)
- `src/draw_list.rs` - Draw list with vertex/index data

## Questions/Decisions

1. **Should UI rendering happen in VulkanRenderer or separately?**
   - Decision: In VulkanRenderer as a render graph pass
   - Rationale: Integrates with existing pipeline, proper ordering

2. **How to handle the ash dependency boundary?**
   - katla_ui should NOT depend on ash
   - Vulkan types come from katla_vulkan wrappers
   - Actual rendering happens in katla_vulkan/katla_app

3. **Buffer updates: staging vs. direct mapping?**
   - Start with direct mapping (simpler)
   - May need staging for performance later

## Testing

1. Verify UI draw list is generated correctly (log vertex/index counts)
2. Verify pipeline is created with correct blend state
3. Verify UI renders on screen
4. Verify alpha blending works correctly
5. Verify screen size uniform is updated on resize

## Future Enhancements

- [ ] Font rendering with glyph atlas
- [ ] Text input widget with actual text
- [ ] Clipping support for nested windows
- [ ] Scroll areas
- [ ] Theme system
- [ ] Custom widget styling
