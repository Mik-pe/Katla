# katla_gfx

## Descriptor Set Layout

The rendering pipeline uses a **3-set descriptor layout** for efficient resource binding:

```wgsl
// Set 0: Per-Frame Data (shared across all draws)
@group(0) @binding(0) var<storage, read> frame_data: FrameUniforms;
// Camera (view/proj), lighting, time, etc.

// Set 1: Per-Object Data (indexed by instance_index)
@group(1) @binding(0) var<storage, read> objects: array<ObjectUniforms>;
// Model matrix, material params, texture indices

// Set 2: Global Resources (bindless textures, skinning, etc.)
@group(2) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 4096>;
@group(2) @binding(1) var shared_sampler: sampler;
@group(2) @binding(2) var<storage, read> skeleton_data: array<SkeletonUniforms>; // for skinned meshes
```

**Why this layout?**
- **Set 0** updates once per frame (camera moves, lighting changes)
- **Set 1** uses instance indexing - no per-draw descriptor updates
- **Set 2** contains bindless textures and optional features (skinning)

**Implementation:**
- `StorageUniformManager` - Manages Set 0 (frame) + Set 1 (objects) in one buffer
- `BindlessTextureManager` - Manages Set 2 (textures + sampler)
- `SkeletonDescriptorSet` - Optional Set 2 binding for skinned meshes

## Image Barriers

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
