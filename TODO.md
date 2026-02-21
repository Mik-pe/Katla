TODO
---

## VulkanRenderer Cleanup (IN PROGRESS)

### Completed ✅
- [x] Added `PassExecutionContext` wrapper methods for cleaner API
  - `bind_graphics_pipeline()`, `bind_graphics_pipeline_with_descriptors()`
  - `bind_index_buffer()`, `bind_vertex_buffers()`
  - `draw_indexed()`, `draw_array()`, `draw_fullscreen()`
- [x] Created `FrameResources` and `RenderTarget` abstractions
  - Opaque handles for resources (no ResourceId exposure)
  - Semantic names (swapchain, viewport_color, viewport_depth, output_color)
- [x] Added `PassBuilder` convenience methods
  - `write_color()`, `write_depth()` - accept `&RenderTarget`
  - `clear_color_target()`, `clear_depth_target()`
  - `blit()` - for present pass setup
- [x] Added new render graph API
  - `create_render_graph_with_resources()` - builder with pre-registered resources
  - `compile_render_graph()` - compile a builder
- [x] Made AssetRegistry methods public
  - `get_mesh()`, `get_material()`, `get_material_mut()`
  - `MeshAsset`, `MaterialAsset` structs now public
- [x] Made `vk_layout()`, `vk_set()` methods public
  - `MaterialPipeline::vk_layout()`
  - `StorageDescriptorSet::vk_set()`
  - `SkeletonDescriptorSet::vk_set()`
  - `UITextures::vk_set()`
- [x] Created `render_graph` module in katla_app
- [x] Removed pipeline storage from VulkanRenderer
  - Removed `sky_pipeline`, `grid_pipeline`, `ui_pipeline` fields
  - Removed `set_sky()`, `set_grid()`, `set_ui()` methods
  - `setup_render_graph()` passes pipelines to render graph builder

### Remaining Work

#### Phase 1: Migrate application to new render graph API
- [ ] Add proper `get_renderer_context()` method that works with Rc<RefCell<>> types
- [ ] Make `UiDrawData`, `UIBuffers`, `UITextures` Clone or use Rc<RefCell<>>
- [ ] Update `render_graph.rs` to use new API instead of legacy `setup_render_graph()`
- [ ] Test that everything still renders

#### Phase 2: Remove legacy render graph code
- [ ] Remove `rebuild_render_graph_internal()` method
- [ ] Remove `setup_render_graph()` method
- [ ] All render graph building happens in application layer

#### Phase 3: Fix pre-existing issues
- [ ] Fix image layout transitions in present pass
- [ ] Fix descriptor set binding issues
- [ ] Unified "RenderTarget" concept (single type for swapchain/viewport/texture)

## Architecture Goals

The VulkanRenderer should be GENERIC:
- Holds HIGH-LEVEL constructs or PRIVATE objects for book-keeping
- Supports multiple rendergraphs outputting to swapchain or viewports
- Does NOT hold application-specific pipelines (sky, grid, ui)
- Provides infrastructure, not feature implementations

The Application layer should:
- Own pipelines (sky, grid, ui, materials)
- Define passes via render graph API
- Call renderer.compile_render_graph(builder)

## Current State

- ✅ Infrastructure for new API is in place
- ✅ Application builds and runs
- ✅ Pipeline ownership moved to application (passed to setup_render_graph)
- ⚠️ Application still uses legacy `setup_render_graph()` API internally
- ⚠️ Pre-existing validation errors remain

## Pre-existing Issues

- Preview system is too specific (preview_render_graph, preview_storage_manager, etc.)
- Output vs Viewport RenderTarget should use unified type
