# Render Graph API Design - Developer Experience Perspective

> **Note**: All types and APIs described in this document are public exports from `katla_gfx`. This document focuses on the developer experience perspective - ease of use, discoverability, and sensible defaults for users of the library (including `katla_app`).

## Design Philosophy

As an app-maintainer reviewing graphics APIs from 10+ years of shipping games, my priorities are:

1. **Developer Velocity** - If it takes more than 30 seconds of thought to write, it's too complex
2. **Sensible Defaults** - Common cases should be trivial, details optional
3. **Composability** - Passes snap together like LEGO, no special glue needed
4. **Discoverability** - Features visible in autocomplete actually exist and work
5. **Performance by Default** - The easy path is also the fast path

**Note**: This design describes the **public API surface of katla_gfx** - what users of the library (including katla_app) interact with. Internal implementation details are handled separately.

## Core Principles

- **Builder pattern for configuration** - Fluent, autocomplete-friendly, impossible to get wrong
- **String-based resource names** - No graph compilation step, no lifetime hell
- **Implicit synchronization** - Passes declare reads/writes, graph inserts barriers
- **DrawList integration** - Existing deferred submission works without changes
- **No hybrid states** - One way to do things, not three

---

## Builder API

### FrameGraphBuilder

```rust
use katla_gfx::render_graph::*;

// Create a frame graph - typically done once at startup
let graph = FrameGraph::builder()
    .add_pass(GeometryPass::new("geometry")
        .write_color("color", ImageFormat::R16G16B16A16Sfloat)
        .write_depth("depth", ImageFormat::D32SfloatS8Uint))
    .add_pass(LightingPass::new("lighting")
        .read_color("color")
        .read_depth("depth")
        .write("output", ImageFormat::R8G8B8A8Srgb))
    .build(&renderer)?;

// Execute the graph - done every frame
graph.execute(&renderer, |ctx| {
    // Access passes by name via autocomplete
    ctx.pass("geometry").draw_list(&my_draw_list);
    ctx.pass("lighting").push_uniform(light_data);
})?;
```

### PassBuilder Pattern

All passes follow the same builder pattern:

```rust
// Geometry pass - renders 3D geometry
let geometry = GeometryPass::new("geometry")
    .write_color("color", ImageFormat::R16G16B16A16Sfloat)
    .write_depth("depth", ImageFormat::D32SfloatS8Uint)
    .clear_color([0.1, 0.1, 0.15, 1.0])
    .clear_depth(1.0);

// Fullscreen pass - post-processing
let bloom = FullscreenPass::new("bloom")
    .read("color")
    .write("bloom_out", ImageFormat::R16G16B16A16Sfloat)
    .pipeline(bloom_pipeline);

// Compute pass - GPU compute work
let culling = ComputePass::new("culling")
    .read("depth")
    .write("culled", ImageFormat::R16G16B16A16Sfloat)
    .pipeline(culling_pipeline);
```

---

## Pass Templates

### GeometryPass

Renders 3D geometry with optional depth pre-pass.

```rust
let geometry = GeometryPass::new("geometry")
    .write_color("color", ImageFormat::R16G16B16A16Sfloat)
    .write_depth("depth", ImageFormat::D32SfloatS8Uint)
    .clear_color([0.1, 0.1, 0.15, 1.0])
    .clear_depth(1.0);

// Execution
graph.execute(&renderer, |ctx| {
    ctx.pass("geometry")
        .draw_list(&opaque_draw_list)
        .draw_list(&transparent_draw_list);
})?;
```

**Defaults:**
- Color format: R16G16B16A16Sfloat (HDR friendly)
- Depth format: D32SfloatS8Uint
- Clear color: [0.1, 0.1, 0.15, 1.0]
- Clear depth: 1.0

### FullscreenPass

Post-processing and compute-like pixel work.

```rust
let tone_map = FullscreenPass::new("tone_map")
    .read("hdr_color")
    .write("ldr_output", ImageFormat::R8G8B8A8Srgb)
    .pipeline(tone_map_pipeline);

// Execution
graph.execute(&renderer, |ctx| {
    ctx.pass("tone_map")
        .push_uniform(exposure)
        .dispatch();
})?;
```

**Defaults:**
- Output format: R8G8B8A8Srgb (presentable)
- No clear (reads previous pass output)

### ShadowPass

Shadow mapping for directional and point lights.

```rust
let shadows = ShadowPass::new("shadows")
    .write_depth("shadow_map", ImageFormat::D32Sfloat)
    .resolution(2048, 2048)
    .light_type(LightType::Directional);

// Execution
graph.execute(&renderer, |ctx| {
    ctx.pass("shadows")
        .light_direction(light_dir)
        .draw_list(&shadow_casters);
})?;
```

**Defaults:**
- Resolution: 1024x1024
- Depth format: D32Sfloat
- Clear depth: 1.0

### CustomPass

For special cases, define custom behavior:

```rust
let custom = CustomPass::new("custom", |ctx, resources| {
    // Access resources by name
    let color = resources.texture("color");
    let depth = resources.texture("depth");

    // Full control over command buffer
    ctx.cmd_bind_pipeline(compute_pipeline);
    ctx.cmd_bind_descriptor_set(0, descriptor_set);
    ctx.cmd_dispatch(128, 128, 1);

    // Declare resource transitions
    resources.barrier("color", BarrierKind::ComputeReadWrite);
});
```

---

## Integration with DrawList

The existing DrawList API works without modification:

```rust
let mut draw_list = DrawList::new();

// Opaque geometry
draw_list.push(DrawCall::new(mesh, material)
    .with_transform(model_matrix)
    .with_color([1.0, 0.5, 0.2, 1.0])
    .with_pbr(0.8, 0.2, 1.0));

// Instanced rendering
let instances = vec![instance1, instance2, instance3];
draw_list.push(DrawCall::instanced(mesh, material, instances));

// Sort for optimal rendering
draw_list.compute_sort_keys(camera_position);
draw_list.sort_optimal();

// Submit to graph
graph.execute(&renderer, |ctx| {
    ctx.pass("geometry").draw_list(&draw_list);
})?;
```

### Per-Pass DrawLists

Different passes can use different draw lists:

```rust
graph.execute(&renderer, |ctx| {
    // Shadow pass - casters only
    ctx.pass("shadows").draw_list(&shadow_casters);

    // Geometry pass - everything
    ctx.pass("geometry").draw_list(&all_geometry);

    // Transparent pass - sorted
    ctx.pass("transparent").draw_list(&transparent);
})?;
```

---

## Common Rendering Patterns

### Forward Rendering

Simple forward rendering with post-processing:

```rust
use katla_gfx::render_graph::*;

let graph = FrameGraph::builder()
    .add_pass(GeometryPass::new("geometry")
        .write_color("color", ImageFormat::R16G16B16A16Sfloat)
        .write_depth("depth", ImageFormat::D32SfloatS8Uint))
    .add_pass(FullscreenPass::new("tone_map")
        .read("color")
        .write("output", ImageFormat::R8G8B8A8Srgb)
        .pipeline(tone_map_pipeline))
    .build(&renderer)?;

// Every frame
graph.execute(&renderer, |ctx| {
    ctx.pass("geometry").draw_list(&draw_list);
    ctx.pass("tone_map").dispatch();
})?;
```

### Deferred Shading

Geometry buffer + lighting pass:

```rust
let graph = FrameGraph::builder()
    // G-Buffer generation
    .add_pass(GeometryPass::new("gbuffer")
        .write_color("albedo", ImageFormat::R8G8B8A8Srgb)
        .write_color("normal", ImageFormat::R16G16B16A16Sfloat)
        .write_color("position", ImageFormat::R32G32B32A32Sfloat)
        .write_depth("depth", ImageFormat::D32SfloatS8Uint))
    // Lighting
    .add_pass(FullscreenPass::new("lighting")
        .read("albedo")
        .read("normal")
        .read("position")
        .read("depth")
        .write("color", ImageFormat::R16G16B16A16Sfloat)
        .pipeline(lighting_pipeline))
    // Tone mapping
    .add_pass(FullscreenPass::new("tone_map")
        .read("color")
        .write("output", ImageFormat::R8G8B8A8Srgb)
        .pipeline(tone_map_pipeline))
    .build(&renderer)?;

graph.execute(&renderer, |ctx| {
    ctx.pass("gbuffer").draw_list(&draw_list);
    ctx.pass("lighting").push_uniform(lights).dispatch();
    ctx.pass("tone_map").dispatch();
})?;
```

### Shadow Mapping

Directional shadow + forward rendering:

```rust
let graph = FrameGraph::builder()
    // Shadow pass
    .add_pass(ShadowPass::new("shadows")
        .write_depth("shadow_map", ImageFormat::D32Sfloat)
        .resolution(2048, 2048)
        .light_type(LightType::Directional))
    // Main pass
    .add_pass(GeometryPass::new("geometry")
        .read("shadow_map")
        .write_color("color", ImageFormat::R16G16B16A16Sfloat)
        .write_depth("depth", ImageFormat::D32SfloatS8Uint))
    // Tone mapping
    .add_pass(FullscreenPass::new("tone_map")
        .read("color")
        .write("output", ImageFormat::R8G8B8A8Srgb)
        .pipeline(tone_map_pipeline))
    .build(&renderer)?;

graph.execute(&renderer, |ctx| {
    ctx.pass("shadows")
        .light_direction(light_dir)
        .draw_list(&shadow_casters);

    ctx.pass("geometry")
        .shadow_map("shadow_map")
        .draw_list(&main_geometry);

    ctx.pass("tone_map").dispatch();
})?;
```

### Bloom

Threshold + blur + composite:

```rust
let graph = FrameGraph::builder()
    .add_pass(GeometryPass::new("geometry")
        .write_color("color", ImageFormat::R16G16B16A16Sfloat)
        .write_depth("depth", ImageFormat::D32SfloatS8Uint))
    .add_pass(FullscreenPass::new("bloom_threshold")
        .read("color")
        .write("bright", ImageFormat::R16G16B16A16Sfloat)
        .pipeline(threshold_pipeline))
    .add_pass(FullscreenPass::new("bloom_blur")
        .read("bright")
        .write("bloom", ImageFormat::R16G16B16A16Sfloat)
        .pipeline(blur_pipeline))
    .add_pass(FullscreenPass::new("bloom_composite")
        .read("color")
        .read("bloom")
        .write("output", ImageFormat::R8G8B8A8Srgb)
        .pipeline(composite_pipeline))
    .build(&renderer)?;
```

---

## Error Handling

### Build-Time Errors

Graph construction validates resource lifetimes and detect cycles:

```rust
let graph = FrameGraph::builder()
    .add_pass(GeometryPass::new("p1")
        .write_color("color", ImageFormat::R16G16B16A16Sfloat))
    .add_pass(FullscreenPass::new("p2")
        .read("nonexistent")  // Compile error: resource not declared
        .write("output", ImageFormat::R8G8B8A8Srgb))
    .build(&renderer);

// Result: Err(RenderGraphError::ResourceNotFound("nonexistent"))
```

### Runtime Errors

Execution errors are recoverable and reported clearly:

```rust
match graph.execute(&renderer, |ctx| {
    ctx.pass("geometry").draw_list(&draw_list);
}) {
    Ok(_) => {},
    Err(RenderGraphError::ResourceNotFound { name, pass }) => {
        log::error!("Pass '{}' tried to access unknown resource '{}'", pass, name);
    },
    Err(RenderGraphError::PipelineNotFound { pass }) => {
        log::error!("Pass '{}' has no pipeline assigned", pass);
    },
    Err(e) => log::error!("Render graph error: {:?}", e),
}
```

### Panic-Free Design

All errors return `Result`, never panic:

```rust
// This never panics
if let Some(pass) = ctx.try_pass("optional_pass") {
    pass.draw_list(&optional_draw_list);
}

// Safe access to resources
if let Some(tex) = ctx.try_texture("maybe_exists") {
    // Use tex
}
```

---

## API Surface (Public - Exposed from katla_gfx)

### FrameGraphBuilder

```rust
impl FrameGraphBuilder {
    pub fn new() -> Self;
    pub fn add_pass(mut self, pass: impl Pass) -> Self;
    pub fn build(self, renderer: &VulkanRenderer) -> Result<FrameGraph, RenderGraphError>;
}
```

### FrameGraph

```rust
impl FrameGraph {
    pub fn builder() -> FrameGraphBuilder;
    pub fn execute<F>(&self, renderer: &VulkanRenderer, f: F) -> Result<(), RenderGraphError>
    where F: FnOnce(ExecutionContext);
}
```

### ExecutionContext

```rust
impl ExecutionContext {
    // Access passes by name (autocomplete-friendly)
    pub fn pass(&mut self, name: &str) -> PassHandle;
    pub fn try_pass(&mut self, name: &str) -> Option<PassHandle>;

    // Resource access
    pub fn texture(&self, name: &str) -> &Texture;
    pub fn try_texture(&self, name: &str) -> Option<&Texture>;
}
```

### PassHandle

```rust
impl PassHandle {
    // Geometry pass
    pub fn draw_list(&mut self, draw_list: &DrawList);

    // Fullscreen pass
    pub fn push_uniform(&mut self, data: &[u8]);
    pub fn dispatch(&mut self);

    // Shadow pass
    pub fn light_direction(&mut self, dir: [f32; 3]);
}
```

### Pass Builders

```rust
// GeometryPass
impl GeometryPass {
    pub fn new(name: &str) -> GeometryPassBuilder;
}

impl GeometryPassBuilder {
    pub fn write_color(mut self, name: &str, format: ImageFormat) -> Self;
    pub fn write_depth(mut self, name: &str, format: ImageFormat) -> Self;
    pub fn clear_color(mut self, color: [f32; 4]) -> Self;
    pub fn clear_depth(mut self, depth: f32) -> Self;
}

// FullscreenPass
impl FullscreenPass {
    pub fn new(name: &str) -> FullscreenPassBuilder;
}

impl FullscreenPassBuilder {
    pub fn read(mut self, name: &str) -> Self;
    pub fn write(mut self, name: &str, format: ImageFormat) -> Self;
    pub fn pipeline(mut self, pipeline: PipelineHandle) -> Self;
}

// ShadowPass
impl ShadowPass {
    pub fn new(name: &str) -> ShadowPassBuilder;
}

impl ShadowPassBuilder {
    pub fn write_depth(mut self, name: &str, format: ImageFormat) -> Self;
    pub fn resolution(mut self, width: u32, height: u32) -> Self;
    pub fn light_type(mut self, ty: LightType) -> Self;
}
```

---

## Migration Path

### From Current API

Current code:
```rust
renderer.set_frame_uniforms(uniforms);
renderer.render_frame(&draw_list, &ui_commands)?;
```

Migrates to:
```rust
graph.execute(&renderer, |ctx| {
    ctx.pass("geometry").draw_list(&draw_list);
    ctx.pass("ui").draw_ui(&ui_commands);
})?;
```

### Gradual Adoption

1. Start with a single geometry pass (drop-in replacement)
2. Add post-processing passes incrementally
3. Migrate complex techniques (shadows, deferred) as needed

---

## Implementation Notes

### Resource Management

- Resources are created on first graph execution
- Resources persist across frames (no per-frame allocations)
- Transient resources freed after last use
- Reference counted for automatic lifetime

### Synchronization

- Barriers inserted automatically based on read/write
- Read-after-read: no barrier
- Write-after-read: Shader->Compute barrier
- Write-after-write: Compute->Shader barrier

### Performance

- No graph compilation at runtime (build on creation)
- Pass execution order computed once
- Resource barriers cached between frames
- Zero-allocation execution path

### Debugging

- Pass names included in debug labels
- Resource tracking for validation layers
- Optional graph visualization
- Detailed error messages with pass context
