# Editor UI Frame Graph Integration Plan

## Goal
Integrate the UI pass into the frame graph so that the 3D viewport is rendered as part of the editor UI, not as a fullscreen pass.

## Architecture

```
┌─────────────────┐
│   Sky Pass      │ ──→ hdr_color (R16G16B16A16_SFLOAT)
└────────┬────────┘
         │ write
         ▼
┌─────────────────┐
│ Geometry Pass   │ ──→ hdr_color (LoadOp::Load)
└────────┬────────┘
         │ write
         ▼
┌─────────────────┐
│  Tonemap Pass   │ ←── reads hdr_color
└────────┬────────┘ ──→ tonemapped_viewport (B8G8R8A8_SRGB)
         │ write
         ▼
┌─────────────────┐
│    UI Pass      │ ←── reads tonemapped_viewport (as texture)
└────────┬────────┘ ──→ backbuffer
         │ write
         ▼
    [Swapchain]
```

## Implementation Steps

### Step 1: Add tonemapped_viewport transient texture
- Create a new transient texture for the LDR output
- Format: B8G8R8A8_SRGB (standard swapchain format)
- Size: same as swapchain

### Step 2: Modify tonemap pass to write to texture
- Change tonemap pass from `.write_backbuffer()` to `.write("tonemapped_viewport", ...)`
- Register tonemapped_viewport with bindless for UI sampling

### Step 3: Add UI pass to frame graph
- UIPass reads "tonemapped_viewport" as a texture
- UIPass writes to "backbuffer"
- UI draws the viewport texture in the viewport region + all editor UI

### Step 4: Register textures for bindless sampling
- Register "tonemapped_viewport" with bindless system
- Pass the texture index to the UI system so it can sample the viewport

### Step 5: Update UI rendering
- Move UI rendering from post-frame-graph to inside the UI pass
- UI draws viewport texture as a textured quad at the viewport bounds
- UI draws all editor panels on top

## Key Files to Modify

1. **katla_app/src/application/builder.rs**
   - Add `tonemapped_viewport` transient texture
   - Change tonemap pass to write to texture
   - Add UIPass to frame graph

2. **katla_app/src/application/mod.rs**
   - Register tonemapped_viewport with bindless
   - Store texture index for UI system

3. **katla_app/src/application/renderer.rs**
   - Submit UI draw list to the UI pass via frame.submit_ui()

4. **katla_gfx/src/render_graph/graph.rs**
   - Ensure execute_graphics_pass handles UI draw lists
   - Handle backbuffer transition for UI pass

## Risks & Mitigations

1. **Backbuffer layout transitions** - Need to ensure proper barriers between tonemap and UI pass
2. **UI texture sampling** - UI shader needs to sample the tonemapped viewport texture
3. **Viewport bounds** - UI needs to know where to draw the viewport texture

## Success Criteria

- 3D scene renders in a viewport panel within the editor UI
- Editor panels (hierarchy, inspector, etc.) are visible around the viewport
- No gray screen or missing textures
