# Katla

Playground for a Vulkan-based render engine. Nothing too serious.

Currently written in Rust using ash as its Vulkan crate and gpu-allocator for allocation.

## Features

- **Vulkan 1.3 (2026)** with modern rendering patterns:
  - Dynamic Rendering (VK_KHR_dynamic_rendering)
  - Synchronization2 (VK_KHR_synchronization2)
  - VMA integration (gpu-allocator)
- **Custom math library** (katla_math) with SIMD support:
  - SSE implementations on x86/x86_64 for Vec4, Mat4, Quat
  - Scalar fallback for other platforms
  - Vec3 uses scalar implementation (better cache efficiency than SSE)
- **ECS architecture** (katla_ecs) with custom Entity Component System
- **Render graph** system for resource management and pass dependencies
- **Material system** with hot reload support
- **Mesh builder API** for procedural geometry (cube, sphere, cylinder, plane, torus)

Can currently display a scene with some textured models in it.
![bild](https://user-images.githubusercontent.com/5653426/133908105-ab84a179-946e-4f9e-b841-0d7906871326.png)

Will develop in my own pace. Will be testing out lots of layers both as a way to get ahold on Vulkan, but also in order to try out multi-threaded GPU uploads later on! 
