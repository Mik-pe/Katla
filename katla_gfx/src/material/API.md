# Material API Guide

This guide explains how to create and use materials in Katla's graphics engine.

## Overview

Materials define how 3D objects are rendered. Each material consists of:
- **Shader** - WGSL code compiled to SPIR-V
- **Pipeline** - Vulkan graphics pipeline with rasterization state
- **Descriptor Sets** - Bound resources (uniforms, textures, etc.)

## Quick Start

```rust
use katla_gfx::{VulkanRenderer, MaterialOptions, VertexType};
use katla_gfx::texture::ImageFormat;

// Create a PBR material with default settings
let material = renderer.compile_material(
    "shaders/pbr.wgsl",
    MaterialOptions {
        vertex_type: VertexType::Pbr,
        color_format: ImageFormat::B8G8R8A8Srgb,
        ..Default::default()
    },
)?;
```

## Material Options

### VertexType

Determines which vertex format the material expects:

| VertexType | Description | Use Case |
|------------|-------------|----------|
| `Pbr` | Standard PBR vertex (position, normal, tangent, UV) | Most 3D models |
| `Ui` | 2D UI vertex (position, UV) | UI elements |
| `Skinned` | Skinned mesh vertex (includes joint indices/weights) | Animated characters |
| `Simple` | Minimal vertex (position only) | Debug visualization, particles |

### Color Format

| Format | Use Case |
|--------|----------|
| `B8G8R8A8Srgb` | LDR rendering to swapchain (default) |
| `R16G16B16A16Sfloat` | HDR intermediate render targets |

### Blend & Render States

| Option | Default | Description |
|--------|---------|-------------|
| `alpha_blended` | `false` | Enable alpha blending for transparent objects |
| `double_sided` | `false` | Disable backface culling |
| `wireframe` | `false` | Render in wireframe mode |

## Common Patterns

### PBR Opaque Material

```rust
let material = renderer.compile_material(
    "shaders/pbr.wgsl",
    MaterialOptions {
        vertex_type: VertexType::Pbr,
        ..Default::default()
    },
)?;
```

### PBR Transparent Material

```rust
let material = renderer.compile_material(
    "shaders/pbr.wgsl",
    MaterialOptions {
        vertex_type: VertexType::Pbr,
        alpha_blended: true,
        ..Default::default()
    },
)?;
```

### UI Material

```rust
let material = renderer.compile_material(
    "shaders/ui.wgsl",
    MaterialOptions {
        vertex_type: VertexType::Ui,
        alpha_blended: true,
        ..Default::default()
    },
)?;
```

### Skinned Character Material

```rust
let material = renderer.compile_material(
    "shaders/skinned.wgsl",
    MaterialOptions {
        vertex_type: VertexType::Skinned,
        ..Default::default()
    },
)?;
```

### HDR Material (for Tonemap Pass)

```rust
let hdr_material = renderer.compile_material(
    "shaders/model.wgsl",
    MaterialOptions {
        vertex_type: VertexType::Pbr,
        color_format: ImageFormat::R16G16B16A16Sfloat,
        ..Default::default()
    },
)?;
```

### Double-Sided Material (foliage, fences)

```rust
let material = renderer.compile_material(
    "shaders/pbr.wgsl",
    MaterialOptions {
        vertex_type: VertexType::Pbr,
        double_sided: true,
        ..Default::default()
    },
)?;
```

## Using Materials with Draw Calls

```rust
use katla_gfx::DrawCall;

// Create a draw call with the material
let draw_call = DrawCall::new(mesh_handle, material_handle)
    .with_transform(model_matrix)
    .with_color([1.0, 0.0, 0.0, 1.0])  // red tint
    .with_pbr(0.0, 0.5, 1.0);         // metallic: 0, roughness: 0.5, ao: 1.0
```

## Descriptor Set Layout

Materials use Katla's standard 3-set descriptor layout:

```wgsl
// Set 0: Per-frame and per-object data (storage buffers)
@group(0) @binding(0) var<storage, read> frame_data: FrameUniforms;
@group(0) @binding(1) var<storage, read> objects: array<ObjectUniforms>;

// Set 1: Bindless textures
@group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 4096>;
@group(1) @binding(1) var shared_sampler: sampler;

// Set 2: Skeletal animation (only for skinned meshes)
@group(2) @binding(0) var<storage, read> joint_matrices: array<mat4x4f>;
```

### Accessing Per-Object Data

In your shader, use `@builtin(instance_index)` to index into the objects array:

```wgsl
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,  // x=metallic, y=roughness, z=ao, w=emission_idx
    texture_indices: vec4<u32>, // x=albedo, y=normal, z=metallic_roughness, w=ao
};

@group(0) @binding(1) var<storage, read> objects: array<ObjectUniforms>;

@vertex
fn vs_main(@builtin(instance_index) instance_idx: u32, ...) -> ... {
    let object = objects[instance_idx];
    let model_matrix = object.model;
    let base_color = object.base_color;
    // ...
}
```

## Texture Binding

Textures are bound via the bindless texture system. The application sets texture indices via `ObjectUniforms.texture_indices`:

```rust
draw_call.with_texture_indices([albedo_idx, normal_idx, mr_idx, ao_idx]);
```

In the shader:

```wgsl
@group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 4096>;
@group(1) @binding(1) var shared_sampler: sampler;

@fragment
fn fs_main(...) -> ... {
    let object = objects[instance_idx];
    let albedo_idx = object.texture_indices.x;
    let albedo = textureSample(bindless_textures[albedo_idx], shared_sampler, uv);
    // ...
}
```

## Deferred Materials (Auto Format)

For materials that need to work with multiple formats (e.g., shared between HDR and LDR passes), use `ImageFormat::Auto`:

```rust
// Note: This is primarily for internal use by the frame graph system
let material = renderer.compile_material(
    "shaders/shared.wgsl",
    MaterialOptions {
        vertex_type: VertexType::Pbr,
        color_format: ImageFormat::Auto,  // Compiled on first use
        ..Default::default()
    },
)?;
```

The frame graph will compile the material for the correct format when the pass is first executed.

## Error Handling

Material compilation can fail for several reasons:

```rust
use katla_gfx::RendererError;

match renderer.compile_material("shaders/pbr.wgsl", options) {
    Ok(handle) => { /* use handle */ }
    Err(RendererError::InitializationFailed(msg)) => {
        eprintln!("Material compilation failed: {}", msg);
    }
    Err(e) => {
        eprintln!("Unexpected error: {:?}", e);
    }
}
```

Common failure modes:
- Shader file not found
- WGSL compilation errors
- Pipeline creation failures
- Invalid vertex type for the shader

## Performance Notes

1. **Materials are cached** - Creating the same material twice returns the same handle
2. **Compile at startup** - Create all materials during initialization, not during gameplay
3. **Share materials** - Multiple meshes can use the same material
4. **Format matters** - HDR materials have different pipelines than LDR

## See Also

- [`VulkanRenderer::compile_material()`](../renderer/struct.VulkanRenderer.html#method.compile_material)
- [`MaterialOptions`](../vulkan/material/compiler/struct.MaterialOptions.html)
- [`VertexType`](../vulkan/material/compiler/enum.VertexType.html)
- [`DrawCall`](../renderer/types/struct.DrawCall.html)
