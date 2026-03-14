# Multi-Viewport Architecture

Multi-viewport frame graph implementation architecture and design decisions.

**What belongs here:** Architectural decisions, design patterns, implementation approach for multi-viewport support.  
**What does NOT belong here:** Service ports/commands (use `.factory/services.yaml`).

---

## Architectural Decision: Dedicated Descriptor Set

### Problem
The original documentation proposed using "bindless texture arrays" for the compositing pass. This combines two different solutions that each solve the "many textures" problem:
- Bindless descriptors: Single descriptor set with 4096 texture slots
- Texture arrays: Single binding with array of textures

Using both together solves the same problem twice (over-engineering).

### Solution
Use a **dedicated descriptor set** for the compositing pass:
- **Set 2, Binding 0**: Fixed array of 8 sampler2D bindings
- Simple, explicit, easy to debug
- No bindless complexity for this isolated use case
- Direct array indexing in shader: `viewportTextures[index]`

### Benefits
- **Simpler**: No need to extend bindless system
- **Explicit**: Easy to see what textures compositing uses
- **Easier to debug**: Fixed layout, no dynamic indexing confusion
- **Same performance**: For 8 textures, no performance difference
- **Clear separation**: Bindless (set 1) for dynamic textures, fixed set (set 2) for static

---

## Frame Graph Integration

### Viewport Pass Pattern
Viewports render to transient textures, not directly to backbuffer:

```rust
// Create viewport texture resources
graph.create_resource(GraphResourceDesc {
    name: "viewport_0".to_string(),
    width: 960,
    height: 1080,
    format: ImageFormat::R16G16B16A16Sfloat,
    resource_type: GraphResourceType::ColorAttachment { ... },
});

// Viewport pass writes to transient texture
graph.add_pass(GeometryPass::new("viewport_0_pass")
    .write_color("viewport_0", ImageFormat::R16G16B16A16Sfloat));

// Compositing pass reads viewport textures and writes to backbuffer
graph.add_pass(CompositePass::new("composite")
    .viewport("viewport_0", ViewportRect::new(0.0, 0.0, 960.0, 1080.0))
    .write("backbuffer"));
```

### Resource Naming Conventions
- Viewport textures: `"viewport_0"`, `"viewport_1"`, `"viewport_2"`, etc.
- Viewport passes: `"viewport_0_pass"`, `"viewport_1_pass"`, etc.
- Compositing pass: `"composite"`
- Output: `"backbuffer"` (special-cased in frame graph)

### Barrier Management
Frame graph handles barriers automatically:
- Pre-pass barriers transition resources for the current pass
- Post-pass barriers transition written resources for subsequent readers
- Layout tracking via `RefCell` in `TransientTexture`

No manual barrier management needed for viewport rendering.

---

## Compositing Pass Design

### Viewport Rectangles
```rust
pub struct ViewportRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

// Converts to [x, y, x+width, y+height] for shader
impl ViewportRect {
    pub fn to_array(&self) -> [f32; 4] {
        [self.x, self.y, self.x + self.width, self.y + self.height]
    }
}
```

### Shader Layout
```wgsl
struct ViewportRect {
    x: f32, y: f32, z: f32, w: f32, // x, y, x+w, y+h
}

struct CompositingParams {
    rects: array<ViewportRect, 8>,
    viewport_count: u32,
    screen_size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> params: CompositingParams;
@group(2) @binding(0) var viewportTextures: array<texture_2d<f32>, 8>;
@group(2) @binding(1) var sampler0: sampler;
```

### Alpha Blending Strategy
Compositing shader iterates viewports in **reverse order** (topmost last):
```wgsl
for (var i: i32 = i32(params.viewport_count) - 1; i >= 0; i--) {
    let rect = params.rects[u];
    if (pixel_in_rect) {
        let color = textureSample(viewportTextures[i], sampler0, local_uv);
        if (color.a >= 0.95) {
            return color; // Opaque, overwrite and exit
        }
        result = mix(result, color, color.a); // Blend
    }
}
```

---

## Cleanup Strategy

### Aggressive Removal
When new frame graph approach works, delete old code immediately:
- Delete `render_viewport()` methods
- Remove rendering state from Viewport struct
- Delete direct viewport rendering functions
- No `#[allow(dead_code)]` attributes
- No compatibility shims

### What to Keep
- **ViewportBuilder** API (public interface unchanged)
- **ViewportManager** lifecycle (creation, lookup, destruction)
- **Viewport** configuration (label, extent, clear_color, output_mode)
- Only rendering state moves to frame graph

### Migration Pattern
```rust
// Before (old approach - to be deleted)
renderer.render_viewport(viewport, &camera, &draw_list);

// After (new approach - use frame graph)
graph.add_pass(GeometryPass::new("viewport_pass")
    .write_color("viewport_0", format)
    .material(material));

graph.execute(&renderer, |frame| {
    frame.submit("viewport_pass", &draw_list);
});
```

---

## Validation Approach

### Testing Surfaces
- **Unit tests**: API correctness, error handling
- **Integration tests**: Frame graph compilation, multi-viewport scenarios
- **Visual verification**: `cargo run -- -s` (25 frames)
- **Vulkan validation**: `cargo run -- -v -- -s` (validation layers)

### Evidence Collection
- **Test output**: `cargo test --workspace` logs
- **Shader compilation**: Build output shows WGSL → SPIR-V success
- **Vulkan validation**: Validation layer output
- **Visual**: Screenshots for regression testing
- **Performance**: Frame time comparison

---

## Performance Considerations

### Expected Performance
- Multi-viewport rendering: Within 10% of single viewport baseline
- Compositing pass: Minimal overhead (single fullscreen draw)
- No texture copying: All GPU-resident, zero copy

### Optimization Opportunities
(Future work, not in this mission)
- Resource aliasing for non-overlapping viewports
- Async compute for parallel viewport rendering
- Per-viewport post-processing effects

---

## Key Files

### Implementation Files
- `katla_gfx/src/render_graph/descriptor_sets/compositing.rs` - CompositingDescriptorSet
- `katla_gfx/src/render_graph/passes/composite.rs` - CompositePass template
- `katla_gfx/src/viewport.rs` - Viewport configuration (simplified after cleanup)
- `katla_gfx/src/renderer/viewport_manager.rs` - Viewport lifecycle management

### Shader Files
- `resources/shaders/composite.wgsl` - Compositing shader

### Test Files
- `katla_gfx/tests/compositing_test.rs` - Compositing descriptor set tests
- `katla_gfx/tests/multi_viewport_test.rs` - Integration tests

---

## References

- **Implementation Plan**: `docs/feature-multi-viewport/multi-viewport-implementation-plan.md`
- **Best Practices**: `docs/feature-multi-viewport/vulkan-framegraph-ui-best-practices-2024-2026.md`
- **Current Analysis**: `docs/feature-multi-viewport/katla-rendergraph-analysis.md`
