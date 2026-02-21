# ViewportBuilder Refactor Plan

## Goal

Unify main scene and preview rendering into a single, configurable `ViewportBuilder` API that handles:
- Render target creation
- Layout transitions (automatic)
- Camera/storage uniform management
- Output mode (offscreen vs direct to swapchain)

## API Design

```rust
// === PUBLIC TYPES ===

/// Output mode for viewport
pub enum OutputMode {
    /// Render to texture (UI will sample it) - Editor mode
    Offscreen,
    /// Render directly to swapchain - Game mode
    DirectToSwapchain,
}

/// Depth buffer format
pub enum DepthFormat {
    None,
    D32Sfloat,
    D32SfloatS8Uint,
}

/// Builder for creating viewports
pub struct ViewportBuilder {
    width: u32,
    height: u32,
    depth_format: DepthFormat,
    color_format: ImageFormat,
    output_mode: OutputMode,
    clear_color: [f32; 4],
    label: String,
}

/// Opaque handle to a viewport
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewportHandle(usize);

// === USAGE ===

// Main viewport (editor: offscreen, game: direct to swapchain)
let main = renderer.create_viewport()
    .size(window_width, window_height)
    .with_depth(DepthFormat::D32SfloatS8Uint)
    .output_mode(OutputMode::Offscreen)
    .label("main")
    .build()?;

// Preview viewport (always offscreen)
let preview = renderer.create_viewport()
    .size(512, 512)
    .with_depth(DepthFormat::D32SfloatS8Uint)
    .output_mode(OutputMode::Offscreen)
    .label("preview")
    .build()?;

// Render to viewport
renderer.render_viewport(main_handle, &camera, &draw_list);
renderer.render_viewport(preview_handle, &preview_camera, &preview_draw_list);

// Get texture for UI sampling
ui.image(renderer.viewport_texture(main_handle));
ui.image(renderer.viewport_texture(preview_handle));
```

## Implementation Steps

### Phase 1: Core Types and Viewport Struct

1. Create `ViewportBuilder` struct with builder pattern
2. Create `Viewport` struct to hold internal state:
   - Color/depth images and views
   - Compiled render graph
   - Storage uniform manager (camera)
   - Output mode
   - Label for debugging

### Phase 2: ViewportManager

3. Add `ViewportManager` to `VulkanRenderer`:
   - `viewports: Vec<Viewport>`
   - `create_viewport() -> ViewportBuilder`
   - `render_viewport(handle, camera, draw_list)`
   - `viewport_texture(handle) -> Option<TextureId>`
   - `destroy_viewport(handle)`

### Phase 3: Barrier Helpers

4. Create reusable barrier functions in `sync.rs`:
   - `transition_color_to_attachment(image) -> ImageMemoryBarrier2`
   - `transition_color_to_shader_read(image) -> ImageMemoryBarrier2`
   - `transition_depth_sync(image) -> ImageMemoryBarrier2`

### Phase 4: Migrate Preview

5. Replace preview-specific code with ViewportBuilder:
   - Remove `init_preview_target()`
   - Remove `init_preview_storage()`
   - Remove `setup_preview_render_graph()`
   - Remove `register_preview_texture()`
   - Use viewport handle instead

### Phase 5: Migrate Main Scene

6. Replace main scene code with ViewportBuilder:
   - Remove `init_viewport_target()`
   - Remove `init_output_target()`
   - Consolidate `setup_render_graph()`

### Phase 6: Cleanup

7. Remove deprecated methods
8. Update application layer
9. Add documentation

## Internal Architecture

```
VulkanRenderer
├── context: Rc<VulkanContext>
├── swapchain: Swapchain (handles frames-in-flight)
├── viewports: Vec<Viewport>
│   ├── Viewport (main)
│   │   ├── color_image, color_view
│   │   ├── depth_image, depth_view
│   │   ├── render_graph: CompiledRenderGraph
│   │   ├── storage_manager: StorageUniformManager
│   │   └── output_mode: OutputMode
│   └── Viewport (preview)
│       └── ...
└── ui_system: UITextures
```

## Layout Transition Flow (Automatic)

```
Frame Start
    │
    ▼
┌─────────────────────────────────┐
│ For each viewport:              │
│   transition_to_attachment()    │  ← SHADER_READ_ONLY → COLOR_ATTACHMENT
│   execute_render_graph()        │
│   transition_to_shader_read()   │  ← COLOR_ATTACHMENT → SHADER_READ_ONLY
└─────────────────────────────────┘
    │
    ▼
UI renders (samples viewport textures)
    │
    ▼
Present to swapchain
```

## Files to Modify

- `katla_vulkan/src/lib.rs` - Main renderer, add ViewportManager
- `katla_vulkan/src/sync.rs` - Add barrier helper functions
- `katla_vulkan/src/viewport.rs` - NEW: Viewport, ViewportBuilder, ViewportHandle
- `katla_app/src/application/renderer/mod.rs` - Update to use ViewportBuilder

## Backward Compatibility

- Keep old methods as `#[deprecated]` initially
- Remove after migration is complete
