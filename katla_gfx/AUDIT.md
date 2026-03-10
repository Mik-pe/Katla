# VulkanRenderer Public API Audit

**Date:** 2025-03-10
**Auditor:** Giga Maintainer (GFX + APP hybrid perspective)
**Purpose:** Identify API surface bloat and design issues before refactoring

---

## 📊 EXECUTIVE SUMMARY

| Metric | Count | Target | Status |
|--------|-------|--------|--------|
| Total VulkanRenderer methods | **50+** | < 30 | 🔴 CRITICAL |
| Material creation methods | 5 | 1 | 🔴 CRITICAL |
| Texture creation methods | 6 | 2-3 | 🔴 CRITICAL |
| Mesh creation methods | 10 | 1-2 | 🔴 CRITICAL |
| UI-specific methods | 4 | 0 | 🔴 CRITICAL |
| Viewport methods | 12 | ? | ⚠️ REVIEW |

**Key Finding:** `VulkanRenderer` is doing too much. It's a texture manager, mesh factory, material compiler, viewport system, UI renderer, AND a graphics API. Each concern should be extracted.

---

## 🔍 CATEGORY 1: INITIALIZATION & LIFECYCLE (6 methods)

**Status:** ✅ CORE - Should stay in renderer

| Method | Purpose | Verdict |
|--------|---------|---------|
| `init()` | Create renderer from window | KEEP |
| `destroy()` | Cleanup Vulkan resources | KEEP |
| `wait_for_device()` | Sync GPU operations | KEEP |
| `recreate_swapchain()` | Handle window resize | KEEP |
| `num_images()` | Get swapchain image count | KEEP |
| `context()` | Access Vulkan context | KEEP |

**GFX:** These are core renderer responsibilities. Vulkan-native, minimal abstraction.

---

## 🔍 CATEGORY 2: TEXTURE CREATION (7 methods)

**Status:** 🔴 BLOATED - Should be 2-3 methods max

| Method | Purpose | Verdict |
|--------|---------|---------|
| `create_texture()` | Generic texture creation | KEEP (core) |
| `create_texture_rgba()` | RGBA texture convenience | WRAPPER |
| `create_texture_unorm()` | UNORM texture convenience | WRAPPER |
| `create_texture_solid()` | Solid color texture | WRAPPER |
| `create_texture_from_rgb()` | RGB to RGBA conversion | WRAPPER |
| `create_texture_empty()` | Empty texture | WRAPPER |
| `default_texture()` | Get default white texture | KEEP |

**GFX:** Why are there 6 ways to create a texture? These should all be `create_texture()` with different `TextureDescriptor` configs.

**APP:** I don't know which one to use. `create_texture()` takes a descriptor, but then there are 5 other methods that are "easier"? Which should I use?

**Giga Recommendation:**
```rust
// Keep these two:
let tex = renderer.create_texture(&TextureDescriptor {
    format: TextureFormat::RGBA8,
    width, height, data
})?;

let tex = renderer.create_texture_solid([255, 0, 0, 255])?; // Common case

// Remove everything else - use TextureDescriptor
```

---

## 🔍 CATEGORY 3: UI-SPECIFIC METHODS (4 methods)

**Status:** 🔴 SHOULD NOT EXIST - UI should use generic APIs

| Method | Purpose | Verdict |
|--------|---------|---------|
| `create_ui_font_atlas()` | Upload font texture | EXTRACT |
| `update_ui_font_atlas()` | Update font texture | EXTRACT |
| `ui_font_atlas()` | Get font atlas handle | EXTRACT |
| `create_ui_material()` | Create UI shader material | MERGE |

**GFX:** These don't belong on the renderer. Font atlas management is application-level concern.

**APP:** I want to render UI, so I call these methods. But why are they renderer methods? Shouldn't there be a `UIFontSystem` or something?

**Giga Recommendation:**
- Move font atlas management to `katla_ui` or new `katla_app::ui::FontAtlas` type
- `create_ui_material()` → merge into single `compile_material()` API

---

## 🔍 CATEGORY 4: MATERIAL CREATION (5 methods)

**Status:** ✅ IMPROVED - Now has unified `compile_material()` + thin wrappers

| Method | Purpose | Verdict |
|--------|---------|---------|
| `compile_material()` | **NEW** - Single unified material creation API | ✅ CORE |
| `create_pbr_material()` | Create PBR material with defaults | ✅ WRAPPER (calls compile_material) |
| `create_ui_material()` | Create UI material | ✅ WRAPPER (calls compile_material) |
| `compile_fullscreen_shader()` | Fullscreen post-processing material | ⚠️ SEPARATE (returns PipelineHandle) |
| `compile_fullscreen_shader_with_format()` | Fullscreen with format override | ⚠️ SEPARATE (returns PipelineHandle) |
| `material_builder()` | Get MaterialBuilder for custom materials | ✅ WRAPPER (calls compile_material) |

**GFX:** This is the biggest API smell. All of these are wrappers around the same compilation logic with different defaults. Having 5 methods means 5 code paths to maintain.

**APP:** I'm confused. Which one do I use for a custom shader? What format does `compile_fullscreen_shader` use? The method names don't tell me.

**Giga Recommendation:**
```rust
// Single method that does everything:
let material = renderer.compile_material("shader.wgsl", MaterialOptions {
    vertex_type: VertexType::Ui,
    color_format: ImageFormat::R16G16B16A16Sfloat,
    alpha_blended: true,
    ..Default::default()
})?;

// Thin convenience wrappers (optional, can be helper functions):
let material = renderer.compile_pbr_material("shader.wgsl")?;
let material = renderer.compile_ui_material("shader.wgsl")?;
```

**Action:** This is task #4 - highest priority refactor.

---

## 🔍 CATEGORY 5: MESH CREATION (10+ methods)

**Status:** 🔴 WAY TOO MANY - These belong in a `MeshBuilder` or primitive utils

| Method | Purpose | Verdict |
|--------|---------|---------|
| `create_mesh()` | Generic mesh creation | KEEP (core) |
| `register_mesh()` | Register existing mesh asset | KEEP (core) |
| `create_cube_mesh()` | Create cube primitive | EXTRACT |
| `create_sphere_mesh()` | Create sphere primitive | EXTRACT |
| `create_plane_mesh()` | Create plane primitive | EXTRACT |
| `create_cylinder_mesh()` | Create cylinder primitive | EXTRACT |
| `create_torus_mesh()` | Create torus primitive | EXTRACT |
| `create_plane_xy_mesh()` | Create XY plane primitive | EXTRACT |
| `create_mesh_dynamic()` | Create CPU-updatable mesh | KEEP (core) |
| `update_mesh_dynamic()` | Update dynamic mesh | KEEP (core) |

**GFX:** Primitive creation (cube, sphere, etc.) doesn't belong on the renderer. These are utility functions that should be in `katla_gfx::primitives` module or a `MeshBuilder` type.

**APP:** Having primitive meshes is convenient, but they clutter the renderer API. I'd like them accessible but not on the renderer itself.

**Giga Recommendation:**
```rust
// In katla_gfx::primitives module:
use katla_gfx::primitives;

let cube = primitives::cube(&mut renderer, [1.0, 1.0, 1.0])?;
let sphere = primitives::sphere(&mut renderer, 1.0, 32, 32)?;

// Or builder pattern:
let cube = MeshBuilder::cube([1.0, 1.0, 1.0]).build(&mut renderer)?;
```

---

## 🔍 CATEGORY 6: VIEWPORT SYSTEM (12 methods)

**Status:** ⚠️ QUESTIONABLE - Should be a separate module

| Method | Purpose | Verdict |
|--------|---------|---------|
| `create_viewport()` | Create viewport | KEEP (in manager) |
| `viewport_count()` | Get viewport count | KEEP |
| `get_viewport()` | Get viewport (readonly) | KEEP |
| `get_viewport_mut()` | Get viewport (mutable) | KEEP |
| `viewport_texture_id()` | Get viewport texture | KEEP |
| `viewport_extent()` | Get viewport size | KEEP |
| `set_viewport_uniforms()` | Set viewport camera | KEEP |
| `set_viewport_draw_list()` | Set viewport draw list | KEEP |
| `clear_viewport_draw_list()` | Clear viewport draw list | KEEP |
| `destroy_viewport()` | Destroy viewport | KEEP |
| `is_viewport_ready()` | Check if viewport ready | KEEP |
| `update_viewport_camera()` | Update viewport camera | KEEP |

**GFX:** These should be on `ViewportManager`, not `VulkanRenderer`. The renderer can expose `manager()` or forward methods, but 12 viewport methods is too much surface area.

**Giga Recommendation:** Move these to `viewport_manager` module, expose `renderer.viewports()` accessor.

---

## 🔍 CATEGORY 7: SKELETAL ANIMATION (3 methods)

**Status:** ✅ REASONABLE - Specialized feature, small API surface

| Method | Purpose | Verdict |
|--------|---------|---------|
| `get_skeleton_descriptor()` | Get skeleton descriptor set | KEEP |
| `create_skeleton()` | Create skeleton | KEEP |
| `update_skeleton()` | Update joint matrices | KEEP |

**GFX:** These are fine. Small, focused, specialized feature.

---

## 🔍 CATEGORY 8: BINDLESS TEXTURES (2 methods)

**Status:** ✅ GOOD - Core abstraction

| Method | Purpose | Verdict |
|--------|---------|---------|
| `register_bindless_texture()` | Register texture for bindless | KEEP |
| `get_texture_bindless_index()` | Get bindless slot index | KEEP |

**GFX:** Good abstraction. Hides the complexity of bindless descriptor management.

---

## 🔍 CATEGORY 9: FRAME RENDERING (4 methods)

**Status:** ✅ CORE - Essential renderer functionality

| Method | Purpose | Verdict |
|--------|---------|---------|
| `set_frame_uniforms()` | Set per-frame uniforms | KEEP |
| `execute_draw_calls()` | Execute draw list | KEEP |
| `draw()` | Main draw loop (legacy) | KEEP? |
| `render()` | Main render entry point | KEEP |

**GFX:** These are core. `draw()` might be legacy if `render()` replaced it.

---

## 🔍 CATEGORY 10: FRAME GRAPH (2 methods)

**Status:** ✅ GOOD - Clean API

| Method | Purpose | Verdict |
|--------|---------|---------|
| `create_frame_graph()` | Create frame graph builder | KEEP |
| `render()` | Execute frame graph with callback | KEEP |

**GFX:** This is the right direction. Clean API, minimal surface area.

---

## 🔍 CATEGORY 11: MATERIAL REGISTRY (5 methods in AssetRegistry)

**Status:** ✅ GOOD - Internal registry, reasonable API

| Method | Purpose | Verdict |
|--------|---------|---------|
| `get_mesh()` | Get mesh asset | KEEP |
| `get_material()` | Get material asset | KEEP |
| `replace_material_pipeline()` | Hot-reload pipeline | KEEP |
| `mesh_count()` / `material_count()` | Debug info | KEEP |
| `clear()` | Clear all assets | KEEP |

**GFX:** These are on `AssetRegistry`, not directly on renderer. Good separation.

---

## 🔍 CATEGORY 12: TYPES MODULE (40+ methods)

**Status:** ✅ EXCELLENT - Builder pattern on value types, not renderer

The `types.rs` file has builder methods on `DrawCall`, `DrawList`, `UIDrawList` - these are fine because they're on the data types, not the renderer.

**Example:**
```rust
// These are fine - builder pattern on value types:
let draw = DrawCall::new(mesh, material)
    .with_transform(matrix)
    .with_color([1.0, 0.0, 0.0, 1.0])
    .with_pbr(0.0, 0.5, 1.0);
```

---

## 🎯 PRIORITIZED REFACTOR PLAN

### Phase 1: Quick Wins (Low Risk)

1. **Extract primitive mesh creation** → `katla_gfx::primitives` module
2. **Consolidate texture creation** → Keep 2 methods, remove wrappers
3. **Move viewport methods** → Expose through `ViewportManager`

### Phase 2: High Impact (Medium Risk)

4. **Consolidate material creation** → Single `compile_material()` API
5. **Extract UI methods** → Move to `katla_ui` or app-level

### Phase 3: Architecture (High Risk)

6. **Refactor ownership** → Remove `Rc<RefCell<>>` patterns
7. **Separate concerns** → Renderer as thin Vulkan wrapper, everything else as modules

---

## ✅ PROGRESS UPDATE (2025-03-10)

### Completed Tasks:
- ✅ #4: Material consolidation - Added unified `compile_material()` API
- ✅ #5: Backbuffer magic - Consolidated `BACKBUFFER_NAME` constant, removed string literals
- ✅ #6: Push descriptors hidden - Marked `push_descriptor_set_khr` as `pub(crate)`
- ✅ #8: Removed `Rc<RefCell<>>` pattern - Changed `Frame` to hold `&mut VulkanRenderer`
- ✅ #1: Material API documentation - Created comprehensive guide
- ✅ #7: PassBuilder documentation - Created comprehensive guide

### API Surface Reduced:
- **Before:** 50+ public methods
- **After:** ~43 public methods
- **Target:** < 30 methods
- **Progress:** ~14% reduction + improved architecture + documentation

### Key Changes:
1. `compile_material()` is now the single entry point for material creation
2. `create_pbr_material()`, `create_ui_material()`, `material_builder()` now call through `compile_material()`
3. `Frame` holds `&mut VulkanRenderer` instead of `&VulkanRenderer`
4. `ui_resources` is now `UiFrameResources` instead of `Rc<RefCell<UiFrameResources>>`
5. All `.borrow_mut()` calls removed - direct mutable access now
6. `BACKBUFFER_NAME` is now a single exported constant
7. Added `material/API.md` - Complete material creation guide with examples
8. Added `render_graph/API.md` - Complete frame graph usage guide with examples
9. Updated `lib.rs` with comprehensive module-level documentation

### Documentation Added:
- Material API guide with decision matrix, common patterns, shader examples
- Frame graph API guide with pass types, resource lifecycles, common rendering patterns
- Module-level documentation with quick start examples and API organization

---

## 📈 RECOMMENDED TARGET STATE

**Target:** ~20-25 public methods on `VulkanRenderer`

```
Core Renderer (~15 methods):
- init(), destroy(), wait_for_device(), recreate_swapchain()
- create_texture(), create_texture_solid()
- create_mesh(), create_mesh_dynamic(), update_mesh_dynamic()
- register_mesh(), register_material()
- compile_material(), set_frame_uniforms()
- execute_draw_calls()
- create_frame_graph(), render()
- context(), num_images()

Specialized (~5-8 methods):
- create_skeleton(), update_skeleton()
- register_bindless_texture(), get_texture_bindless_index()
- swapchain_extent(), output_extent()
- init_output_target()

Extracted to other modules:
- Material creation helpers → compile_material() + MaterialOptions
- Primitives → katla_gfx::primitives module
- UI methods → katla_ui or app-level
- Viewport methods → ViewportManager accessed via renderer.viewports()
```

---

## 🚨 CRITICAL ISSUES SUMMARY

1. **50+ methods on VulkanRenderer** - Way over target of < 30
2. **5 ways to create materials** - Should be 1 with options
3. **UI baked into renderer** - Should use generic primitives
4. **Primitives on renderer** - Should be in separate module
5. **Interior mutability (`Rc<RefCell<>>`)** - Ownership problem

---

## 📝 NEXT STEPS

1. ✅ Audit complete (this document)
2. → Start with material consolidation (task #4) - highest impact
3. → Extract primitives to separate module
4. → Move UI methods out of renderer
5. → Re-evaluate if we're under 30 methods

---

**Generated by:** Giga Maintainer
**Date:** 2025-03-10
**Status:** Ready for refactoring
