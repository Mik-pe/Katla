TODO
---

## VulkanRenderer Cleanup (IN PROGRESS)

### Architectural Rules

#### katla_vulkan Responsibilities (RHI Layer)
- **GENERIC drawing primitives only** - doesn't know about "sky", "grid", "ui", "geometry"
- **NO application concepts** - just draws what it's given
- **NO ash::vk types in public API** - all Vulkan types must be wrapped
- **Provides infrastructure, not features** - pipelines, textures, buffers, render graphs
- **Clean primitives:**
  - `draw_draw_list()` - draws whatever meshes are in the list
  - `draw_fullscreen_with_material()` - draws a fullscreen quad
  - `draw_2d(...)` - draws 2D geometry (future)
  - `blit_images()` - copies images

#### katla_app Responsibilities (Application Layer)
- **Owns application concepts** - sky, grid, ui, geometry
- **Owns pipelines** - created from materials, passed to render graph
- **Defines render passes** - using the generic primitives from katla_vulkan
- **Sets draw lists** - tells katla_vulkan what to draw
- **NO direct Vulkan knowledge** - no ash::vk, no descriptor sets, no pipeline layouts

#### API Design Principles
1. **Application says WHAT to draw, not HOW**
   - ❌ `ctx.bind_descriptor_set(...)` - application knows about descriptors
   - ✅ `ctx.draw_fullscreen_with_material(&material)` - just draw it

2. **katla_vulkan is a tool, not a framework**
   - ❌ `ctx.draw_geometry()` - implies katla_vulkan knows what "geometry" is
   - ✅ `ctx.draw_draw_list()` - draws whatever is in the list

3. **Clear ownership boundaries**
   - katla_vulkan: GPU resources (buffers, textures, pipelines)
   - katla_app: Game concepts (entities, components, draw lists)

### Completed ✅
- [x] Added `PassExecutionContext` wrapper methods for cleaner API
- [x] Created `FrameResources` and `RenderTarget` abstractions
- [x] Added `PassBuilder` convenience methods
- [x] Added new render graph API
- [x] Made AssetRegistry methods public
- [x] Made `vk_layout()`, `vk_set()` methods public
- [x] Created `render_graph` module in katla_app
- [x] Removed pipeline storage from VulkanRenderer
- [x] Added `draw_fullscreen_with_material()` - generic fullscreen drawing
- [x] Added `draw_draw_list()` - generic mesh drawing from list
- [x] Added `draw_ui()` - 2D UI drawing helper
- [x] Removed `ash::vk` usage from application render_graph.rs
- [x] Created `RendererContext` with raw pointers for GPU resources
- [x] Added `compile_render_graph()` that sets up RendererContext

### Remaining Work

#### Phase 2: Clean Up RendererContext
- [ ] Review RendererContext fields - remove application-specific stuff
- [ ] RendererContext should only have GENERIC renderer state:
  - asset_registry, draw_list, storage_manager, bindless_manager
  - NOT: ui_data, ui_buffers, ui_textures, sky_pipeline, etc.
- [ ] Move UI-specific rendering to application layer

#### Phase 3: Fix Pre-existing Issues
- [ ] Fix image layout transitions in present pass
- [ ] Fix descriptor set binding issues
- [ ] Unified "RenderTarget" concept (single type for swapchain/viewport/texture)

## Current State

- ✅ Infrastructure for new API is in place
- ✅ Application uses clean API without ash::vk
- ✅ Pipeline ownership moved to application
- ✅ Legacy render graph code removed from VulkanRenderer
- ⚠️ RendererContext still has UI-specific fields (needs cleanup)
- ⚠️ Pre-existing validation errors remain
