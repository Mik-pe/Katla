# katla_gfx

## Cross-Backend Architecture

katla_gfx supports two rendering backends:
- **Vulkan** — via `ash`, all platforms (macOS uses MoltenVK)
- **Metal** — via `objc2-metal`, native macOS backend (cfg-gated behind `target_os = "macos"`)

### Backend Selection

Runtime selection via `AnyRenderer` enum:
```rust
// Vulkan backend (all platforms)
let renderer = AnyRenderer::new_vulkan(display, window, ValidationMode::Full, app_name, engine_name)?;

// Metal backend (macOS only)
let renderer = AnyRenderer::new_metal(display, window, ValidationMode::Full, app_name, engine_name)?;
```

The `GpuRenderer` trait defines the backend-agnostic API. Both `VulkanRenderer` and `MetalRenderer` implement it. `AnyRenderer` dispatches dynamically.

### Shared Types (Backend-Agnostic)

These types are identical across both backends:
- `Handle<T>` — opaque resource handles (MeshHandle, MaterialHandle, TextureHandle, SkeletonHandle)
- `ImageFormat` — pixel format enum
- `DrawList`, `DrawCall`, `FrameUniforms`, `InstanceData` — per-frame data
- `VertexPBR`, `VertexPBRSkinned`, `VertexUI` — vertex layouts
- `TextureDescriptor`, `TextureUsage` — texture creation parameters
- `LoadOp`, `StoreOp`, `ClearValue`, `BarrierKind` — render pass operations
- `Viewport`, `ViewportBuilder`, `ViewportHandle` — viewport management
- Pass templates: `GeometryPass`, `FullscreenPass`, `ShadowPass`, `UIPass`, etc.

### Backend-Specific Code

Each backend lives in its own module:
- `vulkan/` — VulkanContext, VulkanRenderer internals, ash types
- `metal/` — MetalContext, MetalRenderer internals, objc2-metal types

Backend-specific code should stay in these modules. The public API uses `GpuRenderer` trait methods.

### Render Graph

The render graph is generic over the backend:
- `FrameGraph<R: GpuRenderer>` — compiled render graph
- `RenderGraphBackend` trait — backend operations (transient texture creation, bindless)
- `AnyFrameGraph` / `AnyFrame` — runtime dispatch between Vulkan and Metal frame graphs

### When Adding New Features

1. Add the method to `GpuRenderer` trait first (with default no-op impl)
2. Implement for both `VulkanRenderer` and `MetalRenderer`
3. Add dispatch to `AnyRenderer` enum
4. If it involves render graph resources, extend `RenderGraphBackend` trait

## Descriptor Set Layout (Vulkan)

The Vulkan rendering pipeline uses a **3-set descriptor layout** for efficient resource binding:

```wgsl
// ===== Set 0: Per-Frame and Per-Object Data (Storage Buffers) =====
// Binding 0: Frame-level uniforms (shared across all draws)
@group(0) @binding(0) var<storage, read> frame_data: FrameUniforms;

// Binding 1: Per-object array (indexed by @builtin(instance_index))
@group(0) @binding(1) var<storage, read> objects: array<ObjectUniforms>;

// ===== Set 1: Global Resources (Bindless Textures) =====
// Binding 0: Bindless texture array (up to 4096 textures)
@group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 4096>;

// Binding 1: Shared sampler for all textures
@group(1) @binding(1) var shared_sampler: sampler;

// ===== Set 2: Optional Features (Skeletal Animation) =====
// Binding 0: Joint matrices for GPU skinning (only bound for skinned meshes)
@group(2) @binding(0) var<storage, read> joint_matrices: array<mat4x4f>;
```

**Struct Definitions:**
```wgsl
// Frame uniforms (updated once per frame)
struct FrameUniforms {
    view: mat4x4f,              // View matrix (world to camera)
    proj: mat4x4f,              // Projection matrix (camera to clip)
    inv_view_proj: mat4x4f,     // Inverse view-projection (for ray casting)
    camera_position: vec4f,     // Camera position in world space
    light_direction: vec4f,     // Direction TO light (normalized)
    light_color: vec4f,         // Light color (RGB)
    light_intensity: vec4f,     // HDR intensity multiplier
}

// Per-object uniforms (one entry per drawn instance)
struct ObjectUniforms {
    model: mat4x4f,              // Model matrix (object to world)
    base_color: vec4f,           // Tint color
    material_params: vec4f,      // x=metallic, y=roughness, z=ao, w=emission_idx
    texture_indices: vec4<u32>,  // x=albedo, y=normal, z=metallic_roughness, w=ao
}
```

**Why this layout?**
- **Set 0** - Storage buffers with frame data + per-object array
  - Frame uniforms bound once per frame
  - Object array indexed by `@builtin(instance_index)` - no per-draw updates
  - Up to 256 objects per frame (configurable)

- **Set 1** - Bindless textures (shared across all materials)
  - Single descriptor set for all textures
  - Texture indices come from per-object `ObjectUniforms`
  - No need for per-material descriptor sets

- **Set 2** - Optional skeletal animation (only bound when needed)
  - Each skinned mesh gets its own `SkeletonDescriptorSet`
  - Joint matrices updated per-frame by animation system
  - Not bound for static geometry

**Implementation:**
- `StorageUniformManager` - Manages Set 0 (frame + objects storage buffer)
- `BindlessTextureManager` - Manages Set 1 (bindless textures)
- `SkeletonDescriptorSet` - Set 2 binding for skinned meshes

**For Shader Authors:**
- Use `@builtin(instance_index)` to index `objects[]` array
- Access textures via `bindless_textures[texture_indices.x]` pattern
- Only declare Set 2 if writing a skinned mesh shader
- Never use push constants (not supported in WGSL/WebGPU)

## Image Barriers (Vulkan-only)

These apply only to the Vulkan backend. Metal has no image layout concept — synchronization is handled via `MTLFence`/`MTLEvent` or is implicit on Apple Silicon.

Use `ImageBarrier` helpers - never manually construct `vk::ImageMemoryBarrier`.

The new API uses **explicit source layouts** - the source layout determines whether contents are preserved.

```rust
use katla_gfx::barrier::ImageBarrier;
use ash::vk;

// Fresh images (discard contents) - most common
ImageBarrier::transition_from_undefined(cmd, device, image,
    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
ImageBarrier::transition_from_undefined(cmd, device, image,
    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

// Preserving contents (transitioning between used states)
ImageBarrier::transition(cmd, device, image,
    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,   // from
    vk::ImageLayout::PRESENT_SRC_KHR);          // to

// Texture updates (preserving contents)
ImageBarrier::transition(cmd, device, image,
    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    vk::ImageLayout::TRANSFER_DST_OPTIMAL);
// ... upload data ...
ImageBarrier::transition(cmd, device, image,
    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

// Custom ranges with depth images
ImageBarrier::transition_from_undefined_with_range(cmd, device, image,
    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    DEPTH_SUBRESOURCE_RANGE);
```

Automatic stage/access mask deduction, Vulkan 1.3 sync2.

Before using `vk::` types, LOOK FOR EXISTING WRAPPERS/HELPERS - they may already implement the resource handling internally.
