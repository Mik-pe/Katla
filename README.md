# Katla ✨🎮

![logo](logo.png)

A Vulkan game engine in Rust. Playground for graphics experiments. 🐒

## What's Inside 📦

- **Vulkan 1.3** 🔺 - Dynamic rendering, Synchronization2, VMA integration, bindless textures
- **Custom ECS** 🧩 - Sparse set storage, query system, component derive macros
- **Render graph** 📊 - Resource lifetime management, automatic barrier insertion
- **PBR materials** 💎 - Hot reload support, template-based definitions
- **WGSL shaders** ✨ - Compiled via naga at runtime
- **GLTF support** 🦊 - Skeletal animation, PBR materials, background loading
- **Editor UI** 🖼️ - Declarative dockable panels, asset browser, entity inspector, transform gizmos, CodeEditor with syntect highlighting
- **Bindless textures** 🎨 - Single texture array for UI rendering, texture switching via vertex indices, no push descriptor overhead
- **Text pipeline** 🔤 - cosmic-text with HarfBuzz shaping, BiDi, CJK, word wrapping, font fallback; swash rasterization, etagere atlas packing, subpixel positioning
- **GPU-instanced UI** ⚡ - Instanced rendering (shared unit quad + per-instance data) replaces per-quad vertex emission; incremental Taffy layout caching via dirty flags

## Crates 📚

| Crate | Description |
|-------|-------------|
| `katla_gfx` | Vulkan wrapper, render graph, materials |
| `katla_ecs` | Entity component system |
| `katla_math` | SIMD math library |
| `katla_ui` | Declarative UI system — Widget trait, focus chains, dockable panels, cosmic-text pipeline, GPU-instanced rendering, CodeEditor (622 tests) |
| `katla_app` | Application framework, components, systems |

## Running 🏃

```bash
cargo run        # Run the demo 🎮
cargo run -- -s  # Limited frames (validation) ✅
cargo test       # Run tests 🧪
```

## Is this vibecoded? 🤖
**It sure is, I ain't got time to write all of this**  
This repo has become my playground for vibecoding to see how good or bad it can be.
