# Fullscreen Shader Patterns

This document captures patterns for fullscreen rendering shaders in Katla.

## Fullscreen Triangle Pattern

The standard pattern for fullscreen rendering uses a single triangle covering the entire viewport:

```wgsl
// Vertex shader
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> FullscreenVSOutput {
    // Triangle coordinates: (-1,-1), (3,-1), (-1,3)
    // This covers the entire viewport in clip space
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out = FullscreenVSOutput();
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.tex_coord = positions[vertex_index] * 0.5 + 0.5;  // Convert to [0,1]
    return out;
}
```

**Why this works:**
- First vertex (-1, -1): bottom-left corner
- Second vertex (3, -1): extends far right (covers 0 to 1 in UV x)
- Third vertex (-1, 3): extends far top (covers 0 to 1 in UV y)
- GPU rasterization fills the entire viewport

## Parameter Passing Pattern

For fullscreen passes that need parameters (e.g., tonemapping, compositing), use `frame_data` fields in `FrameUniforms`:

```wgsl
// Post-processing params stored in frame uniform buffer
@group(0) @binding(0) var<uniform> frame_data: FrameUniforms;

// Example: tonemapping reads frame_data.tonemap
// Example: wallhack overlay reads frame_data.overlay
```

**Usage in Katla:**
- Post-processing parameters (tonemap, overlay) are stored in `frame_data.tonemap` / `frame_data.overlay` fields
- Per-viewport compositing rectangles use the `objects[]` storage buffer
- Parameters are uploaded via the frame uniform buffer

**Historical note:** Prior to the polish milestone, tonemapping and overlay parameters were passed via `objects[0].base_color` abuse. This was refactored to use proper `frame_data` fields.

## Pass Configuration

Fullscreen passes should set `uses_depth: false`:

```rust
PassDescriptor {
    name: "compositing",
    uses_depth: false,  // Fullscreen passes don't need depth
    // ... other fields
}
```

**Why:** Fullscreen rendering is a 2D post-process step without depth testing.

## Examples in Katla

- `resources/shaders/tonemapping.wgsl` - HDR to LDR tonemapping
- `resources/shaders/ui.wgsl` - UI rendering
- `resources/shaders/composite.wgsl` - Multi-viewport compositing

**Discovered During:** compositing-shader and composite-pass-template features (milestone: compositing-infrastructure)
