# Frame Graph Pass Builder API Guide

This guide explains how to use Katla's frame graph pass builder system to construct rendering pipelines.

## Overview

The frame graph system allows you to declare render passes and their resource dependencies. The graph then:
- Compiles the execution order
- Generates appropriate Vulkan barriers
- Manages transient resource lifetime

## Pass Types

Katla provides several pre-built pass templates:

| Pass | Description | Use Case |
|------|-------------|----------|
| `GeometryPass` | Renders 3D geometry with depth | Standard 3D rendering |
| `FullscreenPass` | Fullscreen triangle post-processing | Tonemapping, blur, effects |
| `UIPass` | 2D UI rendering with alpha blending | Debug overlays, in-game HUD |
| `ShadowPass` | Shadow map generation | Directional/spot light shadows |

## Quick Start

```rust
use katla_gfx::{FrameGraph, GeometryPass, FullscreenPass, UIPass};
use katla_gfx::render_graph::{GraphResourceDesc, GraphResourceType};
use katla_gfx::texture::ImageFormat;
use katla_gfx::render_pass::{ClearValue, LoadOp, StoreOp};

let extent = renderer.swapchain_extent();

let graph = renderer.create_frame_graph()
    // Create an HDR texture for geometry output
    .create_resource(GraphResourceDesc {
        name: "hdr_color".to_string(),
        resource_type: GraphResourceType::ColorAttachment {
            clear_value: Some([0.1, 0.1, 0.1, 1.0]),
        },
        format: ImageFormat::R16G16B16A16Sfloat,
        width: extent.width,
        height: extent.height,
    })
    // Sky pass (renders to HDR texture)
    .add_pass(FullscreenPass::new("sky")
        .write("hdr_color", ImageFormat::R16G16B16A16Sfloat)
        .pipeline(sky_pipeline))
    // Geometry pass (renders 3D scene to HDR)
    .add_pass(GeometryPass::new("geometry")
        .write_color_with(
            "hdr_color",
            ImageFormat::R16G16B16A16Sfloat,
            LoadOp::Load,     // Load sky pass output
            StoreOp::Store,
            ClearValue::OPAQUE_BLACK,
        )
        .write_depth("depth", ImageFormat::D32Sfloat))
    // Tonemap pass (HDR -> LDR, output to backbuffer)
    .add_pass(FullscreenPass::new("tonemap")
        .read("hdr_color")
        .write_backbuffer()
        .pipeline(tonemap_pipeline)
        .tonemap(tonemap_params))
    // UI pass (drawn on top of tonemapped output)
    .add_pass(UIPass::new("ui")
        .write("backbuffer")
        .material(ui_material))
    .build()?;
```

## Pass Builder Methods

### GeometryPass

Renders 3D geometry with depth testing and writing.

```rust
GeometryPass::new("geometry")
    // Color output
    .write_color("color", ImageFormat::R16G16B16A16Sfloat)

    // OR with explicit load/store ops
    .write_color_with(
        "color",
        ImageFormat::R16G16B16A16Sfloat,
        LoadOp::Clear,
        StoreOp::Store,
        ClearValue::OPAQUE_BLACK,
    )

    // Depth buffer (required)
    .write_depth("depth", ImageFormat::D32Sfloat)

    // Optional: Read resources (e.g., shadow maps)
    .read("shadow_map")

    // Optional: Associate a material
    .material(material_handle);
```

### FullscreenPass

Renders a fullscreen triangle for post-processing.

```rust
FullscreenPass::new("tonemap")
    // Read from a texture (e.g., HDR color)
    .read("hdr_color")

    // Write to backbuffer (swapchain)
    .write_backbuffer()

    // OR write to a custom texture
    .write("output", ImageFormat::B8G8R8A8Srgb)

    // Set the graphics pipeline
    .pipeline(pipeline_handle)

    // Optional: Tonemap parameters
    .tonemap(tonemap_params);
```

### UIPass

Renders 2D UI with alpha blending.

```rust
UIPass::new("ui")
    // Write to backbuffer or any color texture
    .write("backbuffer")

    // Set UI material
    .material(ui_material)

    // Optional: Read resources (e.g., font atlas, thumbnails)
    .read("font_atlas");
```

### ShadowPass

Renders shadow maps for lighting.

```rust
ShadowPass::new("shadow")
    .write_depth("shadow_map", ImageFormat::D32Sfloat)
    .light_type(LightType::Directional)
    .resolution(2048, 2048);
```

## Resource Declaration

### Transient Resources

Resources created and managed by the frame graph:

```rust
.create_resource(GraphResourceDesc {
    name: "hdr_color".to_string(),
    resource_type: GraphResourceType::ColorAttachment {
        clear_value: Some([0.1, 0.1, 0.1, 1.0]),
    },
    format: ImageFormat::R16G16B16A16Sfloat,
    width: 1920,
    height: 1080,
})
```

### Resource Types

| Type | Description |
|------|-------------|
| `ColorAttachment` | Color output texture |
| `DepthAttachment` | Depth buffer |

### Importing External Resources

```rust
.import_resource("external_texture", texture_handle)
```

## Resource Lifecycles

The frame graph automatically manages resource transitions:

1. **First write** - Resource transitions from `UNDEFINED` to `COLOR_ATTACHMENT_OPTIMAL`
2. **Subsequent writes** - Uses `LOAD` op to preserve previous pass output
3. **Read after write** - Automatically transitions to `SHADER_READ_ONLY_OPTIMAL`
4. **Multiple passes writing** - Each pass can use `LOAD` to preserve and add to the output

### Backbuffer Special Case

The backbuffer (swapchain) is a special resource:
- Use `write_backbuffer()` instead of `write("backbuffer")` (more discoverable)
- First pass writing to backbuffer uses `CLEAR` op
- Subsequent passes use `LOAD` op to preserve previous output

```rust
// First pass: clears to solid color
.add_pass(FullscreenPass::new("tonemap")
    .write_backbuffer()  // CLEAR op
    .pipeline(tonemap_pipeline))

// Second pass: loads tonemapped output and draws UI on top
.add_pass(UIPass::new("ui")
    .write_backbuffer()  // LOAD op (preserves tonemap output)
    .material(ui_material))
```

## Execution

Submit draw lists to passes during frame execution:

```rust
renderer.render(&mut frame_graph, |frame| {
    // Submit draw lists to named passes
    frame.submit("geometry", &opaque_draw_list);
    frame.submit("geometry", &transparent_draw_list);

    // Submit UI draw list
    frame.submit_ui("ui", &ui_draw_list);
})?;
```

Passes without submitted draw lists still execute (useful for fullscreen post-processing passes).

## Load/Store Operations

| LoadOp | Description |
|--------|-------------|
| `Load` | Preserve existing contents |
| `Clear` | Clear to clear_value |
| `DontCare` | Contents undefined (may not be preserved) |

| StoreOp | Description |
|---------|-------------|
| `Store` | Keep results for later use |
| `DontCare` | Discard after pass |

## Clear Values

```rust
// Color clear value
ClearValue::Color([r, g, b, a])

// Depth clear value
ClearValue::DepthStencil { depth: 1.0, stencil: 0 }

// Predefined colors
ClearValue::OPAQUE_BLACK   // [0, 0, 0, 1]
ClearValue::TRANSPARENT_BLACK  // [0, 0, 0, 0]
```

## Common Patterns

### Deferred Rendering

```rust
let graph = renderer.create_frame_graph()
    // Geometry pass - outputs position, normal, albedo
    .create_resource(gbuffer_pos_desc)
    .create_resource(gbuffer_normal_desc)
    .create_resource(gbuffer_albedo_desc)
    .add_pass(GeometryPass::new("geometry")
        .write_color("gbuffer_pos", ImageFormat::RGBA16SFLOAT)
        .write_color("gbuffer_normal", ImageFormat::RGBA16SFLOAT)
        .write_color("gbuffer_albedo", ImageFormat::B8G8R8A8Srgb)
        .write_depth("depth", ImageFormat::D32Sfloat))
    // Lighting pass - reads gbuffer, outputs to backbuffer
    .add_pass(FullscreenPass::new("lighting")
        .read("gbuffer_pos")
        .read("gbuffer_normal")
        .read("gbuffer_albedo")
        .write_backbuffer()
        .pipeline(lighting_pipeline))
    .build()?;
```

### Forward Rendering with Tonemapping

```rust
let graph = renderer.create_frame_graph()
    // HDR render target
    .create_resource(hdr_desc)
    // Scene pass (HDR)
    .add_pass(GeometryPass::new("scene")
        .write_color("hdr_color", ImageFormat::R16G16B16A16Sfloat)
        .write_depth("depth", ImageFormat::D32Sfloat))
    // Tonemap (HDR -> LDR)
    .add_pass(FullscreenPass::new("tonemap")
        .read("hdr_color")
        .write_backbuffer()
        .pipeline(tonemap_pipeline))
    .build()?;
```

### Cascaded Shadow Maps

```rust
let graph = renderer.create_frame_graph()
    // Cascade 0
    .add_pass(ShadowPass::new("shadow_cascade_0")
        .write_depth("shadow_cascade_0", ImageFormat::D32Sfloat)
        .resolution(2048, 2048))
    // Cascade 1
    .add_pass(ShadowPass::new("shadow_cascade_1")
        .write_depth("shadow_cascade_1", ImageFormat::D32Sfloat)
        .resolution(1024, 1024))
    // Scene pass reads shadow maps
    .add_pass(GeometryPass::new("scene")
        .read("shadow_cascade_0")
        .read("shadow_cascade_1")
        .write_color("color", ImageFormat::B8G8R8A8Srgb)
        .write_depth("depth", ImageFormat::D32Sfloat))
    .build()?;
```

## Error Handling

```rust
use katla_gfx::render_graph::RenderGraphError;

match graph.build() {
    Ok(graph) => { /* use graph */ }
    Err(RenderGraphError::ResourceNotFound(name)) => {
        eprintln!("Resource not found: {}", name);
    }
    Err(RenderGraphError::InvalidPipelineHandle(handle)) => {
        eprintln!("Invalid pipeline handle: {:?}", handle);
    }
    Err(e) => {
        eprintln!("Failed to build frame graph: {:?}", e);
    }
}
```

## Performance Notes

1. **Build once, execute many** - Frame graphs are built at startup and executed every frame
2. **Resource reuse** - Transient resources are allocated once and reused across frames
3. **Barrier efficiency** - The graph generates minimal barriers based on actual usage
4. **Pass culling** - Passes with no draw lists still execute but are very cheap

## Advanced: Custom Passes

For specialized rendering, you can create custom passes by implementing the `PassBuilder` trait:

```rust
use katla_gfx::render_graph::PassBuilder;

pub struct CustomPass {
    name: String,
    reads: Vec<String>,
    writes: Vec<String>,
}

impl PassBuilder for CustomPass {
    fn as_builder(self) -> InternalPassBuilder {
        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads,
            writes: self.writes,
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: None,
            build_fn: Box::new(|_resource_map| {
                Ok(Box::new(CustomPassData))
            }),
            uses_depth: false,
        }
    }
}
```

## See Also

- [`FrameGraph`](../render_graph/struct.FrameGraph.html)
- [`GeometryPass`](../render_graph/passes/geometry/struct.GeometryPass.html)
- [`FullscreenPass`](../render_graph/passes/fullscreen/struct.FullscreenPass.html)
- [`UIPass`](../render_graph/passes/ui/struct.UIPass.html)
- [`ShadowPass`](../render_graph/passes/shadow/struct.ShadowPass.html)
- [`VulkanRenderer::render()`](../renderer/struct.VulkanRenderer.html#method.render)
