# TODO

## ECS

### P2: Features

- [ ] **Add entity/component removal events** — ECS does not emit removal events, so destroyed entity emitters (particle system) can't be detected automatically. Requires ECS-level hooks.

### P3: Polish

- [ ] **Fix doctests** — 7 doc examples still use ` ```ignore ` blocks (down from ~10). Convert key examples to runnable doctests.
- [ ] **Tighten public API surface** — `ComponentStorage`, `ComponentStorageManager`, `ImmutableQuery`, `QueryData`, `OrderedSystem` are `pub use`'d but never used by external crates. Should be `pub(crate)`.
- [ ] **Narrow `World::storage_mut()` exposure** — Exposes internal `ComponentStorageManager`. Used by `katla_app` camera systems; could be replaced with a narrower API.

## Gizmo

### UX

- [ ] Add plane-drag support (e.g., XY, XZ, YZ planes) for translate and scale modes
- [ ] Calibrate scale sensitivity to screen-space movement (magic 0.01 constant is not zoom-aware)

## katla_gfx

### P1: Stubs / Missing Implementations

- [ ] **Implement `create_transient_texture()`** — Currently returns a dead handle (`TextureHandle::new(0)`). Needed for UI rendering with transient textures.

### P2: Robustness

- [ ] **Remove hardcoded compositing viewport layout** — `render_graph/frame/compositing.rs` hardcodes split-screen rects. Should pass viewport rectangles via uniform buffer.
- [ ] **Integrate GPU particle timing** — `particles/timing.rs` has a full `TimestampQuery` struct (`#[allow(dead_code)]`) that is implemented but never used.

### P0: Visibility Tightening

- [ ] **Change `animation` module to `pub(crate) mod`** — Module visibility done (`pub(crate)` by default). But re-exports (`AnimChannelInfo`, `AnimClipHeader`, `JointInfo`, `SkeletonAnimParams`, `PoseComputeBuffers`, `PoseComputePipeline`) are still unconditional because `katla_app` consumes them. Need to either move types to a public module or update `katla_app` access path.
- [ ] **Change `shadow` module to `pub(crate) mod`** — Module visibility done. But `CascadeParams` re-export is still unconditional because `katla_app::builder` consumes it.
- [ ] **Change `lighting` module to `pub(crate) mod`** — Module visibility done. But `PointLightGPU` re-export is still unconditional because `katla_app::renderer` consumes it.

### P3: Polish

- [ ] **Consider a minimal `Mat4` type within katla_gfx** — All matrices still use raw `[f32; 16]`. No `Mat4` newtype exists.
- [ ] **Extract viewport/UI from renderer module** — Viewport and UI management still in `renderer/mod.rs`; should be split into own modules.
- [ ] **Clean up dead code** — `ShadowBuffers::len()/is_empty()`, `CascadeParams::cascades()`, `MaterialBuilder::with_push_constant_range()` are `#[allow(dead_code)]`.

## katla_app

### P1: Stubs / Missing Implementations

- [ ] **Implement `EditorAction::DuplicateEntity`** — Currently logs "not yet implemented" (`editor/mod.rs`). Entity duplication with all components is a stub.
- [ ] **Implement `EditorAction::ResetParticleSystem`** — Currently logs "not yet implemented" (`editor/mod.rs`). Particle system reset is a stub.

### P2: Robustness

- [ ] **Reduce `unwrap()` in physics systems** — `physics_system.rs` and `velocity_system.rs` have ~13 `unwrap()` calls on ECS component queries that will panic if components are missing.
- [ ] **Guard `TransformOptimization` resource access** — `transform_hierarchy_system.rs` calls `unwrap()` on `get_resource_mut::<TransformOptimization>()` which panics if not inserted.
- [ ] **Guard GLTF bone mapping** — `animation/gltf_loader.rs` calls `unwrap()` on `transforms.get(&idx)` which panics on malformed bone mappings.

### P3: Polish

- [ ] **Remove dead `DragToViewport.path` field** — `asset_browser/types.rs` has `#[allow(dead_code)]` on a field that is never read.
- [ ] **Guard asset browser edge cases** — `asset_browser/mod.rs` has `unwrap()` on `drag_asset`, `parent()`, `selected_index` that could panic on edge cases.

## katla_ui

### P2: Robustness

- [ ] **Guard `DrawList::finalize()` on empty lists** — `draw_list.rs` has 8 `unwrap()` calls for min/max computation that panic on empty draw lists.

### P3: Polish

- [ ] **Remove or wire up `selectable()` widget** — `context/widgets/selectable.rs` has `#[allow(dead_code)]` on an implemented but uncalled widget method.

### P3: Font Library Migration (ab_glyph → skrifa + vello_cpu) — DONE

Follow egui's approach: use `skrifa` for font parsing/outlining and `vello_cpu` for rasterization. Only 4 files needed changes; atlas and drawing code were unaffected.

- [x] **Add `skrifa` + `vello_cpu` dependencies** to `katla_ui/Cargo.toml` (also `kurbo` for Bézier paths)
- [x] **Replace `FontArc` storage with skrifa font types** — `text/mod.rs` stores `Arc<Vec<u8>>` in `HashMap<FontId, Arc<Vec<u8>>>`; constructs `skrifa::FontRef` on demand
- [x] **Migrate font loading** — `text/font_loading.rs` uses `FontRef::new(&data)` for validation
- [x] **Migrate text measurement** — `text/measurement.rs` uses `skrifa::MetadataProvider` equivalents
- [x] **Migrate glyph ID lookup** — `font.charmap().map(c)` in `rasterization.rs` and `measurement.rs`
- [x] **Migrate kerning** — stubbed to return 0.0; `// TODO: Add GPOS kerning support via skrifa's GPOS table access`
- [x] **Rewrite glyph rasterization** — `text/rasterization.rs` uses vello_cpu scene rendering (outline → `kurbo::BezPath` via `KurboPen` → `vello_cpu::RenderContext` → pixel buffer)
- [ ] **Adopt egui's subpixel quantization** — 4-bin for Latin, 1-bin for CJK (existing 4-bin subpixel still works, CJK optimization deferred)
- [x] **Verify atlas integration** — `RasterizedGlyph` output and `place_in_atlas` work unchanged
- [x] **Remove `ab_glyph` dependency** — from `katla_ui/Cargo.toml` and workspace `Cargo.toml`

## katla_math

### P3: Polish

- [ ] **Decide fate of scalar quaternion module** — `scalar::quat` module is `#[allow(dead_code)]` on x86/x86_64 (primary targets). Either gate behind cfg or remove.

## Cross-Cutting

### P3: Cleanup

- [ ] **Audit `#[allow(clippy::too_many_arguments)]`** — ~20 functions suppress this lint. Consider introducing parameter structs for functions with many arguments.

### P3: Dependency Hygiene

- [ ] **Upgrade skrifa to latest and deduplicate** — ~~katla_ui pins `skrifa 0.22` while `vello_cpu 0.0.7` transitively pulls `skrifa 0.40`, resulting in two copies in the binary.~~ Done: upgraded to `skrifa 0.40`, replaced custom `BoundsPen` with `ControlBoundsPen`, single version in tree.
- [ ] **Pool `vello_cpu::RenderContext` for CJK workloads** — Currently a fresh `RenderContext` + `Pixmap` is allocated per glyph cache miss. Acceptable for pre-cached ASCII but wasteful for runtime CJK input. Reuse a shared context or pool buffers.
