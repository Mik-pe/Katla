# Viewport Conventions

This document captures viewport-related conventions and patterns in Katla.

## Viewport Rectangle Format

Viewport rectangles in shaders use the **min/max representation**:

```wgsl
struct ViewportRect {
    x: f32,  // min_x (left edge)
    y: f32,  // min_y (bottom edge)
    z: f32,  // max_x (right edge)
    w: f32,  // max_y (top edge)
}
```

**Shader array format:** `[x, y, z, w]` = `[min_x, min_y, max_x, max_y]`

### Conversion Helper

```rust
impl ViewportRect {
    pub fn to_array(&self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }
}
```

### Usage Example

```rust
// Create split-screen layout (left viewport)
let left_viewport = ViewportRect::from_origin_size(0.0, 0.0, 0.5, 1.0);
// Produces: [0.0, 0.0, 0.5, 1.0] = [min_x, min_y, max_x, max_y]

// Create 2x2 grid (top-left quadrant)
let top_left = ViewportRect::from_origin_size(0.0, 0.5, 0.5, 0.5);
// Produces: [0.0, 0.5, 0.5, 1.0]
```

## Backbuffer Format

The backbuffer (swapchain image) uses **sRGB format**:

```rust
// Backbuffer output format
const BACKBUFFER_FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;

// Material compilation for backbuffer rendering
let material = MaterialBuilder::new("compositing")
    .output_format(B8G8R8A8Srgb)  // Backbuffer is sRGB
    .build()?;
```

**Why sRGB:** The final output to display must be in sRGB color space for correct color representation.

## Named Resource Convention

Use `BACKBUFFER_NAME` constant instead of hardcoding "backbuffer":

```rust
// Correct
let pass = CompositePass::new("composite")
    .write_backbuffer()  // Uses BACKBUFFER_NAME internally
    .build();

// Incorrect (don't do this)
let pass = CompositePass::new("composite")
    .write("backbuffer")  // Hardcoded string
    .build();
```

**Benefits:**
- Single source of truth
- Easier refactoring
- Compile-time checking
- Consistent across codebase

**Discovered During:** composite-pass-template feature (milestone: compositing-infrastructure)
