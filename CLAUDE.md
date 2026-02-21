# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Katla is a Vulkan-based 3D render engine written in Rust, using ECS (Entity Component System) architecture. The project is structured as a Cargo workspace with multiple crates:

- **katla_math** - Custom math library (vectors, matrices, quaternions) - SIMD planned (see katla_math/PLAN.md)
- **katla_vulkan** - Vulkan rendering layer with render graph system
- **katla_app** - Application framework, components, and systems
- **katla_ecs** - Custom Entity Component System framework
- **katla_derive** - Derive macros for the ECS (Component trait)
- **katla_ui** - Immediate mode UI system for debug overlays and in-game HUDs

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

# Run
cargo run                      # Run the application
cargo run -- -s               # Run in limited-frame mode (25 frames) for validation
cargo run -- --single-frame    # Same as above, long form
```

## Command Line Arguments

- `-s, --single-frame` - Run in limited-frame mode (25 frames) for validation testing. Useful for checking Vulkan validation errors without running indefinitely.

## Working Conventions

- **Task Continuity**: When working with tasks, continue through the task list without asking for confirmation between tasks. If there are pending tasks, proceed to the next one automatically.

## Git Commit Conventions

### Commit Message Format

Follow a consistent format for commits:

```
Summary line (50-72 chars, imperative mood)

- Optional detailed bullet points
- Each line starts with a hyphen
- Describe WHAT was done, not WHY
- Keep it concise and focused
```

**Examples:**
```
Add animation system with skeletal and transform-based animation

- AnimationPlayer component with play/pause/loop/seek controls
- AnimatedModel, JointTransform, MorphTargetWeights components
- AnimationClip, AnimationChannel, AnimationSampler structures
- AnimationUpdateSystem, SkeletalAnimationSystem, MorphTargetSystem
```

```
Fix transform hierarchy parent-child propagation

- Correct multiplication order (parent * child, not child * parent)
- Add topological sort for proper update ordering
- Fix rotation application to child's position
```

### Commit Guidelines

1. **Test before committing**: Run `cargo test` to ensure all tests pass
2. **Keep commits focused**: One logical change per commit
3. **Write clear summaries**: Use imperative mood ("Add", "Fix", "Refactor")
4. **Include details**: List major files, components, or features added
5. **No Co-Authored-By**: Do not include AI co-authorship tags
6. **Avoid "Update":** Be specific about what was updated

### Commit Workflow

```bash
# Check status
git status

# Stage relevant files
git add path/to/files

# Review changes
git diff --staged

# Commit with message
git commit -m "Summary line

- Detail one
- Detail two
- Detail three"
```

### When to Commit

- After completing a feature or fix
- After adding comprehensive tests
- When code compiles and all tests pass
- Before major refactors
- After documentation updates

## Critical Architecture Rules

### Dependency Restrictions (Enforced Boundaries)

**CRITICAL**: These restrictions maintain clean module boundaries and prevent circular dependencies.

```
katla_app ─────────────────────────────────────────────────────┐
    │                                                          │
    ├── katla_vulkan ──┐                                       │
    │                  │                                       │
    ├── katla_ecs ─────┼──> NO external dependencies           │
    │                  │                                       │
    ├── katla_ui ──────┘                                       │
    │                                                          │
    └── katla_math ────────────> NO crate dependencies        │
                                                               │
katla_derive ──────────────────> proc-macro crate (isolated)  │
                                                               │
game ──────────────────────────> Application (depends on all) ◄┘
```

**Rules:**
- **katla_vulkan** must NOT depend on: `katla_math`, `katla_ecs`, `katla_app`, `katla_ui`
- **katla_ecs** must NOT depend on: `katla_app`, `katla_vulkan`, `katla_math`, `katla_ui`
- **katla_math** must NOT depend on: ANY other crate
- **katla_ui** must NOT depend on: `katla_ecs`, `katla_app`
- **katla_ui** CAN depend on: `katla_math`, `katla_vulkan`
- **katla_app** can depend on: `katla_vulkan`, `katla_ecs`, `katla_math`, `katla_ui`

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
- **frame_resources.rs** - FrameResources struct with pre-registered render targets

### Execution Flow

1. Get builder with `renderer.create_render_graph_with_resources()` → returns `(RenderGraphBuilder, FrameResources)`
2. Add passes: `builder.add_pass(name, |pass| { ... })`
3. Each pass declares what it reads/writes via `pass.write_color()`, `pass.blit()`, etc.
4. Compile with `renderer.compile_render_graph(builder, swapchain_resource_id)`
5. Each frame, `render_frame()` executes the compiled graph

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

## Modern Vulkan 1.3 Rendering

**Status**: ✅ The Katla engine uses modern Vulkan 1.3 (2026) rendering patterns.

### Key Modern Features in Use

- **Dynamic Rendering (VK_KHR_dynamic_rendering)** - Production rendering uses `vkCmdBeginRendering`/`vkCmdEndRendering` instead of legacy render passes
- **Synchronization2 (VK_KHR_synchronization2)** - All pipeline barriers use `vkCmdPipelineBarrier2` with modern barrier types
- **VMA Integration** - Uses `gpu_allocator` for Vulkan Memory Allocator integration
- **Frames In-Flight** - Proper per-frame synchronization with fences and semaphores
- **Bindless Textures** - Single texture array descriptor instead of per-texture descriptors

### Synchronization Pattern

Use **Synchronization2** for all barriers:

```rust
// Modern Synchronization2 barrier pattern
let barrier = ImageMemoryBarrier2::new(image)
    .src_stage(PipelineStage2Flags::TOP_OF_PIPE)
    .src_access(AccessFlags2::NONE)
    .dst_stage(PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT)
    .dst_access(AccessFlags2::COLOR_ATTACHMENT_WRITE)
    .old_layout(vk::ImageLayout::UNDEFINED)
    .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
    .subresource_range(vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    });

DependencyInfo::new()
    .add_image_barrier(barrier)
    .build(|dep_info| unsafe {
        context.device.cmd_pipeline_barrier2(command_buffer, dep_info);
    });
```

**Do NOT use** legacy barrier pattern:
```rust
// ❌ LEGACY - DO NOT USE
context.device.cmd_pipeline_barrier(
    command_buffer,
    src_stage_mask,
    dst_stage_mask,
    vk::DependencyFlags::empty(),
    &memory_barriers,
    &buffer_barriers,
    &image_barriers,
);
```

## Vulkan Wrapper Layer

Located in `katla_vulkan/src/vulkan/`, wraps raw ash calls with idiomatic Rust.

### Key Modules

- **context.rs** - VulkanContext (device, instance, physical device selection)
- **swapchain.rs** - Swapchain management
- **framebuffer.rs** - Framebuffer wrapper (minimal usage with dynamic rendering)
- **commandbuffer.rs** - CommandBuffer wrapper with `begin_rendering()`/`end_rendering()` for dynamic rendering
- **texture.rs** - Texture loading and image creation (uses Synchronization2)
- **pipeline/** - Pipeline creation infrastructure
- **material/** - Material system with hot reload support
- **bda.rs** - `DeviceAddressBuffer` type with BDA flag + persistent mapping (used via descriptors, not push constants)
- **particle_buffer.rs** - GPU particle buffers using `DeviceAddressBuffer`

### VulkanRenderer

Main renderer struct in `katla_vulkan/src/lib.rs`:
- `context: Rc<VulkanContext>` - Vulkan context
- `frame_context: VulkanFrameCtx` - Per-frame data
- `swap_data: SwapData` - Swapchain synchronization (semaphores, fences)
- `asset_registry: AssetRegistry` - GPU asset management (meshes, materials)
- `material_registry: MaterialRegistry` - Template-based materials with hot reload

### Rendering Flow

1. **Frame Acquisition**: `swap_frames()` acquires next swapchain image
2. **Render Graph Execution**: `render_frame()` executes the compiled render graph with dynamic rendering
3. **Synchronization**: Proper semaphores/fences for frames-in-flight
4. **Presentation**: Queue present to swapchain

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
- **Logging**: Use appropriate log levels (see Logging Guidelines below)

## Logging Guidelines

Use the `log` crate with appropriate levels to balance visibility vs noise:

### Log Levels

| Level | Use For | Examples |
|-------|---------|----------|
| `error!` | Unrecoverable errors, critical failures | GPU device lost, failed to create swapchain |
| `warn!` | Recoverable issues, missing optional data | No normals found, template not found using fallback |
| `info!` | Major lifecycle events, user-visible actions | Window resized, model loaded, hot reload enabled |
| `debug!` | Detailed diagnostic info | Parsed X vertices, shader reloaded, entity spawned |

### What Goes Where

**INFO level** (default, visible in normal runs):
- Application startup/shutdown
- Window resize events
- Model/resource loading completed
- Hot reload enabled/disabled
- User-initiated editor actions (delete, select, spawn)
- Frame count on exit (validation mode)

**DEBUG level** (use `RUST_LOG=debug` to see):
- Detailed parsing info (vertex counts, joint counts, etc.)
- Per-component attribute parsing
- Shader file matching and hot reload details
- Animation/skin parsing details
- Skeleton buffer creation
- Mesh creation timing
- Internal state changes

**WARN level**:
- Missing optional data (normals, templates)
- Fallback behavior activated
- Non-critical failures (failed to register skeleton)

### Example

```rust
use log::{debug, info, warn};

// Startup - INFO (user cares)
info!("Loading model from {}", path.display());

// Parsing details - DEBUG (developer debugging)
debug!("Parsed {} vertices, {} indices", vertices.len(), indices.len());

// Missing data with fallback - WARN
warn!("Mesh has no normals, generating smooth normals from geometry");
```

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

The render graph uses **dynamic rendering** by default. Passes execute with `vkCmdBeginRendering`/`vkCmdEndRendering`.

**Architecture:** Scene passes render to `viewport_color`, UI renders to `output_color`. The `present_pass` blits `output_color` to the swapchain.

```rust
// Get builder with pre-registered resources (swapchain, viewport, output)
let (mut builder, resources) = renderer.create_render_graph_with_resources();

// Sky pass - fullscreen quad with sky material
builder.add_pass("sky_pass", |pass| {
    pass.write_color(&resources.viewport_color)
        .write_depth(&resources.viewport_depth)
        .clear_color_target(&resources.viewport_color, [0.4, 0.6, 0.9, 1.0])
        .clear_depth_target(&resources.viewport_depth, 1.0)
        .execute("sky_pass", |ctx| {
            ctx.draw_fullscreen_with_material(&sky_pipeline);
        });
});

// Geometry pass - draw meshes from draw list
builder.add_pass("geometry_pass", |pass| {
    pass.write_color(&resources.viewport_color)
        .write_depth(&resources.viewport_depth)
        .execute("geometry_pass", |ctx| {
            ctx.draw_draw_list();
        });
});

// UI pass - 2D overlay
builder.add_pass("ui_pass", |pass| {
    pass.write_color(&resources.output_color)
        .execute("ui_pass", |ctx| {
            ctx.draw_ui(&ui_pipeline);
        });
});

// Present pass - blit output to swapchain
builder.add_pass("present_pass", |pass| {
    pass.blit(&resources.output_color, &resources.swapchain)
        .execute("present_pass", |ctx| {
            if let (Some((src_img, _)), Some((dst_img, _))) = (
                ctx.get_image(resources.output_color.resource_id()),
                ctx.get_image(resources.swapchain.resource_id()),
            ) {
                let (width, height) = ctx.extent();
                ctx.blit_images(src_img, dst_img, width, height);
            }
        });
});

// Compile with swapchain resource ID for proper layout transitions
renderer.compile_render_graph(builder, Some(resources.swapchain.resource_id()))?;
```

**FrameResources (pre-registered by VulkanRenderer):**
- `swapchain` - Current swapchain image (changes each frame)
- `viewport_color` - Offscreen render target for 3D scene
- `viewport_depth` - Depth buffer for viewport
- `output_color` - Final composition target (scene + UI)

**Key Points:**
- Use `create_render_graph_with_resources()` for pre-registered targets
- Use `write_color()`/`write_depth()` for render targets
- Use `blit()` for transfer operations between images
- The graph automatically uses dynamic rendering (no traditional render passes)
- Synchronization2 barriers inserted automatically for layout transitions

## Integration Notes

When integrating render graph with application layer:

- **Dynamic rendering is automatic** - passes use `vkCmdBeginRendering`/`vkCmdEndRendering`
- **Synchronization is automatic** - barriers inserted for layout transitions using Synchronization2
- **No framebuffer management** - dynamic rendering doesn't require traditional framebuffers
- **Hot reload support** - materials can be reloaded at runtime via `MaterialRegistry`

### Material System

Materials use template-based configuration with hot reload:

- **TOML-based material definitions** - define shaders, textures, parameters
- **No render pass dependency** - materials work with dynamic rendering
- **Per-material uniforms** - optional uniform buffers for material parameters
- **Hot reload** - modify TOML files and reload at runtime

## UI Popup System

Katla's UI uses a closure-based popup API with automatic layout and sizing.

### Basic Usage

```rust
use katla_ui::icons::ForkAwesome;

// Simple popup with items
let action = ui.popup("my_popup", |ui| {
    if ui.popup_item("Open", ForkAwesome::FOLDER_OPEN, true) { return Some("open"); }
    if ui.popup_item("Rename", ForkAwesome::PENCIL, true) { return Some("rename"); }
    ui.popup_separator();
    if ui.popup_item("Delete", ForkAwesome::TRASH, true) { return Some("delete"); }
    None
});

if let Some(action) = action.flatten() {
    // Handle action
}
```

### Opening a Popup

```rust
// Open at mouse position (call on right-click detection)
if ui.input.mouse_clicked(mouse_button::RIGHT) && is_hovered {
    ui.open_context_menu_at("my_popup", ui.input.mouse_pos);
}
```

### Key Principles

1. **Closure-based API** - `popup(id, |ui| { ... })` encapsulates popup content
2. **Automatic layout** - Items position vertically, no manual `current_y` tracking
3. **Automatic sizing** - Background fits content exactly (width + height)
4. **Return values** - Closures can return clicked actions for clean handling

### Available Methods

| Method | Description |
|--------|-------------|
| `popup(id, f)` | Show popup with closure, returns `Option<R>` |
| `popup_item(label, icon, enabled)` | Draw menu item, returns true if clicked |
| `popup_item_with_shortcut(label, icon, enabled, shortcut)` | Same with keyboard hint |
| `popup_separator()` | Draw separator line |
| `open_context_menu_at(id, pos)` | Open popup at position |

### Common ForkAwesome Icons

```rust
use katla_ui::icons::ForkAwesome;

ForkAwesome::FOLDER        // Folder icon
ForkAwesome::FOLDER_OPEN   // Open folder
ForkAwesome::FILE          // File icon
ForkAwesome::PENCIL        // Edit/rename
ForkAwesome::TRASH         // Delete
ForkAwesome::COPY          // Copy/duplicate
ForkAwesome::REFRESH       // Refresh
ForkAwesome::EXTERNAL_LINK // Open in explorer
```

## Shader System (WGSL + naga)

Katla uses **WGSL shaders** compiled to SPIR-V via the **naga library** (not the naga CLI binary).

### Compilation Pipeline

```
WGSL source (.wgsl files)
    ↓
naga::front::wgsl::parse_str()  [in shadermodule.rs]
    ↓
naga::back::spv::write_vec()    [generates SPIR-V]
    ↓
vk::ShaderModule
```

### Key Files

- `katla_vulkan/src/vulkan/material/shadermodule.rs` - `ShaderModule::from_wgsl()` and `from_wgsl_string()`
- `katla_vulkan/src/vulkan/material/reflection.rs` - naga-based shader reflection for uniform layouts
- `resources/shaders/*.wgsl` - All shader source files 
- `resources/materials/*.toml` - All material source files (references shaders)

### Example Shader Structure

```wgsl
// Uniform buffer binding (set 0, binding 0 typically)
struct FrameUniforms {
    view_proj: mat4x4<f32>,
    camera_position: vec3<f32>,
    _pad: f32,
}

@group(0) @binding(0)
var<uniform> frame: FrameUniforms;

// Storage buffer binding (for particle systems, etc.)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // ...
}
```

### Why naga Library (not CLI)?

- **Hot reload support** - Can recompile shaders at runtime without external process
- **Reflection** - naga parses shader structure for automatic uniform buffer layout
- **Error messages** - Better integration with Rust error handling
- **No build step** - Shaders compile on load, no separate SPIR-V generation needed

## Matrix Layout and GLTF Parsing

### Matrix Storage Convention

Katla's `Mat4` uses **column-major** storage: `Mat4.0[i]` is the i-th column as a `Vec4`.

```rust
// Identity matrix - each Vec4 is a COLUMN
Mat4([
    Vec4::new(1.0, 0.0, 0.0, 0.0),  // Column 0
    Vec4::new(0.0, 1.0, 0.0, 0.0),  // Column 1
    Vec4::new(0.0, 0.0, 1.0, 0.0),  // Column 2
    Vec4::new(tx, ty, tz, 1.0),      // Column 3 (translation)
])
```

### GLTF Matrix Parsing

GLTF stores matrices as 16 consecutive floats in **column-major order**. When parsing:
- bytes[0..15] = column 0
- bytes[16..31] = column 1
- etc.

**CRITICAL**: Do NOT transpose when reading! The data is already column-major.

### Common Pitfall: "Values Look Fine But Animation Breaks"

When matrices are transposed:
- Translations end up in the wrong column → bones appear at origin
- Rotations get corrupted → vertices stretch to infinity
- Values are technically "finite and reasonable" → tests pass but animation fails

**Testing Tip**: Check that inverse bind matrices have **non-zero translations**. If all IBMs show `(0, 0, 0)` translation, the matrices are likely transposed.

## Code Review Guidelines

### Performance Patterns

1. **Make small structs `Copy`** - If a struct only contains primitives, derive `Copy` to eliminate clone overhead in hot paths (e.g., `CachedGlyph` for text rendering)

2. **Avoid `.clone()` before iteration** - Use `for &x in &collection` instead of `for x in collection.clone()`

3. **Use helper functions for repeated patterns** - If you draw 4 border rects in multiple places, create `draw_selection_border()` helper

4. **Prefer macros for repetitive struct initialization** - Theme definitions reduced 41% by using a macro

### RHI Abstraction Principles

The katla_vulkan crate should maintain proper RHI (Render Hardware Interface) abstraction:

1. **No raw `ash::vk` types in public API** - All Vulkan types must be wrapped
2. **`vk()` methods should be `pub(crate)`** - Internal access only
3. **Opaque handles for resources** - Use `MeshHandle(usize)` not `&Mesh`
4. **Consistent abstraction levels** - High-level (DrawCall), Mid-level (RenderGraph), Low-level (Context)

See `.claude/skills/vulkan-rhi-validator/` for detailed guidelines.

### Common Code Smells

| Smell | Fix |
|-------|-----|
| 25+ fields in a struct | Split into focused components |
| `Rc<RefCell<>>` everywhere | Consider explicit ownership |
| Panics in public API | Return `Result<T, E>` |
| Magic numbers | Extract to named constants |
| Duplicate code blocks | Extract helper functions |
