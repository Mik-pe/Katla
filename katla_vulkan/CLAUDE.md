# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in the katla_vulkan crate.

## Crate Overview
This is the katla_vulkan crate of the katla 3d game engine:
- **katla_vulkan** - Vulkan abstraction layer with render graph system

## Critical Architecture Rules

**Rules:**
- **katla_vulkan** must NOT depend on: `katla_math`, `katla_ecs`, `katla_app`, `katla_ui`
- **katla_ecs** must NOT depend on: `katla_app`, `katla_vulkan`, `katla_math`, `katla_ui`
- **katla_math** must NOT depend on: ANY other crate
- **katla_ui** must NOT depend on: `katla_ecs`, `katla_app`
- **katla_ui** CAN depend on: `katla_math`, `katla_vulkan`
- **katla_app** can depend on: `katla_vulkan`, `katla_ecs`, `katla_math`, `katla_ui`

### Ash Type Exclusion Rule

**CRITICAL**: `katla_vulkan` crate must NOT export or re-export `ash::vk` types in its public API.

- Create wrapper types for all Vulkan types (see `src/render_graph/types.rs` and `src/vulkan/vertexbuffer.rs` for `IndexType`)
- Use type aliases internally if needed, but NEVER `pub use ash::vk`
- Downstream crates (katla_app) should NOT need to depend on ash directly
- Wrapper types should implement `From<Wrapper> for vk::Type` and `From<vk::Type> for Wrapper` for conversions
- Place wrapper types in the module where they're used (e.g., `IndexType` in `vertexbuffer.rs`) or in dedicated type modules (e.g., `render_graph/types.rs`)

**Note**: Some internal APIs still expose `vk::` types (e.g., `Framebuffer`, `Pipeline`, `CommandBuffer` methods). These should be wrapped when exposed in public APIs.

## Render Graph Architecture
Located in `src/render_graph/`, this is a high-level abstraction for Vulkan rendering.

### Key Modules
- **types.rs** - Wrapper types for Vulkan (ImageFormat, ImageLayout, Extent2D/3D, etc.)
- **resource.rs** - ResourceKind (Buffer, Image, ExternalBuffer, ExternalImage), ResourceUsage
- **graph.rs** - RenderGraph builder and resource management
- **pass.rs** - PassBuilder, Pass, PassExecutionContext, ExecutionRegistry
- **compiled.rs** - CompiledRenderGraph, compilation pipeline (lifetime analysis, render pass generation)
- **frame_resources.rs** - FrameResources struct with pre-registered render targets
- 
### FrameResources (pre-registered by VulkanRenderer)
- `swapchain` - Current swapchain image (changes each frame)
- `viewport_color` - Offscreen render target for 3D scene
- `viewport_depth` - Depth buffer for viewport
- `output_color` - Final composition target (scene + UI)

### Key Points
- Use `create_render_graph_with_resources()` for pre-registered targets
- Use `write_color()`/`write_depth()` for render targets
- Use `blit()` for transfer operations between images
- The graph automatically uses dynamic rendering (no traditional render passes)
- Synchronization2 barriers inserted automatically for layout transitions

### Execution Flow

1. Get builder with `renderer.create_render_graph_with_resources()` → returns `(RenderGraphBuilder, FrameResources)`
2. Add passes: `builder.add_pass(name, |pass| { ... })`
3. Each pass declares what it reads/writes via `pass.write_color()`, `pass.blit()`, etc.
4. Compile with `renderer.compile_render_graph(builder, swapchain_resource_id)`
5. Each frame, `render_frame()` executes the compiled graph

### Wrapper Types Pattern

All Vulkan types wrapped as enums/structs implementing `From<ash::vk::T>`:
```rust
pub enum ImageFormat {
    R8G8B8A8Srgb,
    D32Sfloat,
    // ...
}

impl From<ImageFormat> for ash::vk::Format { ... }
```

## Modern Vulkan 1.3 Rendering

**Status**: ✅ The Katla engine uses modern Vulkan 1.3 (2026) rendering patterns.

### Key Modern Features in Use

- **Dynamic Rendering (VK_KHR_dynamic_rendering)** - Production rendering uses `vkCmdBeginRendering`/`vkCmdEndRendering` instead of legacy render passes
- **Synchronization2 (VK_KHR_synchronization2)** - All pipeline barriers use `vkCmdPipelineBarrier2` with modern barrier types
- **VMA Integration** - Uses `gpu_allocator` for Vulkan Memory Allocator integration
- **Frames In-Flight** - Proper per-frame synchronization with fences and semaphores
- **Bindless Textures** - Single texture array descriptor instead of per-texture descriptors

### Synchronization Pattern

Use **Synchronization2** for all barriers:

```rust
// Modern Synchronization2 barrier pattern
let barrier = ImageMemoryBarrier2::new(image)<...>;

DependencyInfo::new()
    .add_image_barrier(barrier)
    .build(|dep_info| unsafe {
        context.device.cmd_pipeline_barrier2(command_buffer, dep_info);
    });
```

**Do NOT use** legacy barrier pattern:
```rust
// ❌ LEGACY - DO NOT USE
context.device.cmd_pipeline_barrier( 
    ...
);
```

## Vulkan Wrapper Layer

Located in `src/vulkan/`, wraps raw ash calls with idiomatic Rust.

### Key Modules
- **context.rs** - VulkanContext (device, instance, physical device selection)
- **commandbuffer.rs** - CommandBuffer wrapper with `begin_rendering()`/`end_rendering()` for dynamic rendering
- **texture.rs** - Texture loading and image creation (uses Synchronization2)
- **pipeline/** - Pipeline creation infrastructure
- **material/** - Material system with hot reload support
- **particle_buffer.rs** - GPU particle buffers using `DeviceAddressBuffer`

## Material System

Materials use template-based configuration with hot reload:

- **TOML-based material definitions** - define shaders, textures, parameters
- **No render pass dependency** - materials work with dynamic rendering
- **Hot reload** - reloads at runtime

## Shader System (WGSL + naga)

Katla uses **WGSL shaders** compiled to SPIR-V via the **naga library** (not the naga CLI binary).

### Compilation Pipeline

```
WGSL source (.wgsl files)
    ↓
naga::front::wgsl::parse_str()  [in shadermodule.rs]
    ↓
naga::back::spv::write_vec()    [generates SPIR-V]
    ↓
vk::ShaderModule
```

### Key Files

- `src/vulkan/material/shadermodule.rs` - `ShaderModule::from_wgsl()` and `from_wgsl_string()`
- `src/vulkan/material/reflection.rs` - naga-based shader reflection for uniform layouts
- `resources/shaders/*.wgsl` - All shader source files 
- `resources/materials/*.toml` - All material source files (references shaders)

### Example Shader Structure

```wgsl
// Uniform buffer binding (set 0, binding 0 typically)
struct FrameUniforms {
    view_proj: mat4x4<f32>,
    camera_position: vec3<f32>,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> frame: FrameUniforms;

// Storage buffer binding (for particle systems, etc.)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // ...
}
```

## Code Review Guidelines

### Performance Patterns

1. **Make small structs `Copy`** - If a struct only contains primitives, derive `Copy` to eliminate clone overhead in hot paths (e.g., `CachedGlyph` for text rendering)
2. **Avoid `.clone()` before iteration** - Use `for &x in &collection` instead of `for x in collection.clone()`
3. **Use helper functions for repeated patterns** - If you draw 4 border rects in multiple places, create `draw_selection_border()` helper
4. **Prefer macros for repetitive struct initialization** - Theme definitions reduced 41% by using a macro
5. **Look for similar patterns**: Sometimes we already have an implementation similar to what we're implementing. 
  - We should ALWAYS try to reuse and reduce code, especially in the public API
  - We should try to extend existing code over creating new custom codepaths.

### RHI Abstraction Principles

The katla_vulkan crate should maintain proper RHI (Render Hardware Interface) abstraction:

1. **No raw `ash::vk` types in public API** - All Vulkan types must be wrapped
2. **`vk()` methods should be `pub(crate)`** - Internal access only
3. **Opaque handles for resources** - Use `MeshHandle(usize)` not `&Mesh`
4. **Consistent abstraction levels** - High-level (DrawCall), Mid-level (RenderGraph), Low-level (Context)

See `.claude/skills/vulkan-rhi-validator/` for detailed guidelines.
