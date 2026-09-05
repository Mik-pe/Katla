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

The `-s` / `--single-frame` flag runs **100 frames**, then exits automatically.
On Arch Linux, install `vulkan-validation-layers` with pacman to enable Khronos
validation in normal runs. Use `cargo run -- -s -v` for GPU-assisted validation.

## Headless captures

Render the scene and editor without a window (Vulkan on Linux, Metal on macOS):

```bash
cargo run -p game -- --headless -s --screenshot /tmp/katla.png
cargo run -p game -- --ui-test /tmp/katla-ui
cargo run -p game -- --headless -s --scene assets/scenes/playground.katla --screenshot /tmp/playground.png
```

Captures are 2560×1440 PNGs with a 1280×720 logical UI. The UI test captures
five states, including entity selection and Preferences. A Vulkan device and
its driver are required on Linux; no display server is needed. Install the
Khronos validation layer to include Vulkan API checks.

The GPU submission/readback regression test is opt-in:

```bash
cargo test -p katla_gfx --test headless_render -- --ignored
```

## Is this vibecoded? 🤖
**It sure is, I ain't got time to write all of this**  
This repo has become my playground for vibecoding to see how good or bad it can be.
