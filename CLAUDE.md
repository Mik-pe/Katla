# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Katla is a Vulkan-based 3D render engine written in Rust, using ECS (Entity Component System) architecture. The project is structured as a Cargo workspace with multiple crates:

- **katla_math** - Custom math library (vectors, matrices, quaternions) - NO SIMD planned
- **katla_vulkan** - Vulkan rendering layer with render graph system
- **katla_app** - Application framework, components, and systems
- **katla_ecs** - Custom Entity Component System framework
- **katla_derive** - Derive macros for the ECS (Component trait)

## Build and Test Commands

```bash
# Build
cargo build                    # Build all workspace crates
cargo build --release          # Release build
cargo check                    # Quick typecheck
cargo build -p katla_ecs       # Build specific package

# Test
cargo test                     # Run all tests
cargo test --workspace         # Explicit workspace tests
cargo test -p katla_ecs        # Test specific package
cargo test test_entity_id_creation  # Run single test
cargo test test_entity        # Run tests matching pattern
cargo test -- --nocapture      # Show stdout
cargo test --release           # Release mode tests

# Lint
cargo clippy                   # Linter
cargo clippy --fix             # Auto-fix
cargo fmt                      # Format
cargo fmt --check              # Check formatting
```

## Critical Architecture Rules

### Dependency Restrictions (Enforced Boundaries)

**CRITICAL**: These restrictions maintain clean module boundaries and prevent circular dependencies.

- **katla_vulkan** must NOT depend on: `katla_math`, `katla_ecs`, `katla_app`
- **katla_ecs** must NOT depend on: `katla_app`, `katla_vulkan`, `katla_math`
- **katla_math** must NOT depend on: `katla_app`, `katla_vulkan`, `katla_ecs`
- **katla_app** can depend on: `katla_vulkan`, `katla_ecs`, `katla_math`

### Ash Type Exclusion Rule

**CRITICAL**: `katla_vulkan` crate must NOT export or re-export `ash::vk` types in its public API.

- Create wrapper types for all Vulkan types (see `katla_vulkan/src/render_graph/types.rs` and `vulkan/vertexbuffer.rs` for `IndexType`)
- Use type aliases internally if needed, but never `pub use ash::vk`
- Downstream crates (katla_app) should NOT need to depend on ash directly
- Wrapper types should implement `From<Wrapper> for vk::Type` and `From<vk::Type> for Wrapper` for conversions
- Place wrapper types in the module where they're used (e.g., `IndexType` in `vertexbuffer.rs`) or in dedicated type modules (e.g., `render_graph/types.rs`)

**Note**: Some internal APIs still expose `vk::` types (e.g., `Framebuffer`, `Pipeline`, `CommandBuffer` methods). These should be wrapped when exposed in public APIs.

## ECS Architecture

### Core Data Structures

**SparseSet** (`katla_ecs/src/sparse_set.rs`):
- O(1) lookup, insert, remove operations
- Maintains contiguous `dense: Vec<(K, V)>` for iteration
- Uses `sparse: HashMap<K, usize>` for key→index mapping
- Excellent cache locality for iteration

**ComponentStorage** (`katla_ecs/src/storage.rs`):
- Wraps SparseSet<EntityId, Component>
- Each component type has separate storage
- Provides `iter()`, `iter_mut()`, `get()`, `get_mut()`

**ComponentStorageManager** (`katla_ecs/src/storage.rs`):
- HashMap storing `TypeId → Box<dyn Any>` for each component type
- Provides type-safe `add_component()`, `get_component()`, `get_storage()`
- Uses unsafe code internally for multi-borrow (sound due to TypeId uniqueness)

### Query System

**QueryData Trait** (`katla_ecs/src/query/`):
- Type-safe query API for component access
- Implemented for tuples: `&T`, `&mut T`, `(&T, &U)`, `(&mut T, &mut U)`, etc.
- Supports up to 3-component queries (iter1, iter2, iter3 modules)
- Automatically filters entities without all required components

**Query Usage**:
```rust
// Single component
for (entity, transform) in world.storage.query::<&mut TransformComponent>() {
    transform.position += Vec3::new(0.0, 1.0, 0.0);
}

// Multiple components
for (entity, vel, force) in world.storage.query::<(&mut Velocity, &Force)>() {
    vel.acceleration = force.value / vel.mass;
}
```

### System Pattern

**System Trait** (`katla_ecs/src/system.rs`):
```rust
pub trait System {
    fn update(&mut self, world: &mut World, delta_time: f32);
    fn initialize(&mut self) {}
    fn shutdown(&mut self) {}
    fn is_enabled(&self) -> bool { true }
    fn name(&self) -> &str { std::any::type_name::<Self>() }
}
```

**SystemExecutionOrder**: Controls execution order (FIRST, EARLY, NORMAL, LATE, LAST)

**World** (`katla_ecs/src/world.rs`):
- Central manager for entities, components, systems
- `entities: HashSet<EntityId>` - active entity IDs
- `storage: ComponentStorageManager` - all component storage
- `systems: Vec<OrderedSystem>` - registered systems
- `input_state: InputState` - global input state
- Methods: `create_entity()`, `add_component()`, `update()`, `register_system()`

### Component Pattern

Components use `#[derive(Component)]` from `katla_derive`:
```rust
#[derive(Component)]
pub struct TransformComponent {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}
```

## Render Graph Architecture

Located in `katla_vulkan/src/render_graph/`, this is a high-level abstraction for Vulkan rendering.

### Key Modules

- **types.rs** - Wrapper types for Vulkan (ImageFormat, ImageLayout, Extent2D/3D, etc.)
- **resource.rs** - ResourceKind (Buffer, Image, ExternalBuffer, ExternalImage), ResourceUsage
- **graph.rs** - RenderGraph builder and resource management
- **pass.rs** - PassBuilder, Pass, PassExecutionContext, ExecutionRegistry
- **compiled.rs** - CompiledRenderGraph, compilation pipeline (lifetime analysis, render pass generation)
- **builders.rs** - RenderGraphBuilder API

### Execution Flow

1. User builds graph with `RenderGraphBuilder::new()`
2. Add resources: `add_resource(name, ResourceKind)` → returns `ResourceId`
3. Add passes: `add_pass(name, |builder| { ... })`
4. Builder stores closures in `ExecutionRegistry` (avoids trait object lifetime issues)
5. `build(context)` → `CompiledRenderGraph` (transfers registry ownership)
6. `execute(command_buffer)` runs all passes with closure lookup

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

## Vulkan Wrapper Layer

Located in `katla_vulkan/src/vulkan/`, wraps raw ash calls with idiomatic Rust.

### Key Modules

- **context.rs** - VulkanContext (device, instance, physical device selection)
- **swapchain.rs** - Swapchain management
- **renderpass.rs** - RenderPass wrapper, `create_from_config()` for custom render passes
- **framebuffer.rs** - Framebuffer wrapper
- **commandbuffer.rs** - CommandBuffer wrapper, `pipeline_barrier()` for synchronization
- **texture.rs** - Texture loading and image creation
- **pipeline/** - Pipeline creation infrastructure

### VulkanRenderer

Main renderer struct in `katla_vulkan/src/lib.rs`:
- `context: Rc<VulkanContext>` - Vulkan context
- `frame_context: VulkanFrameCtx` - Per-frame data
- `render_pass: RenderPass` - Current render pass
- `swapchain_framebuffers: Vec<vk::Framebuffer>` - Per-swapchain image framebuffers
- `render_graph: Option<CompiledRenderGraph>` - Render graph (being integrated)

## Application Layer

Located in `katla_app/src/`, integrates ECS and Vulkan for the demo application.

### Key Modules

- **application/mod.rs** - Application struct (winit ApplicationHandler)
- **application/builder.rs** - ApplicationBuilder for setup
- **components/** - ECS components (Transform, Drawable, FlyCamera, etc.)
- **entities/** - Entity factories (Camera, ModelEntity)
- **systems/** - ECS systems (FlyCameraSystems)
- **rendering/** - Rendering abstractions (Drawable trait, mesh, material)
- **input/** - Input mapping and binding

### Application Flow

1. `Application::resumed()` - Create window, initialize VulkanRenderer
2. Load models (GLTF via FileCache)
3. Create entities with components
4. `window_event(RedrawRequested)` - Update world, render frame
5. Systems update via `world.update(delta_time)`

## Code Style

Follow patterns established in AGENTS.md:

- **Naming**: `StructName`, `function_name`, `CONSTANT_NAME`, `type_param T`
- **Tests**: Prefix with `test_` (`test_entity_id_creation`)
- **Imports**: Group std, external, internal; use `use crate::...` for internal
  ```rust
  // Import order: std lib, external crates, internal modules
  use std::collections::HashMap;
  use ash::vk;
  use katla_ecs::Component;
  use crate::components::Transform;
  ```
- **Line Length**: Keep lines under 100 characters where practical
- **Indentation**: 4-space indentation (Rust standard)
- **Error Handling**: `Option<T>`, `Result<T, E>`, avoid `unwrap()` in production
- **Documentation**: `///` for public APIs, `//!` for module-level
- **Visibility**: Use `pub(crate)` for internal APIs that are public within the crate
- **Performance**: Mark hot path functions with `#[inline]`, prefer stack allocation

## Common Patterns

### Creating a Component

```rust
use katla_ecs::Component;

#[derive(Component)]
pub struct MyComponent {
    pub value: f32,
}
```

### Creating a System

```rust
use katla_ecs::System;

struct MySystem;

impl System for MySystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        for (entity, comp) in world.storage.query::<&mut MyComponent>() {
            comp.value += delta_time;
        }
    }
}

// Register with order
world.register_system(Box::new(MySystem), SystemExecutionOrder::NORMAL);
```

### Render Graph Usage

```rust
let mut graph_builder = RenderGraphBuilder::new();

let color_target = graph_builder.add_resource(
    "color",
    ResourceKind::Image {
        extent: Extent3D { width: 1920, height: 1080, depth: 1 },
        format: ImageFormat::R8G8B8A8Srgb,
        usage: vec![ImageUsage::ColorAttachment],
        samples: SampleCount::Sample1,
        tiling: ImageTiling::Optimal,
        initial_layout: ImageLayout::Undefined,
        final_layout: ImageLayout::ShaderReadOnlyOptimal,
    },
);

graph_builder.add_pass("geometry_pass", |pass| {
    pass.write(color_target)
        .clear_color(color_target, [0.1, 0.1, 0.1, 1.0])
        .execute("geometry_pass", |ctx| {
            // Record commands
        });
});

let graph = graph_builder.build(&vulkan_context)?;
graph.execute(&mut command_buffer)?;
```

## Integration Notes

When integrating render graph with application layer:

- Use `ExternalImage` ResourceKind for swapchain images
- Capture world/matrices in closures via renderer fields (avoid nested lifetime issues)
- Call `renderer.render_frame()` in RedrawRequested handler
- Render graph currently in integration phase (see katla_vulkan/src/render_graph/Plan.md)
