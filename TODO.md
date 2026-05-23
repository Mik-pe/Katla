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
- [ ] Audit `AnyFrameGraph` for all `#[cfg(target_os = "macos")]` branches that could be collapsed — verified: only enum variants, match arms, and two Metal-specific accessors (`transient_image_view_metal`, `transient_texture_metal`) remain; the accessors require a backend-agnostic texture view type to remove (same blocker as D item above)

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

### G. Decompose GpuRenderer monolith — extract primitive mesh generators off the trait

- [x] Create `katla_gfx/src/primitives/mod.rs` as a public submodule — move all `generate_cube_vertices`, `generate_sphere_vertices`, etc. functions out of `MeshManager` and `renderer/mesh_manager.rs` into free functions in this new module
- [x] Add public `create_primitive_mesh()` free function in `katla_gfx/src/primitives/mod.rs` — takes `&mut impl GpuRenderer`, calls the appropriate generator, then calls `renderer.create_mesh()` on the result; this replaces calling `renderer.create_cube_mesh()` etc.
- [x] Add `create_cube()`, `create_sphere()`, `create_plane()`, `create_cone()`, `create_cylinder()`, `create_torus()`, `create_plane_xy()` convenience free functions that delegate to `create_primitive_mesh()`
- [x] Remove `create_cube_mesh`, `create_sphere_mesh`, `create_plane_mesh`, `create_cone_mesh`, `create_cylinder_mesh`, `create_torus_mesh`, `create_plane_xy_mesh` from `GpuRenderer` trait
- [x] Remove the 7 methods from `VulkanRenderer` impl of `GpuRenderer` — these now delegate to MeshManager internally, and the free functions call `create_mesh` instead
- [x] Remove the 7 methods from `MetalRenderer` impl of `GpuRenderer`
- [x] Remove the 7 match arms from `AnyRenderer` impl of `GpuRenderer`
- [x] Update all call sites in `katla_app` — replace `renderer.create_cube_mesh(size)` with `primitives::create_cube(&mut renderer, size)` etc.
- [x] Run `cargo check --workspace` to verify no compile errors
- [x] Run `cargo test --workspace` to verify all tests pass

### H. Decompose GpuRenderer monolith — remove register_mesh_raw from trait

- [x] Remove `register_mesh_raw` from `GpuRenderer` trait — it is documented as "Backend-specific; callers should use backend types directly", which means it does not belong on a backend-agnostic trait
- [x] Remove `register_mesh_raw` from `VulkanRenderer` impl of `GpuRenderer` — callers that need raw mesh registration should use `VulkanRenderer::create_mesh_dynamic()` directly
- [x] Remove `register_mesh_raw` from `MetalRenderer` impl of `GpuRenderer`
- [x] Remove `register_mesh_raw` match arm from `AnyRenderer` impl of `GpuRenderer`
- [x] Update any call sites in `katla_app` to use `create_mesh_dynamic` instead, or go through `as_vulkan()`/`as_metal()` escape hatch
- [x] Run `cargo check --workspace` and `cargo test --workspace`

### I. Decompose GpuRenderer monolith — remove create_mesh_soa from trait

- [x] Remove `create_mesh_soa` from `GpuRenderer` trait — it is unimplemented on Vulkan (`todo!()`) and takes `HashMap<u32, Vec<u8>>` which loses type safety
- [x] Remove `create_mesh_soa` from `VulkanRenderer` impl — already `todo!()`, so no functional change
- [x] Remove `create_mesh_soa` from `MetalRenderer` impl
- [x] Remove `create_mesh_soa` match arm from `AnyRenderer` impl
- [x] If SOA mesh creation is needed later, design a proper typed API (e.g. `AttributeType` enum key) as a separate trait or method, not on the core GpuRenderer
- [x] Run `cargo check --workspace` and `cargo test --workspace`

### J. Decompose GpuRenderer monolith — consolidate pipeline init methods into a single register_pass_pipeline

- [ ] Add `PassKind`-based `init_pass_pipeline` method to `GpuRenderer` trait — `fn init_pass_pipeline(&mut self, kind: PassKind, shader_paths: &[&Path]) -> Result<(), RendererError>` with a default no-op impl
- [ ] Add `init_pass_pipeline` dispatch to `AnyRenderer` — match on backend, delegate to backend impl
- [ ] Implement `init_pass_pipeline` for `VulkanRenderer` — match on `PassKind` to call the existing `init_shadow_pipeline`, `init_depth_prepass_pipeline`, `init_outline_pipelines`, etc. internally
- [ ] Implement `init_pass_pipeline` for `MetalRenderer` — match on `PassKind` to call the existing Metal pipeline init methods internally
- [ ] Update all `init_*_pipeline` call sites in `katla_app` to use the new single `init_pass_pipeline(kind, paths)` API
- [ ] Mark old individual `init_*_pipeline` methods as deprecated or remove them from the trait (keep them as private backend methods)
- [ ] Run `cargo check --workspace` and `cargo test --workspace`

### K. Unify frame lifecycle — fix begin_frame / end_frame / render_frame asymmetry

- [ ] Audit how Vulkan and Metal backends use `begin_frame`, `end_frame`, `render_frame`, `wait_for_frame` — document the actual control flow for each backend
- [ ] Remove `render_frame` from `GpuRenderer` trait — Vulkan already no-ops it; Metal should use the `render()` + frame graph path instead (aligned with section E goal)
- [ ] Ensure Metal `begin_frame` + `end_frame` pair covers acquire + present, matching Vulkan's `wait_for_frame` + `render()` pattern
- [ ] Document the canonical frame lifecycle in GpuRenderer trait docs: `begin_frame()` -> `set_frame_uniforms()` -> `execute_draw_calls()` -> render graph `render()` -> (implicit present)
- [ ] Run `cargo check --workspace` and `cargo test --workspace`

### L. Fix resize() semantic — stop silently discarding frame graph state

- [ ] Change `GpuRenderer::resize` signature to accept a frame graph reference: `fn resize(&mut self, width: u32, height: u32, frame_graph: &mut dyn FrameGraphResize) -> Result<(), RendererError>` or equivalent
- [ ] Update Vulkan impl to pass the actual app's frame graph to `recreate_swapchain` instead of creating a throwaway `FrameGraph::new()`
- [ ] Update Metal impl to handle resize properly (recreate render targets, transient textures)
- [ ] Update `AnyRenderer` dispatch and all call sites in `katla_app`
- [ ] Run `cargo check --workspace` and `cargo test --workspace`

### M. Generalize texture update API — remove font atlas special-case

- [x] Add `update_texture(&mut self, handle: TextureHandle, data: &[u8])` to `GpuRenderer` trait — general-purpose texture subresource update (default no-op impl; real impl deferred to when concrete use cases arise)
- [x] Add `update_texture` dispatch to `AnyRenderer` (uses default impl, no explicit dispatch needed)
- [ ] Implement `update_texture` for `VulkanRenderer` — copy data to existing texture, handle buffer-image copy
- [ ] Implement `update_texture` for `MetalRenderer` — replace texture data via blit
- [ ] Refactor `update_ui_font_atlas` to use `update_texture` internally
- [ ] Refactor `create_ui_font_atlas` to use `create_texture` internally (if not already)
- [ ] Consider removing `create_ui_font_atlas` and `update_ui_font_atlas` from the trait once the general API works — font atlas management can live in katla_ui or katla_app
- [x] Run `cargo check --workspace` and `cargo test --workspace`

### N. Add GPU capability queries to GpuRenderer

- [x] Define `GpuCapabilities` struct in `katla_gfx/src/renderer/` — fields: `max_texture_size: u32`, `max_bindless_textures: u32`, `supports_compute: bool`, `max_frame_in_flight: usize`, `vendor: GpuVendor` (enum: Nvidia, Amd, Intel, Apple, Unknown)
- [x] Add `fn capabilities(&self) -> &GpuCapabilities` to `GpuRenderer` trait
- [x] Populate `GpuCapabilities` in `VulkanRenderer` from Vulkan physical device properties
- [x] Populate `GpuCapabilities` in `MetalRenderer` from Metal device properties
- [x] Add dispatch to `AnyRenderer`
- [ ] Replace `has_light_culling()` bool on the trait with a field on `GpuCapabilities`
- [x] Run `cargo check --workspace` and `cargo test --workspace`

### O. Add timestamp query API for GPU profiling

- [ ] Define `GpuTimestamp` struct — `pass_name: String`, `duration_ms: f64`
- [ ] Add `fn begin_timestamp(&mut self, label: &str)` and `fn end_timestamp(&mut self, label: &str)` to `GpuRenderer` trait with default no-op impls
- [ ] Add `fn read_timestamps(&self) -> Vec<GpuTimestamp>` to `GpuRenderer` trait with default empty impl
- [ ] Implement timestamp queries for `VulkanRenderer` using Vulkan timestamp queries
- [ ] Implement timestamp queries for `MetalRenderer` using `MTLCounterSampleBuffer`
- [ ] Add dispatch to `AnyRenderer`
- [ ] Run `cargo check --workspace` and `cargo test --workspace`

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

### Phase 1: Crate skeleton + backend setup
- [ ] Create `katla_audio` crate in workspace — add to `Cargo.toml` workspace members, create crate skeleton with `lib.rs`
- [ ] Choose and integrate audio backend — evaluate `cpal` (low-level) vs `kira` (high-level) for output; add dependency to `katla_audio/Cargo.toml`
- [ ] Add audio decoder dependency — `lewton` for OGG Vorbis, `hound` for WAV; wrap behind a common `DecodedAudio` struct (sample rate, channel count, PCM samples)
- [ ] Implement `AudioDevice` — open default output device, create output stream, manage sample rate and buffer size
- [ ] Implement `AudioMixer` — mix N active voices into a single output buffer, handle clipping prevention (soft clamp)
- [ ] Implement `AudioVoice` — represents a single playing sound: source buffer, playback position, volume, looping flag, finished flag
- [ ] Add basic playback API — `AudioEngine::play(sound: &AudioBuffer) -> VoiceHandle`, `VoiceHandle::stop()`, `VoiceHandle::set_volume()`
- [ ] Write unit tests — decode WAV/OGG files to PCM, mix two buffers, verify output sample ranges

### Phase 2: ECS integration
- [ ] Add `AudioSource` component — holds asset path to sound file, derives `Component` via katla_derive
- [ ] Add `AudioListener` component — marks the camera entity that receives positional audio (only one active at a time)
- [ ] Add `AudioEmitter` component — holds volume, looping, playback state; references AudioSource path
- [ ] Implement `AudioSystem` (ECS System trait) — discover entities with AudioEmitter, trigger playback on spawn, stop on destroy
- [ ] Register audio types in `ApplicationBuilder` — add AudioEngine as a resource, register AudioSystem at appropriate execution order
- [ ] Add component serialization for AudioSource and AudioEmitter — RON round-trip support

### Phase 3: 3D positional audio
- [ ] Add `AudioListener` position tracking — read listener entity's TransformComponent each frame, feed position + orientation to spatializer
- [ ] Implement distance-based attenuation — inverse distance model (clamped) for volume falloff based on emitter-to-listener distance
- [ ] Implement panning / spatialization — stereo pan based on emitter direction relative to listener forward vector
- [ ] Add distance model options — linear, inverse clamped, exponential; configurable per-emitter or globally
- [ ] Add minimum/maximum distance and rolloff factor to AudioEmitter — control attenuation curve parameters

### Phase 4: Mixing and streaming
- [ ] Add master volume control — global volume slider applied to final mix output
- [ ] Add audio category channels — SFX, Music, Ambient sub-mixes with independent volume controls
- [ ] Add per-source volume and pitch — VoiceHandle::set_volume(), VoiceHandle::set_pitch() (resampling)
- [ ] Implement audio streaming — stream long audio files (music) in chunks instead of loading entire file; ring buffer for decoded chunks
- [ ] Add looping support — seamless loop points for music, configurable loop region for one-shot variations

### Phase 5: Editor and asset integration
- [ ] Add audio file loading to asset pipeline — recognize .wav/.ogg extensions, decode and cache AudioBuffers
- [ ] Add audio entries to asset browser — show audio files with icon, duration, sample rate metadata
- [ ] Add audio preview in asset browser — play/pause button on audio asset hover or selection
- [ ] Add audio inspector UI — volume slider, looping toggle, category selector for AudioSource/AudioEmitter components
- [ ] Add drag-to-spawn AudioEmitter — drag audio file from asset browser into viewport to create entity with AudioEmitter

## Physics

### Phase 0: Architecture decision
- [ ] Evaluate physics crates (rapier, physx, jolt) vs custom — compare API ergonomics, ECS compatibility, feature set, license, and maintenance status for the engine's scope
- [ ] Write ADR (Architecture Decision Record) documenting the choice — include rationale, tradeoffs, and integration strategy

### Phase 1: Crate skeleton + collision shapes
- [ ] Create `katla_physics` crate in workspace — add to `Cargo.toml` workspace members, create crate skeleton with `lib.rs`
- [ ] Define collision shape types — `SphereShape(f32)`, `BoxShape { half_extents: Vec3 }`, `CapsuleShape { half_height, radius }`, `AABB { min, max }`
- [ ] Add `ColliderShape` component — holds a collision shape, derives `Component` via katla_derive
- [ ] Add `ColliderState` component — stores computed world-space AABB, collision layer/mask flags
- [ ] Implement AABB computation for each shape — transform local shape by entity TransformComponent to get world-space AABB
- [ ] Add serialization for collider components — RON round-trip for ColliderShape and physics materials
- [ ] Register collider components in `ApplicationBuilder` and component registry

### Phase 2: Broadphase + narrowphase
- [ ] Implement broadphase — sweep-and-prune on sorted AABB intervals, output overlapping pair list
- [ ] Implement broadphase layer/mask filtering — only test pairs whose collision layers overlap
- [ ] Implement narrowphase: sphere-sphere test — distance < r1 + r2, return contact point and normal
- [ ] Implement narrowphase: sphere-box test — closest point on box to sphere center
- [ ] Implement narrowphase: box-box (SAT) — test separating axes, return contact manifold
- [ ] Implement narrowphase: sphere-capsule and box-capsule tests
- [ ] Define `ContactManifold` struct — contact points, penetration depth, contact normal for each pair
- [ ] Implement `CollisionSystem` (ECS System trait) — run broadphase then narrowphase each frame, generate contact events

### Phase 3: Rigid body dynamics
- [ ] Add `RigidBody` component — body type (static, dynamic, kinematic), mass, inertia tensor, linear/angular velocity, forces accumulator
- [ ] Implement semi-implicit Euler integration — apply gravity, accumulated forces, update velocity and position each frame
- [ ] Implement collision response — impulse-based resolution using contact manifolds, friction, restitution
- [ ] Add `RigidBodySystem` (ECS System trait) — integrate dynamics, apply forces, sync position back to TransformComponent
- [ ] Implement sleeping — mark near-stationary dynamic bodies as sleeping, skip integration, wake on contact
- [ ] Add physics materials — `PhysicsMaterial { friction, restitution, density }` attached to ColliderShape
- [ ] Implement force application API — apply_force(), apply_impulse(), apply_torque() on RigidBody

### Phase 4: Constraints and raycasting
- [ ] Implement point-to-point constraint — pin two bodies at a shared world point
- [ ] Implement hinge constraint — point-to-point with rotation axis limit
- [ ] Implement distance constraint — maintain fixed distance between two anchor points
- [ ] Add raycast query API — `PhysicsWorld::raycast(origin, direction, max_distance, layer_mask) -> Option<RayHit>`
- [ ] Add shape cast query — sweep a shape along a ray, return first hit
- [ ] Add trigger volumes — collider with sensor flag (no collision response, emits overlap events)
- [ ] Expose raycasting to scripting — `world:raycast(origin, direction, max_distance)` binding

### Phase 5: Debug visualization
- [ ] Add wireframe collider rendering — draw sphere, box, capsule outlines in editor viewport using line primitives
- [ ] Color-code collider types — static=blue, dynamic=green, kinematic=yellow, trigger=purple, sleeping=dimmed
- [ ] Add contact point visualization — render contact normals and penetration depth for selected entity
- [ ] Add physics debug toggle — menu option or hotkey to enable/disable physics wireframe overlay
- [ ] Add raycast visualization — render ray and hit point when performing interactive raycasts in editor

### Phase 6: Editor integration
- [ ] Add collider inspector UI — shape type dropdown, shape-specific parameters (radius, half-extents), physics material fields
- [ ] Add rigid body inspector UI — body type dropdown, mass/inertia fields, velocity display (read-only in play mode)
- [ ] Add Add Component entries — ColliderShape, RigidBody in categorized Add Component menu
- [ ] Add drag-to-add collider — automatically fit collider shape to mesh bounds when attached to mesh entity

## Rendering

- [x] Remove all `cfg(metal/vulkan)` from katla_app — backend-specific conditionals should only exist in katla_gfx

### Post-processing pipeline
- [ ] Add post-process pass infrastructure — reusable fullscreen-quad pass builder in the render graph that takes an input color texture and outputs a processed color texture
- [ ] Add FXAA pass — luminance edge detection, sub-pixel blending; add `fxaa.wgsl` shader; wire into render graph after tonemapping
- [ ] Add bloom pass — bright extraction threshold pass, two-pass gaussian blur (horizontal + vertical) at half resolution, additive compositing onto scene; add bloom shader(s)
- [ ] Add motion blur pass — per-pixel velocity buffer from depth + camera motion, tile-max velocity, blur in motion direction; add `motion_blur.wgsl`
- [ ] Add depth of field pass — circle-of-confusion calculation from depth, separate near/far bokeh blur, compositing; add `dof.wgsl`

### Screen-space effects
- [ ] Add SSAO pass — generate depth+normal buffer, hemisphere sampling kernel, bilateral blur, integrate into lighting shader as ambient occlusion term; add `ssao.wgsl`
- [ ] Add SSR pass — ray-march depth buffer from reflected fragments, fade by distance/edge, temporal accumulation for stability; add `ssr.wgsl`
- [ ] Add ambient lighting integration — expose SSAO texture in lighting pass, sample in PBR shader to modulate ambient term

### Texture compression
- [ ] Add BC1-5 decompression for desktop — support DXT-compressed KTX2 files in texture loader
- [ ] Add BC6/BC7 decompression for HDR textures — support high-quality compressed formats
- [ ] Add ASTC support path for mobile — conditional compilation for mobile targets
- [ ] Add compressed texture upload to GPU — pass pre-compressed data through to `create_texture` without re-encoding
- [ ] Add build-time texture compression tool — offline compressor that converts PNG/TGA to KTX2 with appropriate format per platform

### Shader compilation
- [ ] Add `build.rs` or cargo xtask for offline shader compilation — walk `resources/shaders/`, compile each `.wgsl` to SPIR-V via naga
- [ ] Add compiled shader cache — embed or ship SPIR-V blobs alongside shaders, load directly at runtime instead of naga compile
- [ ] Add shader compilation validation in CI — ensure all shaders compile without errors on push

### Animation system
- [ ] Design `AnimationClip` asset — bone index, keyframe times, position/rotation/scale tracks, duration, loop flag
- [ ] Add `AnimationPlayer` component — holds active clip, playback time, speed, blending weight, derives `Component`
- [ ] Implement skeletal animation sampling — interpolate between keyframes (LERP for position/scale, SLERP for rotation)
- [ ] Add animation blending — blend two AnimationPlayer outputs (crossfade) on shared skeleton
- [ ] Design `AnimatorStateMachine` — states (clips), transitions (conditions, duration, exit time), parameters (bool, float triggers)
- [ ] Add `AnimatorComponent` — holds state machine instance, parameters, current state; updates AnimationPlayer each frame
- [ ] Add `AnimationSystem` (ECS System trait) — advance animation time, evaluate state machine transitions, sample clips, write skeleton pose

### Reflections
- [ ] Add planar reflection pass — render scene from reflected camera for flat reflective surfaces (water, mirrors)
- [ ] Integrate planar reflections into material system — bind reflection texture on materials with reflective property

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
- [x] Add file watcher for `.luau` scripts — detect changes in `resources/scripts/`, trigger recompile
- [x] Implement hot reload — recompile chunk, create new per-script environment, preserve scalar state from old env, swap instances
- [x] Harden VM sandboxing — initialize with `StdLib::ALL_SAFE` (no io/os/debug), configure interrupt watchdog for runaway scripts
- [x] Add `print`/`warn` bridges — route to `log::info!`/`log::warn!` in debug builds only

### Phase 5: Polish + events
- [x] Design gameplay event bus — `EventBus<T>` with `emit(event)` and `subscribe(handler)`; support typed events in katla_script via string-keyed bus
- [x] Implement script event bindings — `world:emit("event_name", table)` and `world:on_event("event_name", callback)` registering Luau functions as handlers
- [x] Add event delivery system — each frame, drain pending events from bus, dispatch to registered script callbacks; ensure delivery order is deterministic
- [ ] Add physics bindings — `world:raycast(origin, direction, max_distance)` returning hit entity + point + normal (depends on Physics Phase 4)
- [ ] Add audio bindings — `world:play_sound("explosion")`, `world:play_sound_at("explosion", position)` (depends on Audio Phase 2)
- [ ] Performance profile — benchmark 1000 script entities with on_update, optimize hot paths
- [x] Optimize script dispatch — batch entity queries, reduce per-hook overhead, consider JIT hints

### Phase 6: Editor integration
- [ ] Add script inspector panel — show attached script path, expose script variables for live editing
- [ ] Add script file browser — show `.luau` files in asset browser, drag-to-attach to entity
- [ ] Generate Luau type definition files (.d.luau) — autocomplete support for engine API in external editors
- [ ] Add script console — capture `print()` output in editor log panel

### Gameplay framework (independent of scripting)
- [ ] Design game state machine — states (Menu, Loading, Playing, Paused, Cutscene), transitions, enter/exit hooks
- [ ] Implement `GameState` enum and `GameStateMachine` — state stack (push/pop), transition hooks (`on_enter`, `on_exit`), per-state update dispatch
- [ ] Add `GameStateManager` as ECS resource — accessible by systems and scripts; systems query current state to conditionally run
- [ ] Design gameplay event system — `EventBus<E>` generic typed event bus for gameplay-level events (OnDamage, OnCollect, OnCollision, etc.) decoupled from ECS events
- [ ] Implement `EventBus` — `emit(event)`, `subscribe(handler)`, `drain()` per frame; type-erased storage for multiple event types
- [ ] Wire gameplay events into ECS — collision events from physics, trigger events from trigger volumes, script events from Luau
- [ ] Design cutscene/timeline data model — `Timeline` asset with tracks (animation, audio, camera, event), keyframes per track, duration
- [ ] Implement timeline playback — `TimelinePlayer` component with play/pause/scrub, evaluate all tracks at current time, dispatch results
- [ ] Add timeline editor UI — track lanes, keyframe diamonds, scrubber bar, playback controls (depends on Editor dockable layout)

## Asset Pipeline

### AI Agent — Asset & Script Tools

- [x] Add `list_assets` tool to AI agent — list files in `resources/` recursively, with optional extension filter (`"luau"`, `"gltf"`) and subdir filter (`"scripts"`). Lets the AI discover available scripts and assets.
- [x] Add `read_asset` tool to AI agent — read file contents from `resources/` by relative path. Lets the AI inspect existing scripts, materials, etc.
- [x] Add `write_asset` tool to AI agent — create or overwrite files in `resources/` with given path and content. Enables full workflow: AI creates a script, adds ScriptComponent, sets the path, script is ready to run.
- [x] Add `delete_asset` tool to AI agent — delete files from `resources/` by relative path. Should refuse to delete non-empty directories.

### General asset pipeline

#### Hot reload
- [ ] Integrate `notify` crate for file watching — watch `shaders/` and `resources/` directories recursively for file changes
- [ ] Add file change event routing — map changed file paths to asset types (shader -> recompile material, texture -> reload, script -> hot reload)
- [ ] Implement shader hot reload — detect `.wgsl` changes, recompile material pipeline, swap in on next frame
- [ ] Implement texture hot reload — detect image changes, re-upload texture data to GPU, keep same bindless slot

#### Asset bundling
- [ ] Design asset bundle format — header (magic, version, file table), compressed entries, support random access for large assets
- [ ] Implement bundle packer tool — walk `resources/`, compress entries, write bundle file; as cargo xtask or build script
- [ ] Implement bundle reader — `BundleFs` implementing virtual filesystem interface, mount bundle at runtime
- [ ] Add release build integration — automatically bundle resources in release mode, fall back to filesystem in debug

#### Serialization improvements
- [ ] Add component serialization registry — `SerializationRegistry` mapping type IDs to serialize/deserialize closures; auto-register on component registration
- [ ] Implement generic scene serializer — walk entity hierarchy, look up each component type in registry, emit RON dynamically
- [ ] Implement generic scene deserializer — parse RON, look up component types by name in registry, construct components dynamically
- [ ] Add binary serialization option — `bincode` format alongside RON; selector based on file extension (`.scene` vs `.bscene`)
- [ ] Benchmark binary vs RON load times — verify bincode is meaningfully faster before committing to dual format

#### Editor integration
- [ ] Add native file dialogs — integrate `rfd` for Open Scene, Save Scene As, Import Asset dialogs
- [ ] Add asset import pipeline — convert source formats (FBX, PSD, TGA) to engine formats (glTF, PNG) as a preprocessing step
- [ ] Add import manifest — track source-to-engine format mappings, re-import when source changes

## Release & Deployment

- [x] Add CI/CD pipeline — GitHub Actions for build, test, clippy, fmt on push; artifact upload for release builds

### macOS packaging
- [ ] Generate macOS `.app` bundle structure — `Contents/MacOS/` binary, `Contents/Resources/` assets, `Info.plist` with app metadata
- [ ] Create app icon — `.icns` file from logo, reference in Info.plist
- [ ] Embed MoltenVK runtime — bundle MoltenVK dylib so users don't need Vulkan SDK installed
- [ ] Package as `.dmg` — create DMG with background image, Applications symlink, drag-to-install UX
- [ ] Add `cargo xtask bundle` command — automate the entire .app + .dmg generation pipeline

### Windows packaging
- [ ] Add Windows CI runner — GitHub Actions windows-latest runner, build release binary
- [ ] Bundle Vulkan runtime — detect/install Vulkan runtime as part of installer or bundle loader library
- [ ] Create Windows installer — WiX or NSIS installer with start menu shortcut, uninstaller
- [ ] Add `.exe` icon embedding — embed application icon in the binary resource section

### Linux packaging
- [ ] Add Linux CI runner — GitHub Actions ubuntu-latest runner, build release binary
- [ ] Create AppImage package — bundle binary + resources + Vulkan loader into portable AppImage
- [ ] Test Vulkan/ABI compatibility — verify binary runs on Ubuntu 22.04+ and common distros

### Signing and security
- [ ] Add macOS Developer ID signing — sign `.app` with Developer ID certificate, codesign all bundled frameworks
- [ ] Add macOS notarization — submit signed app to Apple notary service, staple ticket to DMG
- [ ] Add Windows code signing — sign `.exe` and installer with EV code signing certificate
- [ ] Set up signing secrets in CI — store certificates and passwords as GitHub Actions secrets

### Runtime systems
- [ ] Design save-game data model — what to persist (player progress, settings, unlocks), versioning, backward compatibility
- [ ] Implement `SaveGame` struct and serialization — JSON or bincode, user-writable save directory, load/save API
- [ ] Add save-game slots — support multiple save slots, slot selection UI in menu
- [ ] Add release mode resource embedding — `include_dir!` or `include_bytes!` for critical assets (shaders, default textures, fonts) into binary for zero-dependency startup
- [ ] Add embed-vs-filesystem fallback — release builds load from embedded, debug builds load from filesystem for hot reload

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

#### Timeline/animation editor
- [ ] Design timeline data model — `Timeline` asset with multiple tracks (bone animation, float curves, events), keyframes per track
- [ ] Implement timeline UI layout — horizontal time ruler, track lanes, keyframe diamonds, scrubber/playhead
- [ ] Add keyframe editing — click to add keyframe, drag to move, double-click to edit value, delete key
- [ ] Add curve editor — tangent handles for interpolation mode (linear, bezier, step), mini graph per track
- [ ] Add playback controls — play/pause, loop toggle, speed control, scrub-to-time
- [ ] Wire timeline to AnimationPlayer — preview animations in viewport while scrubbing

#### Material editor
- [ ] Design material editor layout — texture slots (albedo, normal, metallic, roughness, emission), numeric sliders, live preview
- [ ] Add texture slot widgets — drag-and-drop from asset browser, thumbnail preview, clear button
- [ ] Add PBR property sliders — metallic (0-1), roughness (0-1), emission color/intensity
- [ ] Add live material preview — apply changes in real-time to selected entity in viewport
- [ ] Add material serialization — save edited material back to .mat file

#### Terrain editor
- [ ] Design terrain component — `TerrainComponent` with heightmap, layer count, grid resolution
- [ ] Implement heightmap painting — raise/lower/flatten/smooth brushes with adjustable radius and strength
- [ ] Add terrain layer blending — paint blend weights for multiple material layers (grass, rock, dirt)
- [ ] Add foliage scattering — scatter mesh instances on terrain surface with density/rule parameters
- [ ] Add terrain mesh generation — generate LOD mesh from heightmap with configurable tessellation

#### Undo history panel
- [ ] Implement undo stack UI — list of operation names with timestamps, current position highlighted
- [ ] Add click-to-jump — click any entry in the stack to undo/redo to that point
- [ ] Add undo stack visualization — show branch points when redo stack is discarded by new operation

#### Dockable layout system
- [ ] Complete `DockLayout` skeleton — implement dock node tree (split, tab, leaf) with serialization
- [ ] Implement tab dragging — drag tab from one dock area to another, show drop preview overlay
- [ ] Implement split/dock gestures — drag to edge of panel to split, drag to tab bar to tab
- [ ] Add layout persistence — save/restore dock layout to disk on app shutdown/startup
- [ ] Convert existing panels to dockable — migrate scene hierarchy, inspector, asset browser, console to dock system

#### Profiler overlay
- [ ] Add GPU timestamp queries — insert timestamp queries at render pass boundaries in frame graph
- [ ] Collect per-pass timing data — store pass name + duration in a frame timing buffer
- [ ] Add profiler overlay UI — floating panel with frame time graph (sparkline), per-pass timing bars, FPS counter
- [ ] Add memory tracking — track GPU allocation counts and total bytes per resource type
- [ ] Add draw call counter — increment per draw call, display in profiler overlay

#### Gamepad input
- [ ] Add gamepad crate dependency — `gilrs` for cross-platform gamepad support
- [ ] Implement `GamepadInput` resource — poll connected gamepads each frame, read axis/button state
- [ ] Extend `InputMapper` with gamepad bindings — map gamepad axes/buttons to logical actions alongside keyboard/mouse
- [ ] Add gamepad to scripting bindings — expose `world:is_gamepad_pressed()`, `world:get_gamepad_axis()` to Luau

- [x] Add console/output log panel — capture log output in editor, filter by level, search
- [x] Fix asset browser tooltip line spacing — hover tooltip on asset items has inconsistent line spacing compared to the rest of the UI
- [x] Fix text input selection/active highlight being too opaque — the "Filter" input in asset browser and "Script" path input have a selection color that's too bright/invasive, obscuring the text. Investigate if transparency isn't rendering correctly. Should be fixed in a reusable text input style so all text inputs benefit.

## Developer Experience

### Documentation
- [ ] Write getting-started tutorial — step-by-step guide: create entity, add components, write a system, load a model, make something interactive
- [ ] Write component and system catalog — reference docs for all built-in components, systems, and their fields
- [ ] Write rendering pipeline overview — explain render graph passes, how to add custom passes, material system
- [ ] Write scripting guide — Luau API reference, hook lifecycle, world access patterns, example scripts

### Example game
- [ ] Design example game scope — simple 3D game: player movement, collectibles, score, win/lose state
- [ ] Implement player controller — WASD movement via script or system, camera follow, basic collision with world
- [ ] Add collectible items — spawn entities with trigger volumes, detect overlap, increment score
- [ ] Add game state — start screen, playing, win/lose; display score and UI overlay
- [ ] Package as runnable example — `cargo run --example game` or `game/` crate with its own main

### Profiling and instrumentation
- [ ] Add Tracy integration — conditional Tracy profiler markers on render passes and ECS systems (behind `tracy` feature flag)
- [ ] Add GPU timestamp queries — insert timestamp queries at render pass boundaries, collect per-pass durations
- [ ] Add frame timing display — render frame time graph and FPS counter in status bar or overlay
- [ ] Add system timing — measure ECS system execution time, display in debug overlay

### Testing
- [ ] Design integration test framework — headless app init, entity spawning, frame execution, state assertions
- [ ] Add render test infrastructure — render N frames, read back pixels, compare against golden images
- [ ] Add ECS round-trip tests — spawn entity, add components, serialize, deserialize, verify equivalence
- [x] Add scripting integration tests — load script, call on_update, verify world mutations via command queue
- [ ] Add headless CI test suite — run integration tests without GPU in CI (mock renderer or software rasterizer)

- [x] Fix AppError::Graphics to carry typed RendererError instead of String — preserve error chain for debugging
