# katla_gfx

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
