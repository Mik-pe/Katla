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
- [x] Added `bindless_manager` field to `RendererContext`
- [x] Created `render_graph` module in katla_app (compatibility layer)

### Remaining Work
- [ ] Make `AssetRegistry::get_mesh()` and `get_material_mut()` public
  - Or add wrapper methods that return safe types
- [ ] Make `vk_layout()`, `vk_set()` methods public
  - Or add wrapper methods for descriptor binding
- [ ] Complete the application-layer render graph building
  - Fix `render_graph.rs` to use public APIs
  - Remove dependency on internal `vk::` types
- [ ] Remove application-specific code from VulkanRenderer
  - Remove `sky_pipeline`, `grid_pipeline`, `ui_pipeline` fields
  - Remove `set_sky()`, `set_grid()`, `set_ui()` methods
  - Remove `rebuild_render_graph_internal()` method
- [ ] Fix pre-existing validation errors
  - Image layout transitions in present pass
  - Descriptor set binding issues
- [ ] Unified "RenderTarget" concept
  - Single type that can output to swapchain or texture/viewport
  - Remove OutputRenderTarget vs ViewportRenderTarget distinction

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

## Pre-existing Issues

- Preview system is too specific (preview_render_graph, preview_storage_manager, etc.)
- Output vs Viewport RenderTarget should use unified type
