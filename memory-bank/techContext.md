# Tech Context

## Language & Edition

Rust 2024 edition. Clippy + rustfmt enforced.

## Key Dependencies

| Crate | Dependency | Purpose |
|-------|-----------|---------|
| katla_gfx | `ash` | Vulkan bindings |
| katla_gfx | `objc2-metal` | Metal bindings (macOS) |
| katla_gfx | `naga` | WGSL→SPIR-V/Metal shader compilation at runtime |
| katla_gfx | `gpu-allocator` (VMA) | GPU memory management |
| katla_ecs | `rayon` | Parallel system execution |
| katla_ui | `taffy` | Flexbox layout |
| katla_ui | `bytemuck` | Pod/Zeroable for GPU data |
| katla_ui | `slotmap` | Stable element IDs |
| katla_physics | `rapier3d` | Physics engine |
| katla_script | `mlua` (luau, vendored) | Lua VM |
| katla_script | `notify` | File system watcher for script hot reload |

## Build & Run

```bash
cargo check                    # Quick typecheck
cargo build                    # Build all
cargo build -p <crate>         # Build specific crate
cargo test                     # Run all tests
cargo test -p <crate>          # Test specific crate
cargo test -- --nocapture      # Show stdout
cargo clippy                   # Lint
cargo fmt                      # Format
cargo run                      # Run the app
cargo run -- -s                # Limited-frame mode (100 frames, validation)
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo run -- -s  # Metal validation (macOS)
```

## Features

- `editor` — katla_ecs, katla_physics, katla_script: enables inspector, agent, scene_tool modules. Adds serde_json dep.
- `validation` — katla_gfx: promotes internal modules from `pub(crate)` to `pub` for validation examples/benchmarks.

## Shader Pipeline

WGSL is canonical: Naga produces SPIR-V for Vulkan and MSL for Metal. Metal 4 compiler work and native archive persistence run through one worker service; known variants are warmed before frames, and reload swaps only successfully compiled replacements. Frame encoding cannot compile a pipeline. Sampled Metal textures use bounded staged uploads into private GPU storage. See the pipeline, binding/residency, frame-slot and texture-upload documents under docs/.

## Asset Pipeline

GLTF models with skeletal animation, PBR materials. Template-based material definitions. Background loading. `ResourceManager::discover()` finds assets from `resources/` directory.
