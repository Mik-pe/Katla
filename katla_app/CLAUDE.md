# katla_app

Application framework integrating ECS and Vulkan.

## Key Modules

| Module | Purpose |
|--------|---------|
| `application/mod.rs` | Application struct (winit ApplicationHandler) |
| `application/builder.rs` | ApplicationBuilder for setup |
| `components/` | ECS components (Transform, Drawable, FlyCamera) |
| `entities/` | Entity factories (Camera, ModelEntity) |
| `systems/` | ECS systems (FlyCameraSystems) |
| `rendering/` | Rendering abstractions (Drawable, mesh, material) |
| `input/` | Input mapping and binding |

## Application Setup

```rust
let app = ApplicationBuilder::new()
    .with_window(1280, 720, "Katla")
    .with_system(FlyCameraSystem, SystemExecutionOrder::EARLY)
    .build()?;

app.run();
```

## Components

Common components defined here:

- `TransformComponent` - Position, rotation, scale
- `DrawableComponent` - Mesh + material for rendering
- `FlyCamera` - First-person camera controller
- `DirectionalLight` - Sun light source

## Rendering

The rendering module provides:

- `Drawable` trait for renderable objects
- `MeshBuilder` for procedural geometry
- Material wrappers for katla_vulkan materials

## Dependencies

Can depend on: `katla_vulkan`, `katla_ecs`, `katla_math`, `katla_ui`
