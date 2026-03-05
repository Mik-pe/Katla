# katla_gfx

## Image Barriers

Use `ImageBarrier` helpers - never manually construct `vk::ImageMemoryBarrier`.

```rust
use katla_gfx::barrier::ImageBarrier;

// Common cases (99% of usage)
ImageBarrier::to_color_attachment(cmd, device, image);
ImageBarrier::to_present_src(cmd, device, image);
ImageBarrier::to_shader_read(cmd, device, image);
ImageBarrier::to_depth_attachment(cmd, device, image);
ImageBarrier::shader_read_to_transfer_dst(cmd, device, image);
ImageBarrier::transfer_dst_to_shader_read(cmd, device, image);

// Custom ranges (rare)
ImageBarrier::to_color_attachment_with_range(cmd, device, image, custom_range);
```

Automatic stage/access mask deduction, Vulkan 1.3 sync2.
