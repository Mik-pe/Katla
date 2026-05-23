# TODO

## Backend Abstraction Cleanup

### A. Remove MetalFrameGraph (dead code)

- [x] Delete `katla_gfx/src/metal/metal_frame_graph.rs` entirely
- [x] Remove `mod metal_frame_graph` and `pub use` from `katla_gfx/src/metal/mod.rs`
- [x] Verify no remaining imports or references to `MetalFrameGraph` across the codebase

### B. Modularize MetalRenderer (2998-line monolith → ~15 files)

- [x] Extract `bind_common_resources` and `draw_objects` into `metal/draw_helpers.rs`
- [x] Extract all `init_*_pipeline` methods into `metal/init_pipelines.rs`
- [x] Extract `recreate_render_targets` into `metal/render_targets.rs`
- [x] Make all struct fields `pub(crate)` for submodule access
- [x] Make constants and `read_shader`/`resolve_wgsl_includes` `pub(crate)`
- [x] Extract mesh management (`MetalMesh`, `create_mesh*`, `register_mesh_raw`, `update_mesh_dynamic`) into `metal/mesh_api.rs`
- [x] Extract texture management (`MetalTextureEntry`, `create_texture`, `create_texture_solid`, bindless slot queries) into `metal/texture_api.rs`
- [x] Extract material/pipeline management (`MetalMaterial`, `compile_material`) into `metal/material_api.rs`
- [x] Extract skeleton management (`create_skeleton`, `update_skeleton`) into `metal/skeleton_api.rs`
- [x] Extract viewport management (`create_viewport`, `viewport_count`, `get_viewport`, `destroy_viewport`) into `metal/viewport_api.rs`
- [x] Extract frame lifecycle (`begin_frame`, `end_frame`, `render_frame`, `wait_for_frame`) into `metal/frame_lifecycle.rs`
- [x] Extract font atlas (`create_ui_font_atlas`, `update_ui_font_atlas`) into `metal/font_atlas.rs`

### C. Unify pipeline initialization — eliminate Metal-specific methods on AnyRenderer

- [x] Add `init_light_culling(width, height, shader_path)` to `GpuRenderer` trait with default no-op impl
- [x] Add `init_shadow_resources()` to `GpuRenderer` trait with default no-op impl
- [x] Add `init_shadow_pipeline(shader_path)` and `init_shadow_pipeline_skinned(shader_path)` to `GpuRenderer` trait
- [x] Add `init_sky_pipeline(shader_path)` to `GpuRenderer` trait with default no-op impl
- [x] Add `init_tonemap_pipeline(shader_path)` to `GpuRenderer` trait with default no-op impl
- [x] Add `init_depth_prepass_pipeline(shader_path)`, `init_depth_prepass_skinned_pipeline`, `init_depth_prepass_billboard_pipeline` to `GpuRenderer` trait
- [x] Add `init_outline_pipelines(stencil_mark, stencil_mark_skinned, outline_draw, outline_draw_skinned)` to `GpuRenderer` trait
- [x] Add `init_picking_pipeline(shader_path)` and `init_picking_skinned_pipeline(shader_path)` to `GpuRenderer` trait
- [x] Add `init_stencil_indicator_pipelines` to `GpuRenderer` trait
- [x] Add `set_viewport_bindless_slot(slot)` to `GpuRenderer` trait with default no-op impl
- [x] Remove `#[cfg(target_os = "macos")]` from init_* and set_viewport_bindless_slot on AnyRenderer — now dispatches to both backends
- [ ] Add `set_geometry_hdr_view` and `set_tonemap_output_view` to `GpuRenderer` trait — needs backend-agnostic texture view type

### D. Clean up `cfg(target_os = "macos")` gating in AnyFrameGraph / AnyFrame

- [x] Verify `AnyFrame` has no Metal-only methods (confirmed clean — just `submit`, `submit_ui`, `dispatch`)
- [ ] Remove `transient_image_view_metal()` and `transient_texture_metal()` Metal-only methods from `AnyFrameGraph` — requires backend-agnostic texture view type
- [ ] Audit `AnyFrameGraph` for all `#[cfg(target_os = "macos")]` branches that could be collapsed

### E. Align Metal backend with shared FrameGraph<B> execution path

- [ ] Verify `RenderGraphBackend` impl for `MetalRenderer` is complete (create/destroy transient textures, bindless registration, swapchain/depth image views)
- [ ] Verify `Frame<'_, MetalRenderer>` execution dispatches all pass types (geometry, shadow, fullscreen, compositing, particles, outline, UI, depth prepass) through `RenderGraphBackend` trait methods
- [ ] Ensure Metal backend's `render_frame()` goes through `FrameGraph<MetalRenderer>::execute()` identically to the Vulkan path, not through a separate hardcoded pass sequence
- [ ] Remove any remaining dual-code-path divergence between how Vulkan and Metal execute the same frame graph
- [ ] Delete the `render_frame()` method from `GpuRenderer` if it's now unused (Vulkan already no-ops it, Metal should use `render()` with frame graph)

### F. Reduce cfg(target_os = "macos") count across katla_gfx

- [x] Audit all `#[cfg(target_os = "macos")]` in `katla_gfx/src/lib.rs` — minimize to module declarations only
- [x] Audit all `#[cfg(target_os = "macos")]` in `katla_gfx/src/renderer/any_renderer.rs` — after task C, the only remaining ones should be the enum variant definitions and match arms
- [x] Audit all `#[cfg(target_os = "macos")]` in `katla_gfx/src/render_graph/any_frame_graph.rs` — after task D, only enum variant + match arms remain
- [ ] Ensure `katla_app` has zero `cfg(target_os)` or `cfg(metal/vulkan)` gates (already tracked as existing TODO item)

## Particle System Usability

- [x] Fix position duplication between Transform and EmitterConfig — ParticleSystem::update should read the entity's TransformComponent and override config.position so emitters follow their entity when moved
- [x] Wrap EmitterConfig GPU padding behind a user-facing builder or separate user config struct — users should not need to set _pad_position, _pad_velocity, _pad_color, _pad_forces manually
- [x] Change EmitterConfig.shape from raw u32 to EmitterShape enum — provide direct field access without requiring set_shape()/get_shape() helpers
- [x] Fix with_line_shape axis parameter being silently ignored — removed the unused axis parameter since the shader only supports Y-axis lines
- [x] Add editing controls to the ParticleInspector — sliders/drag fields for emit rate, lifetime, scale, color, forces etc. instead of read-only text rows
- [x] Add built-in preset factory functions — EmitterPreset::fire(), EmitterPreset::smoke(), EmitterPreset::sparks() etc. with sensible defaults
- [x] Change ParticleSystem::update to take &mut GlobalParticleSystem instead of &mut Option<GlobalParticleSystem> — avoids silent skip and awkward call sites
- [ ] Add per-emitter alive count feedback — allow querying actual alive particle count per emitter, not just theoretical estimated_max_alive
- [x] Add kill-all-particles-on-destroy option — optionally immediately kill all living particles when an emitter is destroyed instead of letting them expire naturally
- [ ] Add color over lifetime and size over lifetime curves — enable fire (bright to dark), smoke (opaque to transparent), sparks (big to small) effects without shader modifications

## Audio System

- [ ] Add audio crate (katla_audio) to workspace — choose backend (e.g. cpal + lewton for decoding, or kira for a higher-level solution)
- [ ] Implement basic audio playback — load and play WAV/OGG files, stereo mixing, volume control
- [ ] Add AudioSource component and AudioSystem — play one-shot sounds triggered by gameplay events
- [ ] Add 3D positional audio — spatialize sounds based on emitter TransformComponent relative to listener (camera)
- [ ] Add audio mixing — master volume, category channels (SFX, music, ambient), per-source volume
- [ ] Add streaming audio — support long-running music tracks without loading entire file into memory
- [ ] Integrate audio into asset browser — show audio files with waveform preview, drag-to-spawn AudioEmitter entities

## Physics

- [ ] Add collision detection — broadphase (sweep-and-prune or grid), narrowphase (SAT or GJK), contact generation
- [ ] Add collision shapes — AABB, sphere, box, capsule, mesh collider components
- [ ] Add rigid body dynamics — mass, inertia, angular velocity, torque, integration (Verlet or semi-implicit Euler)
- [ ] Add constraints and joints — point-to-point, hinge, distance constraints
- [ ] Add physics raycasting — raycast query returning hit entity, point, normal, distance
- [ ] Add trigger volumes — overlap detection without collision response (sensors)
- [ ] Add physics materials — friction, restitution, density per-shape
- [ ] Add physics debug visualization — wireframe collider rendering in editor viewport
- [ ] Decide: build custom or integrate existing physics crate (rapier, physx, jolt) — evaluate tradeoffs for the engine's scope

## Rendering

- [x] Remove all `cfg(metal/vulkan)` from katla_app — backend-specific conditionals should only exist in katla_gfx
- [ ] Add anti-aliasing — start with FXAA (post-process, easy), then MSAA or TAA for higher quality
- [ ] Add bloom post-processing pass — bright extraction + gaussian blur + compositing in render graph
- [ ] Add SSAO (screen-space ambient occlusion) — depth+normal based, integrate into lighting pass
- [ ] Add texture compression — BC1-7 on desktop, ASTC on mobile; add compressed texture upload path
- [ ] Add offline shader compilation step — precompile .wgsl to SPIR-V at build time instead of runtime naga compilation
- [ ] Add animation state machine — blend trees, crossfade transitions, state graph editor
- [ ] Add motion blur and depth of field as optional render graph passes
- [ ] Add screen-space reflections (SSR) or planar reflections for water/mirror surfaces

## Scripting & Game Logic

### Phase 1: Crate skeleton + on_update
- [x] Create `katla_script` crate with `mlua` dependency (features: `luau`, `vendored`, `serialize`)
- [x] Implement `ScriptComponent` — holds script path + `ScriptInstanceHandle`, derives `Component` via katla_derive
- [x] Implement `ScriptEngine` — single Luau VM, script cache (path -> compiled chunk), instance registry
- [x] Implement `ScriptSystem` (ECS System trait) — discovers entities with ScriptComponent, calls `on_update(entity, world, dt)`
- [x] Register `Vec3` and `Transform` as Luau `UserData` — field getters/setters, operator overloads (+, -, * scalar), utility methods (length, normalize, dot, cross)
- [x] Implement command queue — `ScriptWorld` proxy queues mutations (SetTransform, SetPosition, SpawnEntity, DestroyEntity), applied after all scripts run
- [x] Define `ScriptWorldAccess` trait in `katla_script` — abstract interface for get_transform/set_transform/spawn/destroy using raw `katla_math` types; `katla_app` provides concrete impl that bridges to TransformComponent
- [x] Wire `ScriptSystem` into `ApplicationBuilder` at `NORMAL` execution order

### Phase 2: Lifecycle hooks + entity operations
- [x] Add `on_spawn` hook — called when entity first gets a ScriptComponent
- [x] Add `on_destroy` hook — called when entity with ScriptComponent is destroyed
- [x] Register `EntityId` as Luau `UserData` — `id()`, `__tostring`, comparison operators
- [x] Add entity spawn/destroy from scripts — `world:spawn_entity()` returns EntityId, `world:destroy_entity(id)` queues destruction
- [x] Add entity query from scripts — `world:get_all_with("ScriptComponent")` (entity_exists already implemented)

### Phase 3: Input + serialization + error handling
- [x] Add input bindings — `world:is_action_pressed("move_forward")`, `world:get_mouse_delta()`, reads from InputState resource
- [x] Add `ScriptComponent` to scene serialization — `ScriptComponent(script_path: "scripts/player")` in RON scene files
- [x] Add script error recovery — wrap all hook calls in error handlers, log errors with script path + line number, optionally disable broken instances
- [x] Add `Color` and `Quat` UserData bindings — constructors, field access, utility methods (Quat.from_axis_angle, Color.from_rgb, etc.)

### Phase 4: Hot reload + sandboxing
- [ ] Add file watcher for `.luau` scripts — detect changes in `resources/scripts/`, trigger recompile
- [ ] Implement hot reload — recompile chunk, create new per-script environment, preserve scalar state from old env, swap instances
- [x] Harden VM sandboxing — initialize with `StdLib::ALL_SAFE` (no io/os/debug), configure interrupt watchdog for runaway scripts
- [x] Add `print`/`warn` bridges — route to `log::info!`/`log::warn!` in debug builds only

### Phase 5: Polish + events
- [ ] Add script-to-script events — `world:emit("player_damaged", {amount = 10})`, `world:on_event("player_damaged", callback)` via gameplay event bus
- [ ] Add physics bindings — `world:raycast(origin, direction, max_distance)` returning hit entity + point + normal
- [ ] Add audio bindings — `world:play_sound("explosion")`, `world:play_sound_at("explosion", position)`
- [ ] Performance profile — benchmark 1000 script entities with on_update, optimize hot paths

### Phase 6: Editor integration
- [ ] Add script inspector panel — show attached script path, expose script variables for live editing
- [ ] Add script file browser — show `.luau` files in asset browser, drag-to-attach to entity
- [ ] Generate Luau type definition files (.d.luau) — autocomplete support for engine API in external editors
- [ ] Add script console — capture `print()` output in editor log panel

### Gameplay framework (independent of scripting)
- [ ] Design gameplay framework — game states (menu, loading, playing, paused), state machine, transition hooks
- [ ] Add gameplay event system — typed event bus for gameplay-level events (OnDamage, OnCollect, OnCollision, etc.) decoupled from ECS events
- [ ] Add cutscene/timeline system — sequencer with tracks for animation, audio, camera, events; scrubbing in editor

## Asset Pipeline

### AI Agent — Asset & Script Tools

- [x] Add `list_assets` tool to AI agent — list files in `resources/` recursively, with optional extension filter (`"luau"`, `"gltf"`) and subdir filter (`"scripts"`). Lets the AI discover available scripts and assets.
- [x] Add `read_asset` tool to AI agent — read file contents from `resources/` by relative path. Lets the AI inspect existing scripts, materials, etc.
- [x] Add `write_asset` tool to AI agent — create or overwrite files in `resources/` with given path and content. Enables full workflow: AI creates a script, adds ScriptComponent, sets the path, script is ready to run.
- [x] Add `delete_asset` tool to AI agent — delete files from `resources/` by relative path. Should refuse to delete non-empty directories.

### General asset pipeline

- [ ] Add file watcher for hot reload — watch shaders/, resources/ for changes using `notify` crate; auto-recompile materials and reload textures
- [ ] Add asset bundling format — pack resources into a single archive (custom or zip/pak) for release builds; embed or ship alongside binary
- [ ] Add component serialization registry — data-driven registry mapping Component types to serializers/deserializers so user components round-trip automatically
- [ ] Add native file dialogs — integrate `rfd` for Open Scene, Save Scene As, Import Asset dialogs
- [ ] Add binary serialization option — optional binary scene format (e.g. bincode) alongside RON for faster load times in release
- [ ] Add asset import pipeline — convert source formats (FBX, PSD, TGA) to engine formats (glTF, PNG) as a preprocessing step

## Release & Deployment

- [x] Add CI/CD pipeline — GitHub Actions for build, test, clippy, fmt on push; artifact upload for release builds
- [ ] Add macOS .app bundle generation — Info.plist, icon, embed MoltenVK, package as .dmg for distribution
- [ ] Add Windows build target — cross-compile or native CI runner, .exe packaging, Vulkan runtime bundling
- [ ] Add Linux build target — AppImage or Flatpak packaging, Vulkan/ABI compatibility
- [ ] Add app signing and notarization — macOS Developer ID signing, Windows code signing
- [ ] Add save-game system — persist runtime game state (player progress, settings, unlocked content) separate from scene serialization
- [ ] Add release mode resource embedding — embed critical assets (shaders, default textures, fonts) into binary for zero-dependency startup

## Editor

### Inspector component UX

- [x] Add inspector UI for all registered components — `MassComponent` (mass slider), `DragComponent` (coefficient slider), `PerspectiveComponent` (fov/near/aspect_ratio sliders), `DirectionalLight` (direction Vec3, color picker, intensity slider). Each needs a `section_header_with_remove` section with editable widgets.
- [x] Add remove button to ParticleEmitter inspector section — switch from `section_header` to `section_header_with_remove`, handle `EditorAction::RemoveComponent` for `"ParticleEmitterComponent"`
- [x] Categorize the Add Component popup menu — group flat list into Lighting (PointLight, DirectionalLight), Physics (MassComponent, DragComponent), Scripting (ScriptComponent), Camera (PerspectiveComponent) with category headers
- [x] Auto-focus script path input after adding ScriptComponent — focus the text input so the user can immediately type the path without clicking
- [x] Wire up undo/redo for Add/Remove Component — store proper `SceneCommand`s instead of discarding the `UndoGroup`, so Ctrl+Z works after adding or removing components

### Scene lifecycle

- [ ] Preserve entity names when playing a scene — entity names change from e.g. "Entity 1" to "Entity 4294967336" (raw EntityId) when the default scene is played, should keep the original human-readable names
- [ ] Fix entities (fox, helmet) disappearing from view on play/stop — some entities vanish from the viewport when entering or exiting play mode, possibly moved to NaN positions or destroyed (names also change, see above)

### Component registry completeness

- [x] Register ParticleEmitterComponent in the component registry — has full inspector UI (5 sliders) but can't be added via "Add Component" or AI agent
- [x] Register VelocityComponent in the component registry — serialized in scenes but not addable from UI or AI
- [x] Add serialization round-trip for components missing it — MassComponent, DragComponent, PerspectiveComponent, DirectionalLight are registered but lost on scene save/load

### Inspector menu

~~Make inspector menu scrollable — when many components are attached to an entity, the inspector overflows and content below the viewport becomes inaccessible~~ — False positive. Inspector already has ScrollArea wrapping all content.
- [x] Make "Add Component" menu scrollable — too many items to fit in the viewport, needs scrolling to access components that overflow

### Panels and tooling

- [ ] Add timeline/animation editor — keyframe editing, curve editor, scrubbing, animation preview
- [ ] Add material editor — visual material property editing (textures, metallic, roughness, emission) with live preview
- [ ] Add terrain editor — heightmap painting, layer blending, foliage scattering
- [x] Add console/output log panel — capture log output in editor, filter by level, search
- [ ] Add undo history panel — visual undo stack showing operation names, click to jump to any point
- [ ] Add dockable layout system — complete the existing DockLayout skeleton, make all panels repositionable and resizable
- [ ] Add in-editor profiler overlay — per-pass GPU timing, frame time graph, draw call count, memory usage
- [ ] Add gamepad input support — extend InputMapper with gamepad axes/buttons for editor and runtime
- [x] Fix asset browser tooltip line spacing — hover tooltip on asset items has inconsistent line spacing compared to the rest of the UI
- [x] Fix text input selection/active highlight being too opaque — the "Filter" input in asset browser and "Script" path input have a selection color that's too bright/invasive, obscuring the text. Investigate if transparency isn't rendering correctly. Should be fixed in a reusable text input style so all text inputs benefit.

## Developer Experience

- [ ] Write getting-started tutorial — step-by-step guide: create entity, add components, write a system, load a model, make something interactive
- [ ] Write component and system catalog — reference docs for all built-in components, systems, and their fields
- [ ] Write example game in game/ crate — demonstrate actual gameplay: player movement, collecting items, score, win/lose
- [ ] Add profiler integration — Tracy or PIX instrumentation markers on render passes and systems
- [ ] Add per-pass GPU timing — timestamp queries in render graph, display in status bar or overlay
- [x] Fix AppError::Graphics to carry typed RendererError instead of String — preserve error chain for debugging
- [ ] Add integration tests for full app lifecycle — init, spawn entities, run N frames, check state, shutdown without panic
