# TODO

## Task Sizing Convention

Individual tasks should be small enough to complete in a single focused session. For large features (new subsystems, architectural changes, cross-cutting refactors), the TODO item is scoped as **exploration, ideation, and architecture** — research patterns, evaluate alternatives, and produce a concrete implementation plan as smaller TODO items. The output of such a task is a breakdown, not working code.

## Backend Abstraction Cleanup

### B. Design backend-agnostic texture view type

- [x] **Explore backend-agnostic texture view type** — Evaluated three approaches: (1) wgpu-hal-style Api trait with associated types (already used internally, doesn't solve the AnyFrameGraph boundary), (2) enum wrapper `AnyTextureView` with `Vulkan(VkImageView)` / `Metal(MetalTextureView)` variants, (3) `dyn Trait` object (requires new non-generic trait, overengineered). **Decision: enum wrapper** — consistent with existing `AnyRenderer`/`AnyFrameGraph` enum dispatch pattern, no architectural changes needed.

- [ ] Create `AnyTextureView` enum in `katla_gfx/src/render_graph/any_texture_view.rs` — `Vulkan(VkImageView)` and `Metal(MetalTextureView)` variants, `Send + Sync`, expose basic accessors (format, dimensions) via the existing `GpuImageView` trait or simple delegated methods
- [ ] Add `transient_image_view(name, frame_idx) -> Option<AnyTextureView>` to `AnyFrameGraph` — replaces `transient_image_view_metal()`
- [ ] Add `transient_texture(name, frame_idx) -> Option<&AnyTransientTexture>` to `AnyFrameGraph` — replaces `transient_texture_metal()`
- [ ] Remove `transient_image_view_metal()` and `transient_texture_metal()` Metal-only methods from `AnyFrameGraph`
- [ ] Update `AnyRenderer` to add `set_geometry_hdr_view` / `set_tonemap_output_view` taking `AnyTextureView` — dispatch to Vulkan (no-op or forward) / Metal (unwrap and call concrete method)
- [ ] Update `katla_app` callers (`builder.rs`, `renderer.rs`) to use new backend-agnostic methods instead of Metal-specific ones
- [ ] Remove `#[cfg(target_os = "macos")]` gates from `AnyFrameGraph` / `AnyFrame` that are now handled by the enum variants

### C. Unify pipeline initialization — eliminate Metal-specific methods on AnyRenderer

- [ ] ~~Add `set_geometry_hdr_view` and `set_tonemap_output_view` to `GpuRenderer` trait~~ — superseded by B items above; the enum wrapper approach keeps these on `AnyRenderer` rather than the trait

### E. Align Metal backend with shared FrameGraph<B> execution path

- [ ] Add particles pass dispatch through `RenderGraphBackend` on Metal
- [ ] Add compositing pass dispatch through `RenderGraphBackend` on Metal
- [ ] Add stencil-indicator pass dispatch through `RenderGraphBackend` on Metal
- [ ] Add generic compute pass dispatch through `RenderGraphBackend` on Metal
- [ ] Refactor Metal `collect_draw_lists()` to produce `FrameGraph<MetalRenderer>` nodes instead of a hardcoded list
- [ ] Wire Metal `render_frame()` through `FrameGraph<MetalRenderer>::execute()` instead of the hardcoded pass sequence
- [ ] Remove the Metal-specific hardcoded pass execution path once data-driven graph execution is working

## Audio System

### Phase 14: Production bugs and correctness

- [ ] **Explore background decode thread design** — Research threading model for audio streaming: thread ownership of `StreamingDecoder`, ring buffer sizing, synchronization primitives, and shutdown coordination. Produce concrete implementation TODO items.
- [ ] Refactor `StreamingVoice::fill_ring_buffer()` to consume from the pre-filled ring buffer without performing I/O
- [ ] Wire background decode thread lifecycle (start/stop) into `AudioEngine` init/shutdown

### Phase 15: Audio quality and robustness
- [x] Add automatic fade-in/fade-out on voice start/stop — voices currently start and stop instantly with no gain ramp, causing audible clicks/pops. Add a short (1-5ms) linear fade-in when a voice begins playback and a fade-out when stopped, before marking it finished. This is standard practice in all production audio engines (Kira, FMOD, Wwise).
 - [x] Add configurable tween duration — `Voice::tween_smoothing` and `StreamingVoice::tween_smoothing` are hardcoded to 0.3 with no API to change them. Kira uses time-based tweens (e.g., `Tween { duration: 200ms }`). Expose tween duration or speed as a parameter on `VoiceHandle::set_volume_tweened()` etc.
- [x] Add per-voice aux send levels — aux buses currently accumulate a copy of the entire main mix at a fixed `send_level`. Production audio engines allow each voice to have its own send level to each aux bus (e.g., a specific SFX sends 50% to reverb while music sends 0%). Add a `sends: Vec<(AuxBusId, f32)>` field to `Voice` and `StreamingVoice`.
- [x] Add voice steal/priority system — there is no limit on the number of simultaneous voices. With enough concurrent sounds, the mix saturates and quality degrades. Add a maximum voice count and a priority-based voice stealing mechanism (lowest priority voice is stopped to make room for a new one).
- [x] Add voice pooling — voices are allocated and deallocated every time a sound plays/stops, causing allocation pressure in the audio thread's Mutex. Pre-allocate a fixed pool of Voice objects and reuse slots.
- [x] Improve resampling quality — both `Voice` and `StreamingVoice` use linear interpolation for sample rate conversion and pitch shifting. For production quality, add at least cubic (Catmull-Rom) interpolation, optionally sinc for offline/bounce. Linear interpolation causes audible artifacts with high-frequency content.
- [x] Add proper reverb stereo decorrelation — `ReverbEffect` processes a mono sum of the input and applies the same mono reverb to both channels, collapsing stereo image. Use separate delay lines for left/right with slightly different delay times, or process L/R independently with decorrelation filters.
- [x] Add audio device hot-swap — Added poll_device_change() with cpal device ID tracking, rebuilds stream on device change.
 - [x] Add silence detection for streaming voices — `StreamingVoice::mix_into()` processes the full output buffer even when volume is 0.0 (only skips when `voice_volume == 0.0`, but tweening can make this check imprecise). Add an early-out when the voice has been silent for multiple consecutive frames.

### Phase 16: Feature parity with production audio engines
- [x] **Explore audio clock/timeline architecture** — Research sample-accurate scheduling APIs from production engines (Kira, FMOD, Wwise), evaluate clock design (position counter, scheduling queue, voice integration), and produce concrete implementation TODO items.
- [x] Add audio file metadata query — no way to query duration, sample rate, or channel count of an audio file without fully decoding it. Add `AudioBuffer::from_path_metadata()` or similar that reads headers only (WAV fmt chunk, OGG/MP3 frame headers) without decoding the entire file. Needed for the asset browser duration display.
- [x] Add looping crossfade support — seamless loop transitions currently just jump from `loop_end` to `loop_start`, which can cause clicks if the waveform doesn't align. Add a short crossfade region at the loop point (mix the tail of the loop with the head of the next iteration).
- [x] Add playback position query — no way to query the current playback position of a voice (in seconds or samples). Add `VoiceHandle::position() -> f32` and `StreamingVoiceHandle::position() -> f32` for UI scrub bars, subtitle sync, and gameplay triggers.
- [x] Add seek API for streaming voices — `StreamingVoiceHandle` has no seek method. Add `StreamingVoiceHandle::seek(position: Duration)` to allow scrubbing to arbitrary positions in a streaming file.
- [ ] **Explore audio recording/bounce design** — Research offline render patterns: capturing the final mix to WAV, non-realtime rendering, cutscene bounce workflows. Produce concrete implementation TODO items.

### Phase 17: Audio system activation and global settings
- [x] Add AudioSettings to Preferences — Added AudioSettings struct with master/sfx/music/ambient volumes, defaults to 1.0.
- [x] Add Audio tab to preferences panel — Added Audio tab with master/SFX/music/ambient volume sliders with live preview.
- [x] Apply saved audio settings on startup — Calls set_master_volume and set_category_volume after AudioSystem init.
- [x] Add AudioSource inspector UI — Read-only section with path, sample rate, channels, duration, and Play Preview button.
- [x] Add AudioListener indicator in inspector — Shows active listener entity, warns if multiple AudioListener components exist.

### Phase 18: Audio mixer UI
- [x] Add peak/RMS level computation in `AudioMixer::render()` — Per-category and master peak/RMS computed during render, written to LevelsBuffer.
- [x] Add atomic double-buffered level snapshots — LevelsBuffer with AtomicUsize index, fetch_xor(1) swap, lock-free audio→UI communication.
- [x] Add VU meter widget to katla_ui — Added VuMeter ViewDescriptor with color-graded RMS bar and peak hold indicator.
- [x] Add mixer panel layout — dockable panel with master bus fader + VU meter, SFX/Music/Ambient sub-buses with faders + VU meters, aux bus sends with wet/dry controls
- [x] Add voice pool status display — show active voice count, peak voice count, and which voices are playing (with name/category/volume) in the mixer panel or a debug overlay
- [x] Add reverb zone visualizer — `ReverbZone` components exist but are invisible in the editor. Draw wireframe boxes/spheres showing reverb zone extents with color-coding for decay/wet parameters, similar to physics collider visualization.

## Physics

### Phase 5.5: Physics production readiness

- [x] **Fix `editor` feature cfg warnings in katla_physics** — katla_physics uses Component derive from katla_derive which has `#[cfg(feature = "editor")]` but katla_physics doesn't declare this feature. Add `editor = []` feature to katla_physics/Cargo.toml to resolve 5 warnings in collider.rs, joint.rs, material.rs, rigid_body.rs, trigger.rs.
- [x] **Fix clippy::too_many_arguments warning** — `create_body_ex` in physics_world.rs has 9 arguments. Either add `#[allow(clippy::too_many_arguments)]` or refactor to use a builder pattern with `BodyBuilder` struct.
- [x] **Make gravity configurable** — `PhysicsWorld::new()` hardcodes gravity as `Vector::new(0.0, -9.81, 0.0)`. Add `PhysicsWorld::with_gravity(gravity: Vec3)` constructor or `set_gravity(&mut self, gravity: Vec3)` method.
- [x] **Add `PhysicsError` enum for explicit error handling** — PhysicsWorld uses `Option` for fallible operations (body_transform, body_velocity). Add a `PhysicsError` enum with variants like `BodyNotFound`, `ColliderNotFound`, `InvalidHandle` and return `Result<T, PhysicsError>` from methods where appropriate.
- [x] **Expose CCD configuration** — Rapier supports Continuous Collision Detection for fast-moving bodies but it's not exposed in katla_physics. Add CCD enable/disable parameter to body creation methods or as a global PhysicsWorld setting.
- [ ] **Explore character controller design** — Evaluate Rapier's `KinematicCharacterController` API, research common patterns (slope handling, stairs, step-assist, jump), design `CharacterController` component fields and system integration, produce concrete implementation TODO items.

### Phase 6: Physics component scene serialization

- [x] **Add `RigidBodyDescriptor`** — enum with Static, Dynamic, Kinematic variants; added to EntityDescriptor
- [x] **Add `ColliderShapeDescriptor`** — enum with Sphere(radius), Box(half_extents), Capsule { half_height, radius } variants; added to EntityDescriptor
- [x] **Add `PhysicsMaterialDescriptor`** — struct with friction, restitution, density fields; added to EntityDescriptor
- [x] **Add `TriggerVolumeDescriptor` and `CollisionFilterDescriptor`** — trigger volume as unit struct; collision filter with layers/mask; added both to EntityDescriptor
- [x] **Implement save path for Rapier physics components** — Reads all 5 physics components from ECS entities, converts to descriptor types, skips runtime-only fields.
- [x] **Implement load path for Rapier physics components** — Creates ECS components from physics descriptors and adds them to spawned entities.
- [x] **Remove hardcoded `spawn_physics_demo_objects()`** — Removed function and call site; physics entities now serialize to scene files.
- [x] **Add physics entities to default.katla scene** — Static ground box (10x0.05x10 half-extents, friction 0.7), 10 dynamic grid spheres (top 2 rows of 5x5, radius 0.4, friction 0.5, restitution 0.3), dynamic CenterCube (box [0.5,0.5,0.5]), dynamic CyanSphere (sphere 0.7), dynamic MagentaCylinder (capsule half_height 0.375 radius 0.5). LimeTorus stays visual (torus trimesh is separate TODO). PhysicsActive(true) at builder init so the demo plays on load.

### Phase 7: Physics entity lifecycle

 - [x] **Handle entity destruction for joints** — Same issue: joints referencing destroyed entities leak Rapier joint handles. Add cleanup for `Joint` components whose `entity_a` or `entity_b` no longer exist.
- [x] **Add entity despawn callback for physics** — When the editor removes a `RigidBody` or `ColliderShape` component from an entity, the corresponding Rapier handles should be cleaned up. Wire into the existing `EditorAction::RemoveComponent` handler.

### Phase 8: Collider mesh fitting, shape types, and prefabs

- [x] **Add geometry data cache for mesh vertex positions** — Added GeometryCache (HashMap<MeshHandle, Arc<MeshGeometryData>>) populated during mesh loading.
- [x] **Extend `ColliderShape` enum with mesh-derived variants** — Added Trimesh(MeshHandle), ConvexHull(MeshHandle), Heightfield(HeightfieldShape) with serde support.
- [x] **Wire new `ColliderShape` variants through `collider_shape_to_rapier()`** — Trimesh/ConvexHull use MeshColliderData from geometry cache, Heightfield uses inline data.
- [ ] **Implement trimesh collider generation for static environment meshes** — For static environment geometry (floors, walls, level architecture), generate exact trimesh colliders from the mesh's vertex/index data. Add an editor action or auto-detection: when a static `RigidBody` entity has a mesh, default to trimesh collider. Trimesh colliders only work with static bodies in Rapier.
- [ ] **Implement convex hull collider generation for dynamic props** — For dynamic/kinematic objects with complex meshes, compute a convex hull from vertex positions using Rapier's `SharedShape::convex_hull`. Convex hulls support dynamic simulation (unlike trimesh) but are approximate — they enclose the mesh but may have gaps. Add editor action to convert a mesh entity's collider to convex hull.
- [ ] **Implement capsule auto-fit from mesh dimensions** — Capsule colliders are ideal for character-like objects (humanoids, pillars, barrels). When auto-fitting a collider, compute the mesh AABB and check if it is tall and narrow (height > 2 × width). If so, generate a `CapsuleShape { half_height: height/2 - radius, radius: width/2 }` instead of a box. Add capsule as an explicit option in the editor collider type picker so users can override auto-fit.
- [ ] **Implement best-fit shape selection logic** — When auto-generating a collider for a mesh entity, choose the best shape type based on mesh characteristics: (a) sphere if AABB is roughly cubic and small, (b) capsule if tall/narrow (height > 2 × width), (c) box for general shapes, (d) convex hull for complex dynamic props, (e) trimesh for static environment geometry. This replaces the current box-only auto-fit.
- [ ] **Explore collider cache design** — Computing convex hulls/trimesh colliders from mesh data is expensive. Research caching strategies: key by mesh handle + shape type, reuse across entities sharing the same mesh, invalidation on mesh hot-reload. Produce concrete implementation TODO items.
- [ ] **Update editor collider type picker UI** — The editor inspector for `ColliderShape` currently shows Sphere/Box/Capsule dropdown. Extend to show all shape types: Sphere, Box, Capsule, Trimesh, ConvexHull, Heightfield. When switching type, reset to auto-fit dimensions from the entity's mesh bounds. Disable Trimesh for non-static bodies (Rapier constraint). Disable Heightfield for non-mesh entities.
- [ ] **Explore prefab system design** — Research ECS prefab patterns (component bundles, template entities, nested prefabs). Evaluate how other engines handle prefab instantiation and overrides. Produce concrete implementation TODO items.
- [ ] **Add physics entity spawn from asset browser** — Drag a physics prefab or mesh from the asset browser into the viewport to spawn an entity with auto-fitted collider + rigid body + default material.

### Phase 9: Physics robustness and testing

 - [x] **Add test for static body spawn tracking** — Verify that static bodies correctly track their spawned state despite having no Rapier `RigidBodyHandle` (related to the invalid-handle fix in Phase 5).
 - [x] **Add test for entity destruction cleanup** — Spawn a dynamic body, destroy the entity, verify that `PhysicsWorld` body/collider counts decrease correctly.
 - [x] **Add test for joint spawning** — Create two entities with `RigidBody` + `ColliderShape`, add a `Joint` component referencing both, run one frame, verify the joint is created in `PhysicsWorld`.
 - ~~**Add test for play-mode gating**~~ — Already covered by `test_play_mode_gating` (PhysicsActive false) and `test_gravity_affects_dynamic` (PhysicsActive true).
 - ~~**Add integration test for physics scene round-trip**~~ — Blocked on Phase 6: physics components have no scene serialization support yet.
 - [x] **Add test for kinematic body sync** — Spawn a kinematic body, move its `TransformComponent`, run one frame, verify Rapier body position matches the new transform.
 - [x] **Add stress test for many dynamic bodies** — Spawn 100+ dynamic bodies, step for N frames, verify no panics or deadlocks. Identify performance bottlenecks in the spawn/sync loop.
 - [x] **Add test for `apply_force` and `apply_impulse` through ECS** — Current tests only verify body creation and gravity. Add tests that apply forces/impulses and verify velocity/position changes.

### Phase 10: Physics scripting polish

- [x] **Expose `apply_force` / `apply_impulse` to Luau scripts** — Scripts can raycast but cannot apply forces or impulses to physics bodies. Add `world:apply_force(entity_id, force: Vec3)` and `world:apply_impulse(entity_id, impulse: Vec3)` script bindings.
- [x] **Expose body velocity read/write to scripts** — Add `world:get_velocity(entity_id) -> Vec3` and `world:set_velocity(entity_id, velocity: Vec3)` for script-driven physics control.
- [x] **Expose trigger volume queries to scripts** — Scripts should be able to check if an entity with a `TriggerVolume` is currently overlapping with specific entities, not just receive enter/exit events.
- [x] **Add physics collision event scripting** — Already wired. Fixed one-frame delay in event dispatch order.

## Rendering

### Metal rendering bugs
- [x] Billboard icons don't show in Metal
- [ ] **Investigate animated fox (skinned mesh) not showing in Metal** — Determine root cause (missing joint buffer bind, shader mismatch, pipeline state) before scoping fix. Could be trivial or require significant plumbing.
- [ ] **Investigate particle systems not showing in Metal** — Determine root cause (compute dispatch path, particle buffer upload, draw call) before scoping fix. Could be trivial or require significant plumbing.

### Post-processing pipeline
- [ ] Add post-process pass infrastructure — reusable fullscreen-quad pass builder in the render graph that takes an input color texture and outputs a processed color texture
- [ ] Add FXAA pass
  - [ ] Write `fxaa.wgsl` shader — luminance edge detection, sub-pixel blending
  - [ ] Wire FXAA pass into render graph after tonemapping
- [ ] Add bloom pass
  - [ ] Add bright extraction threshold pass — extract pixels above luminance threshold at full resolution
  - [ ] Add two-pass gaussian blur — horizontal + vertical blur at half resolution
  - [ ] Add additive compositing pass — blend blurred bloom onto scene output
  - [ ] Write bloom shader(s)
- [ ] Add motion blur pass
  - [ ] Generate per-pixel velocity buffer from depth + camera motion
  - [ ] Implement tile-max velocity computation
  - [ ] Write `motion_blur.wgsl` — blur in motion direction using tile-max
- [ ] Add depth of field pass
  - [ ] Compute circle-of-confusion from depth and camera focus settings
  - [ ] Implement separate near/far bokeh blur passes
  - [ ] Add compositing pass — blend near/far bokeh with in-focus image
  - [ ] Write `dof.wgsl`

### Screen-space effects
- [ ] Add SSAO pass
  - [ ] Generate depth + normal buffer (G-buffer extension or dedicated pass)
  - [ ] Implement hemisphere sampling kernel in `ssao.wgsl`
  - [ ] Add bilateral blur pass to denoise SSAO output
  - [ ] Integrate SSAO texture into PBR lighting shader as ambient occlusion term
- [ ] Add SSR pass
  - [ ] Implement depth buffer ray-marching from reflected fragments in `ssr.wgsl`
  - [ ] Add distance/edge fade and temporal accumulation for stability
  - [ ] Wire SSR output into lighting/compositing pass

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
- [ ] **Explore planar reflection architecture** — Research reflection rendering techniques (reflected camera, oblique clipping, mirror textures). Produce concrete implementation TODO items.
- [ ] Integrate planar reflections into material system — bind reflection texture on materials with reflective property

## Scripting & Game Logic

### Gameplay framework
- [ ] **Explore game state machine architecture** — Research state stack patterns (push/pop, transition hooks, per-state update dispatch). Evaluate how Bevy/Unity/Unreal handle game states. Produce concrete implementation TODO items for `GameState` enum, `GameStateMachine`, and `GameStateManager` ECS resource.
- [ ] **Explore gameplay event system design** — Research typed event bus patterns (`EventBus<E>`) for gameplay events (OnDamage, OnCollect, OnCollision). Evaluate type-erased storage, subscription models, frame-scoped vs persistent events. Produce concrete implementation TODO items.
- [ ] **Explore cutscene/timeline data model** — Research timeline asset design (tracks, keyframes, duration), playback engine patterns (play/pause/scrub, multi-track evaluation), and editor UI approaches (track lanes, keyframe diamonds, scrubber). Produce concrete implementation TODO items.

## Asset Pipeline

#### Asset bundling
- [ ] **Explore asset bundle format and tooling** — Research virtual filesystem patterns (header + compressed entries, random access), evaluate existing Rust VFS crates, design packer tool and runtime reader. Produce concrete implementation TODO items covering bundle format, packer, reader, and release integration.

#### Serialization improvements
- [ ] **Explore component serialization registry design** — Research type-erased serialization patterns (type ID to serialize/deserialize closures), evaluate RON vs bincode tradeoffs, design the generic scene serializer/deserializer architecture. Produce concrete implementation TODO items.

#### Editor integration
- [ ] Add native file dialogs — integrate `rfd` for Open Scene, Save Scene As, Import Asset dialogs
- [ ] **Explore asset import pipeline design** — Research format conversion workflows (FBX→glTF, PSD→PNG, TGA→PNG), evaluate existing Rust libraries for each format, design the import manifest and re-import trigger system. Produce concrete implementation TODO items.
- [ ] Add import manifest — track source-to-engine format mappings, re-import when source changes

## Release & Deployment

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

### Panels and tooling

#### Asset browser context windows
- [ ] **Explore asset browser context window architecture** — Research floating window patterns for different asset types (model preview, material preview, code editor, image viewer, audio player). Evaluate shared window shell vs per-type panels. Produce concrete implementation TODO items.

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
- [ ] **Explore heightmap painting architecture** — Research brush systems for terrain editing (raise/lower/flatten/smooth), evaluate GPU vs CPU brush application, design brush parameter UI. Produce concrete implementation TODO items.
- [ ] Add terrain layer blending — paint blend weights for multiple material layers (grass, rock, dirt)
- [ ] **Explore foliage scattering design** — Research instanced mesh scattering on terrain (density rules, slope filtering, collision avoidance). Produce concrete implementation TODO items.
- [ ] **Explore terrain mesh generation approaches** — Research LOD mesh generation from heightmaps (tessellation levels, seam stitching, chunk boundaries). Produce concrete implementation TODO items.

#### Undo history panel
- [ ] Implement undo stack UI — list of operation names with timestamps, current position highlighted
- [ ] Add click-to-jump — click any entry in the stack to undo/redo to that point
- [ ] Add undo stack visualization — show branch points when redo stack is discarded by new operation

#### Dockable layout system
- [ ] **Explore dockable layout architecture** — Research dock node tree patterns (split, tab, leaf), serialization, drag-and-drop panel docking, and layout persistence. Evaluate existing Rust docking libraries (egui dock, etc). Produce concrete implementation TODO items covering: tree structure, tab dragging, split gestures, layout persistence, and panel migration.

#### Profiler overlay
- [ ] Add GPU timestamp queries — insert timestamp queries at render pass boundaries in frame graph
- [ ] Collect per-pass timing data — store pass name + duration in a frame timing buffer
- [ ] Add profiler overlay UI — floating panel with frame time graph (sparkline), per-pass timing bars, FPS counter
- [ ] Add memory tracking — track GPU allocation counts and total bytes per resource type

#### Gamepad input
- [ ] Add gamepad crate dependency — `gilrs` for cross-platform gamepad support
- [ ] Implement `GamepadInput` resource — poll connected gamepads each frame, read axis/button state
- [ ] Extend `InputMapper` with gamepad bindings — map gamepad axes/buttons to logical actions alongside keyboard/mouse
- [ ] Add gamepad to scripting bindings — expose `world:is_gamepad_pressed()`, `world:get_gamepad_axis()` to Luau

### Declarative UI migration

#### Prerequisites: ergonomic view constructors

No builder structs. Free functions return `ViewDescriptor` directly. Optional fields are set via modifier methods on `ViewDescriptor` that pattern-match the variant. Containers accept `impl IntoIterator<Item = ViewDescriptor>` so you pass arrays `[child1, child2]` instead of `vec![]`. Everything is one type, everything composes.

**Design:**

```
Free functions (all return ViewDescriptor)    Modifier methods (consume self, return Self)
─────────────────────────────────────────     ────────────────────────────────────────────
text(content)                                 .color(c) .font_size(fs)
button(label)                                 .fill(c) .hover(c) .border(c) .on_click(cb)
image_button(icon)                            .enabled(b) .fill(c) .on_click(cb)
slider(label, value_id, range)                .show_value(b) .precision(n)
labeled_slider(label, value_id, range)        .label_width(w) .show_value(b) .precision(n)
textfield(placeholder, value_id)              .on_submit(cb)
progress(value, range)                        .fill(c)
image(texture, tint)                          .uv(rect)
hstack(impl IntoIterator<Item = VD>)          .spacing(f) .padding(p) .padding_all(f) .align(a)
vstack(impl IntoIterator<Item = VD>)          (same)
zstack(impl IntoIterator<Item = (Align, VD)>) .padding(p)
panel(title, content)                         .header_height(f)
scroll(content, scroll_id)                    —
overlay(anchor, offset, content)              —
statusbar(height, content)                    —
modal(width, height, open_id, content)        —
context_menu(items, open_id)                  —
empty()                                       —
toggle(label, state_id)                       —
radio(value_id, index, label)                 —
property_row(label, value)                    —
color_picker(label, state_id)                 —
draggle_panel(t, w, h, content, state_id)     .close_on_outside(b)
menubar(groups)                               .right_content(vd) .height(f)
tree_view(items, exp_id, sel_id, scroll_id)   .row_height(f) .indent(f) .on_select(cb) .on_right_click(cb)
```

Modifier methods match on the variant — `.color()` sets `Text.color`, `.on_click()` sets `Button.on_click`, `.spacing()` sets `HStack`/`VStack` spacing. Misapplied modifiers (e.g. `.color()` on a `ScrollView`) are a no-op with a `debug_assert!` in test builds. This tradeoff is acceptable — same approach egui uses.

**Before/after:**

```rust
// TODAY — StatusBarView (6 lines per text, every None spelled out)
ViewDescriptor::Text {
    content: format!("FPS: {:.0}", data.fps),
    color: Some(fps_color),
    font_size: None,
}
ViewDescriptor::HStack(Box::new(StackDescriptor {
    children: left_items,
    spacing: 8.0,
    padding: Padding::all(4.0),
    alignment: Alignment::Center,
}))

// AFTER — same thing
text(format!("FPS: {:.0}", data.fps)).color(fps_color)
hstack(left_items).spacing(8.0).padding_all(4.0).align(Alignment::Center)
```

```rust
// TODAY — EditorRootView
ViewDescriptor::ZStack(Box::new(ZStackDescriptor {
    children: vec![
        (Alignment::TopLeading, viewport_grid),
        (Alignment::TopLeading, toolbar),
    ],
    padding: Padding::zero(),
}))

// AFTER — no vec![], no Padding::zero()
zstack([
    (Alignment::TopLeading, viewport_grid),
    (Alignment::TopLeading, toolbar),
])
```

```rust
// TODAY — GizmoButtonsView
ViewDescriptor::RadioButton { value_id: mode_id, index, label: label.to_string() }
ViewDescriptor::HStack(Box::new(StackDescriptor {
    children, spacing: 2.0, padding: Padding::all(10.0), alignment: Alignment::Leading,
}))

// AFTER
radio(mode_id, index, label)
hstack(children).spacing(2.0).padding_all(10.0)
```

**Implementation tasks:**

 - [x] Create `katla_ui/src/declarative/constructors.rs` — module for all free functions. Each is a plain `fn foo(...) -> ViewDescriptor` that fills defaults for optional fields. Start with the trivial ones: `empty()`, `toggle()`, `radio()`, `property_row()`, `color_picker()`.
- [x] Add leaf free functions with optionals — `text()`, `button()`, `image_button()`, `slider()`, `labeled_slider()`, `textfield()`, `progress()`, `image()`. All optional fields default to `None` / `false` / `0`. These work standalone even before modifier methods exist.
- [x] Add container free functions with `impl IntoIterator` — `hstack()`, `vstack()` take `impl IntoIterator<Item = ViewDescriptor>`, `zstack()` takes `impl IntoIterator<Item = (Alignment, ViewDescriptor)>`. Single-child containers (`scroll`, `panel`, `overlay`, `statusbar`) take `ViewDescriptor` directly. `draggle_panel`, `menubar`, `tree_view` take their specific struct args.
- [x] Add modifier methods on `ViewDescriptor` — implement in `descriptor.rs` as `impl ViewDescriptor`. Each method consumes `self`, pattern-matches to the relevant variant(s), updates the field, returns `self`. Start with: `.color()`, `.font_size()`, `.fill()`, `.hover()`, `.border()`, `.on_click()`, `.enabled()`, `.on_submit()`, `.show_value()`, `.precision()`, `.label_width()`, `.uv()`. Add `debug_assert!` in the else branch to catch misapplied modifiers in tests.
- [x] Add container modifier methods — `.spacing()`, `.padding()`, `.padding_all()`, `.align()` on `HStack`/`VStack`. `.padding()` on `ZStack`. `.header_height()` on `Panel`. `.close_on_outside()` on `DraggablePanel`. `.right_content()` and `.height()` on `MenuBar`. `.row_height()`, `.indent()`, `.on_select()`, `.on_right_click()` on `TreeView`.
- [x] Re-export everything from `declarative/mod.rs` — `pub use constructors::*` so users write `use katla_ui::declarative::{text, button, hstack};`.
- [x] Add unit tests — for each free function, verify it produces the correct `ViewDescriptor` variant with expected defaults. For each modifier, verify it sets the field and that misapplied modifiers no-op (test the `debug_assert!` fires).
- [x] Refactor `StatusBarView` — replace all struct-literal `ViewDescriptor::Text` and `ViewDescriptor::HStack` with `text().color()` and `hstack().spacing().padding_all().align()`. First real consumer, validates the end-to-end feel.
- [x] Refactor `GizmoButtonsView` — replace `RadioButton` struct literals with `radio()`, `HStack` with `hstack().spacing().padding_all()`.
- [x] Refactor `EditorRootView` — replace `ZStack` struct literal with `zstack([...])`.
- [x] Refactor `helpers.rs` — rewrite `section_header()`, `delete_button()` to use free functions + modifiers internally.

#### Prerequisites: layout and diffing infrastructure

- [x] Replace heuristic text measurement with real font metrics — `measure_text_descriptor()` currently uses `char_count * height * 0.6`. Use the existing `FontSystem` to measure actual glyph advances for the layout string, so Taffy flexbox sizes match what the renderer draws.
- [x] Add stable child identity for list diffing — add an optional `key: Option<u64>` to `StackDescriptor` children (or a `KeyedChild` wrapper) so diffing can match children by identity instead of index. Prevents state corruption and spurious animations when list order changes.

#### Widget gaps: missing declarative features needed for migration

- [x] Add `Section` descriptor — collapsible section with header row (label + optional remove button + expand/collapse chevron). Equivalent to the `section_header()` helper but as a proper container variant. Needed by Inspector.
- [x] Add `TabBar` descriptor — tab strip with selectable tabs, content area below. Equivalent to immediate-mode `begin_row` with styled buttons. Needed by Preferences.
- [x] Add `Grid` descriptor — `GridDescriptor { columns: usize, cell_size: Vec2, spacing: f32, children: Vec<ViewDescriptor> }` mapped to a wrapping Taffy flex layout. Needed by Preferences and Viewport Grid.
- [x] Add `Separator` descriptor — horizontal or vertical divider line. Needed by most panels.
- [x] Add `Icon` descriptor — render a ForkAwesome icon by char code with configurable size and color. Needed by Toolbar and Inspector.
- [x] Add `ProgressBar` improvements — add optional label text overlay on the progress bar track.
- [x] Add `Selectable` descriptor — wrapper that highlights on hover and fires on_click, for list items and grid cells. Needed by Asset Browser and Hierarchy.
- [x] Add `Conditional` descriptor or extend `show_if` helper — support `if`/`else` branching in descriptor trees with stable identity on both branches so diffing doesn't destroy state.
- [x] Add `Vec3Slider` free function constructor — descriptor exists but lacks a `vec3_slider(label, value_ids, range) -> ViewDescriptor` constructor function for use in Inspector and other panels.

#### Prerequisites: Environment injection pattern

- [x] Standardize Environment injection pattern across all declarative panels — Already complete. All 10 panels use ctx.env() pattern; no thread_local bridges remain.

#### Phase 1: Migrate simple panels (build confidence)

- [x] Migrate Viewport Grid panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `ViewportGridDrawCtx`, inject data via `Environment`, build a `Grid` or `VStack` of `Image` + `Text` cells with hit-testing via `Selectable` descriptors. Remove `set_viewport_grid_ctx`/`take_viewport_grid_ctx`.
- [x] Migrate Toolbar panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `ToolbarDrawCtx`, inject via `Environment`, build `MenuBar` with `MenuGroup` dropdowns and `ImageButton` descriptors. Remove `set_toolbar_ctx`/`take_toolbar_ctx`.
- [x] Migrate Gizmo panel fully declarative — already uses `RadioButton` descriptors but reads `GizmoDrawCtx` from `Environment` via thread-local. Move the gizmo data to `Environment` only, remove any thread-local remnants.

#### Phase 2: Migrate medium panels

- [x] Migrate Preferences panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `PreferencesDrawCtx`, inject via `Environment`. Use `TabBar` for General/Viewport/AI tabs, `Grid` for label+widget rows, `LabeledSlider`/`Toggle` for settings. Remove `set_preferences_ctx`/`take_preferences_ctx`.
- [x] Migrate Co-Creator panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `CoCreatorDrawCtx`. Use `DraggablePanel`, `ScrollView` with `Text` rows, `TextField` for input. Remove `set_co_creator_ctx`/`take_co_creator_ctx`.
- [x] Migrate Particle Inspector panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `ParticleInspectorDrawCtx`. Use `DraggablePanel`, `Section` for particle modules (emitter, color over lifetime, size over lifetime), `LabeledSlider`/`Vec3Slider` per module. Remove `set_particle_inspector_ctx`/`take_particle_inspector_ctx`.

#### Phase 3: Migrate complex panels

- [x] Migrate Hierarchy panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `HierarchyDrawCtx`. Use `TreeView` descriptor with `TreeItem` data from `Environment`, `ContextMenu` for right-click actions, `on_select` callback. Remove `set_hierarchy_ctx`/`take_hierarchy_ctx`.
- [x] Migrate Inspector panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `InspectorDrawCtx`. Use `DraggablePanel`, `Section` per component with `delete_button`, `LabeledSlider`/`Vec3Slider`/`Toggle`/`ColorPicker` per field, `Modal` for Add Component picker. This is the hardest migration. Remove `set_inspector_ctx`/`take_inspector_ctx`.
- [x] Migrate Console panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `ConsoleDrawCtx`. Use `DraggablePanel`, `ScrollView` with `Text` rows (colored by log level), `TextField` for command input with `on_submit`. Remove `set_console_ctx`/`take_console_ctx`.
- [x] Migrate Asset Browser panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `AssetBrowserDrawCtx`. Use `Grid` or custom `Selectable` grid for thumbnails, `ContextMenu` for right-click, `TextField` for search, `Modal` for rename/delete confirmations. Remove `set_asset_browser_ctx`/`take_asset_browser_ctx`.

#### Cleanup: remove legacy code

- [x] Remove all thread-local `RefCell<Option<DrawCtx>>` bridges — Confirmed none remain; all panels use Environment injection.
- [x] Remove `ViewDescriptor::Custom` escape hatch — removed entirely since all panels migrated to declarative trees. Also removed `CustomDrawFn` type alias, `scratch_data`/`set_scratch`/`get_scratch` infrastructure from `UiContext`, and `test_diff_same_custom_is_update` test.
- [x] **Remove immediate-mode builder widgets that have declarative equivalents** — Remove from `widgets/mod.rs` public API and update all callers. Keep only widgets with no declarative counterpart (e.g. `DockArea`). Remove one at a time:
  - [x] Remove `Button` — callers use `button_with_colors()` directly
  - [x] Remove `Slider` — callers use `ui.slider()` directly
  - [x] Remove `LabeledSlider` — callers use inline `draw_labeled_slider()` helper
  - [x] Remove `Vec3Slider` — callers use `vec3_slider()`
  - [x] Remove `ToggleButton` — callers use `toggle()`
  - [x] Remove `TextInput` — callers use `textfield()`
  - [x] Remove `RadioButton` — callers use `radio()`
  - [x] Remove `ImageButton` — callers use `ui.image_button()` directly
  - [x] Remove `Panel` — callers use `panel()`
- [x] Add `ViewDescriptor` construction tests — Added 19 tests for constructors and modifiers.
- [x] **Add declarative integration tests** — frame-level tests that build a descriptor tree, run `ViewTree::frame()`, assert bounds, actions, and state mutations:
  - [x] Add tests for `diff_descriptor` — 8 integration tests for keyed/unkeyed insert/remove/reorder.
  - [x] Add tests for `ViewTree::sync_tree` — verify tree sync preserves state across descriptor changes, handles mount/unmount
  - [x] Add tests for `TransitionContainer` — verify enter/exit transitions fire correctly, animation state management
  - [x] Add tests for `DockArea` — verify tab/panel docking, splitting, and layout computation
  - [x] Add tests for `ColorPicker` — verify color selection, HSV state, and callback invocation
  - [x] Add tests for `BindingResolver` — verify state binding resolution, nested bindings, and error handling for missing keys
- [x] Add integration tests for new widget descriptors — add tests for `Section`, `TabBar`, `Grid`, `Separator`, `Icon`, `Selectable` descriptors to ensure they build, diff, layout, and render correctly.

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
- [ ] Add system timing — measure ECS system execution time, display in debug overlay

### Testing
- [ ] **Explore integration test framework design** — Research headless app init patterns (mock renderer, software rasterizer, GPU-less CI), entity spawning test helpers, frame execution harness, and state assertion utilities. Produce concrete implementation TODO items.
- [ ] Add ECS round-trip tests — spawn entity, add components, serialize, deserialize, verify equivalence

## Production Readiness

### katla_app - Critical Issues (Block Production)

- [x] **Remove all `#[allow(dead_code)]` violations** — Project rule: never suppress dead code warnings. Remove unused code instead. Locations: `ui/editor_ui/types.rs:316,516`, `ui/particle_inspector.rs:46`, `ui/renderer.rs:1`, `util/background_loader.rs:1,39,61,118`
- [x] **Eliminate `unwrap()` in `rapier_physics_system.rs`** — 2 production unwraps removed (1 in prev batch, 1 in this batch); remaining ~33 are test code
- ~~**Eliminate `unwrap()` in `editor/mod.rs`**~~ — False positive. All 12 unwraps are in test code.
- [x] **Eliminate `unwrap()` in `stl_parser.rs`** — 9 unwraps. Parsing code should return `Result` instead of unwrapping on malformed input
- ~~**Eliminate `unwrap()` in `spawner.rs`**~~ — False positive. All unwraps are in test code.
- [x] **Eliminate `unwrap()` in `editor/agent.rs`** — 5 production unwraps replaced with if-let/unwrap_or_else
- [x] **Eliminate `unwrap()` in `builder.rs`** — 6 unwraps. Application builder should propagate initialization failures
- [x] **Eliminate `unwrap()` in remaining katla_app files** — 6 production unwraps removed across mcp, layout, viewport_grid, mod, physics_system; rest were test code
- [x] **Eliminate `expect()` in katla_app production code** — 19 expects replaced with error propagation in init.rs (15), gizmo.rs (2), billboard_icons.rs (1), renderer.rs (1); kept 6 genuine invariants in game_state.rs (2) and background_loader.rs (4)
- [x] **Eliminate 10 `panic!()` calls** — Uncontrolled crashes. Replace with proper error handling and propagation
- [x] **Fix clippy warnings blocking `-D warnings` builds** — Run `cargo clippy -p katla_app -- -D warnings` to identify and fix all warnings
- [x] **Fix `ViewportManager` cross-crate doc link** — `resources/viewport_state.rs:86` references `ViewportManager` (in katla_gfx, not katla_app). The other doc links in `resource_loading.rs` have been fixed.

### katla_app - Major Issues (Should Fix Before Production)

- [x] **Improve error handling in Preferences** — `preferences.rs:load()` silently returns defaults on error. Should propagate errors or at minimum log with `error!` level
- [x] **Complete audio spatial positioning** — Implemented distance-based attenuation and stereo panning for PlaySoundAt voices.
- [x] **Make resource path discovery more robust** — `resources/mod.rs` uses multiple fallback paths that depend on runtime context. Consider using `CARGO_MANIFEST_DIR` as primary with explicit override via environment variable
- [x] **Add retry logic for background loading** — `util/background_loader.rs` has no retry mechanism for transient failures (network timeouts, disk I/O errors)
- [ ] **Add GPU resource health checks** — No validation that GPU resources (textures, buffers, pipelines) are in good state after initialization or during runtime

### katla_app - Documentation

- [ ] **Add production deployment guide** — Document requirements, configuration, and steps for deploying katla_app-based applications to end users
- [ ] **Add error handling best practices guide** — Document patterns for proper error propagation in katla_app, especially for plugin/script authors

### katla_audio - Critical Issues (Block Production)

- [x] **Remove unused `is_looping()` methods** — `voice.rs:221` and `streaming_voice.rs:119` have `pub fn is_looping()` that are never used. Violates project rule against `#[allow(dead_code)]` — either use them or remove them
- [x] **Eliminate 34 `unwrap()`/`expect()` calls** — Replaced all 26 `Mutex::lock().unwrap()` in mixer.rs with `.expect()`. No other production unwraps remain (referenced file:lines were stale).
- [x] **Fix clippy warnings blocking `-D warnings` builds** — 2 warnings from unused `is_looping` methods prevent clean builds with `-D warnings`

### katla_audio - Major Issues (Should Fix Before Production)

- [x] **Improve device error handling** — `AudioEngine::new()` returns `Result` but callers may not handle all error cases properly. Add better recovery/error messages for common failures (no device, permissions, etc.)
- [x] **Add stream error recovery** — Audio stream errors (device disconnection, underrun) should attempt recovery rather than failing permanently
- [x] **Document thread safety guarantees** — Real-time audio thread constraints should be documented for API users to avoid deadlocks
- [x] **Add audio device hot-swap** — Implemented poll_device_change() in AudioEngine, called each frame from AudioSystem.

### katla_audio - Documentation

- [ ] **Add API usage examples** — Document common patterns: playing sounds, streaming music, applying effects, spatial audio
- [ ] **Add performance considerations guide** — Document voice limits, mixing overhead, CPU usage per effect type

### katla_script - Critical Issues (Block Production)

- ~~**Eliminate `unwrap()`/`expect()` in `sandbox.rs`**~~ — False positive. Production code already returns Result<_, ScriptError>; all unwraps are in test code.
- [x] **Eliminate `expect()` in `system.rs`** — Removed unused Default impl containing the expect; callers use ScriptSystem::new() which returns Result
- ~~**Eliminate 18 `panic!()` calls**~~ — False positive. All 18 are in test assertion code (`tests.rs`), which is standard Rust test practice.
- [x] **Remove unused mutable variable** — Clippy warning: "variable does not need to be mutable" blocks `-D warnings` builds
- [x] **Fix clippy warnings blocking `-D warnings` builds** — 5 warnings prevent clean builds. Run `cargo clippy -p katla_script -- -D warnings`

### katla_script - Major Issues (Should Fix Before Production)

- [x] **ScriptSystem initialization should not panic** — `ScriptSystem::new()` now returns `Result<Self, ScriptError>` instead of panicking.
- [x] **Improve error messages from Lua** — Script errors should provide more context (which script, which function, stack traces where available)
- [x] **Add script timeout protection** — Long-running scripts can block the main thread. Add execution time limits or yield points
- [x] **Sandbox script capabilities** — Scripts currently have full access. Need safe subset of Lua APIs for production (restrict file I/O, network, etc.)
- [x] **Add script state serialization** — Already implemented. ScriptDescriptor in EntityDescriptor with save/load paths.

### katla_script - Documentation

- [ ] **Add Lua API reference** — Comprehensive documentation of all bindings available to scripts
- [ ] **Add script best practices guide** — Patterns for performance, error handling, event subscription

### katla_ecs - Production Readiness Roadmap

**Current release decision:** No-go for production. Keep the runtime on the sequential ECS path until every P0 exit criterion below is complete and validated. `update_parallel()` must be treated as experimental/unsafe until the parallel architecture is replaced.

**Agent execution rules:**

- Complete one task ID per PR unless a task explicitly lists inseparable substeps.
- Add the regression test before or in the same commit as the fix.
- Do not re-enable parallel runtime execution as part of an intermediate refactor.
- Preserve public behavior unless the task explicitly changes the contract.
- Update `katla_ecs/AGENTS.md` and `memory-bank/systemPatterns.md` when an architectural contract changes.
- Run the task's listed checks plus `cargo fmt --all -- --check` before marking it complete.

#### P0 - Immediate containment (block all production use)

- [ ] **ECS-P0-001: Disable parallel ECS execution in runtime loops** — Replace calls to `world.update_parallel(dt)` in windowed and headless application loops with `world.update(dt)`.
  - **Done when:** normal and headless runtime paths are sequential; no application startup path invokes `update_parallel()`; existing frame-loop tests pass.
- [ ] **ECS-P0-002: Gate the unsafe parallel API** — Make `World::update_parallel()` internal or place it behind an opt-in `experimental_parallel_ecs` feature that is disabled by default.
  - **Depends on:** ECS-P0-001.
  - **Done when:** downstream crates cannot accidentally select the parallel path in a default build; API docs clearly state that the feature is experimental and not production-safe.
- [ ] **ECS-P0-003: Add a release guard against accidental parallel activation** — Add a compile-time or startup assertion/test that production/default builds do not enable `experimental_parallel_ecs`.
  - **Depends on:** ECS-P0-002.
  - **Done when:** CI fails if the standard game binary starts using the experimental path again.

#### P0 - Entity identity and sparse-set correctness

- [ ] **ECS-P0-010: Reproduce stale-ID component removal** — Add a regression test that destroys an entity, reuses the slot, then calls `remove_component` with the stale ID and verifies the new entity is unchanged.
  - **Done when:** the test fails on the current implementation and documents the expected generation behavior.
- [ ] **ECS-P0-011: Validate entities in `World::remove_component`** — Return `false` without touching storage when the ID is not live or its generation is stale.
  - **Depends on:** ECS-P0-010.
  - **Done when:** stale, never-created, and already-destroyed IDs cannot remove components or emit component events.
- [ ] **ECS-P0-012: Make `SparseSet<EntityId, T>` compare the complete key** — Verify the stored dense key equals the requested `EntityId`, including generation, before update, lookup, mutable lookup, contains, or removal succeeds.
  - **Done when:** all sparse-set operations reject an index collision with a different generation; swap-remove bookkeeping remains correct.
- [ ] **ECS-P0-013: Add generational sparse-set property tests** — Generate repeated allocate/add/remove/destroy/reuse sequences and compare sparse-set results with a simple reference model.
  - **Depends on:** ECS-P0-012.
  - **Done when:** randomized tests cover index reuse, replacement, swap removal, missing pages, and multiple generations.
- [ ] **ECS-P0-014: Preserve stale-ID invalidation across `clear_entities()`** — Redesign allocator clearing so old IDs cannot become valid after the world is cleared and index 0/generation 0 is allocated again.
  - **Done when:** an ID captured before `clear_entities()` remains invalid after arbitrary subsequent spawns; allocator count and free-list invariants remain valid.
- [ ] **ECS-P0-015: Define generation-wrap behavior** — Choose and document a policy for `u32` generation overflow: retire the slot, widen/epoch the identifier, or explicitly accept wrap after a proven bound.
  - **Done when:** the allocator has a testable policy and no comment claims wrapping is automatically safe.
- [ ] **ECS-P0-016: Guard entity-index exhaustion** — Replace `self.slots.len() as u32` truncation with checked conversion and a clear allocation error/panic policy before the index exceeds `u32::MAX`.
  - **Done when:** allocation cannot silently alias an existing index after integer truncation.
- [ ] **ECS-P0-017: Audit `EntityId::from_raw` boundaries** — Enumerate every deserialization, scripting, physics, and editor call site; validate IDs against the target `World` before component/resource operations.
  - **Done when:** untrusted or persisted raw IDs cannot mutate a newly reused entity by index alone.

#### P0 - Despawn, component removal, and event lifetime

- [ ] **ECS-P0-020: Specify the lifecycle contract before refactoring** — Document exact ordering for spawn, add, replace, remove, despawn, callbacks, storage deletion, and event visibility.
  - **Done when:** the contract answers whether removed component data is available to cleanup hooks, which frame readers observe events, and whether replacement emits `Added`, `Changed`, or a dedicated event.
- [ ] **ECS-P0-021: Add failing end-to-end despawn tests** — Cover `World::destroy_entity` through ScriptSystem `on_destroy`, GPU resource tracking cleanup, physics cleanup, and event readers.
  - **Done when:** tests demonstrate the current missing-data/event-flush failures without relying on direct engine method calls.
- [ ] **ECS-P0-022: Introduce deferred structural commands** — Add a `Commands` queue for spawn, despawn, add, replace, and remove operations requested during system execution.
  - **Depends on:** ECS-P0-020.
  - **Done when:** systems can queue structural changes without mutating allocator/storage maps during iteration; commands apply at a deterministic stage boundary.
- [ ] **ECS-P0-023: Add pre-remove/pre-despawn cleanup data** — Invoke lifecycle hooks or capture removal payloads while component values are still accessible, before storage deletion and generation invalidation.
  - **Depends on:** ECS-P0-020, ECS-P0-022.
  - **Done when:** cleanup consumers never need to query a component after it has been removed.
- [ ] **ECS-P0-024: Double-buffer or cursor-track ECS events** — Replace unconditional end-of-update clearing with a current/previous buffer or reader cursors so all intended consumers can observe each event exactly once.
  - **Done when:** systems and post-update application consumers have explicitly defined visibility; events survive long enough for GPU cleanup; old events do not repeat indefinitely.
- [ ] **ECS-P0-025: Fix GPU cleanup ownership** — Track the handles needed for cleanup independently of querying a destroyed `DrawableComponent`, or include the handles in the removal payload.
  - **Depends on:** ECS-P0-023, ECS-P0-024.
  - **Done when:** destroying an entity decrements mesh/material/skeleton reference counts and destroys zero-reference GPU resources in an integration test.
- [ ] **ECS-P0-026: Fix ScriptSystem destruction lifecycle** — Maintain a reliable entity-to-script-instance mapping and call `on_destroy` before component removal; remove the instance even when the hook fails.
  - **Depends on:** ECS-P0-023, ECS-P0-024.
  - **Done when:** normal World destruction triggers exactly one `on_destroy`, removes the instance, and does not require `get_component` on a dead entity.
- [ ] **ECS-P0-027: Define component replacement events** — Make repeated `add_component<T>` distinguish first insertion from replacement and emit the documented event sequence.
  - **Depends on:** ECS-P0-020.
  - **Done when:** duplicate insertion tests no longer report two misleading `Added` events unless that is the explicitly chosen contract.
- [ ] **ECS-P0-028: Make `clear_entities()` lifecycle-safe** — Decide whether clearing emits per-entity events or a bulk-clear event and ensure script, physics, audio, and GPU owners release external resources.
  - **Depends on:** ECS-P0-020, ECS-P0-023.
  - **Done when:** scene reload/clear cannot bypass cleanup hooks or leak external resources.

#### P0 - Transform hierarchy robustness

- [ ] **ECS-P0-030: Reject self-parenting and parent cycles at mutation time** — Centralize Parent updates behind a validated API that detects cycles before committing the relationship.
  - **Done when:** `A -> A` and `A -> B -> A` are rejected with a recoverable error and the prior hierarchy remains intact.
- [ ] **ECS-P0-031: Make topological sorting return an error** — Return cycle details instead of logging and continuing with a partially invalid ordering.
  - **Depends on:** ECS-P0-030.
  - **Done when:** cyclic nodes are never included in the update list as if they were valid.
- [ ] **ECS-P0-032: Add visited/depth guards to ancestry traversal** — Make `calculate_world_transform` terminate on malformed legacy or deserialized data even if validation was bypassed.
  - **Done when:** no hierarchy input can produce an infinite loop; errors identify the involved entities.
- [ ] **ECS-P0-033: Replace recursive descendant traversal** — Use an iterative stack/queue with a visited set to avoid stack overflow and repeated visits.
  - **Done when:** a very deep hierarchy and a cyclic corrupted hierarchy both terminate deterministically.
- [ ] **ECS-P0-034: Add hierarchy adversarial tests** — Cover self-cycle, two-node cycle, long cycle, missing parent, parent without transform, and a chain deep enough to overflow the previous recursive implementation.
  - **Depends on:** ECS-P0-030 through ECS-P0-033.

#### P0 - Parallel ECS architecture replacement

- [ ] **ECS-P0-040: Write an ADR for safe system execution** — Compare typed system parameters plus exclusive systems against alternative designs; explicitly reject creating simultaneous `&mut World` references.
  - **Done when:** the ADR defines ownership, system registration, resources, structural commands, events, stages, and migration strategy.
- [ ] **ECS-P0-041: Split parallel and exclusive system capabilities** — Parallel systems must be `Send` and receive only scoped typed access; exclusive/main-thread systems may receive `&mut World` and never enter Rayon.
  - **Depends on:** ECS-P0-040.
  - **Done when:** a `!Send` system cannot be scheduled on a worker at compile time; ScriptSystem is registered in the exclusive lane.
- [ ] **ECS-P0-042: Separate sendable and non-send resources** — Require `Send + Sync` for worker-accessible resources and provide an explicit main-thread-only resource store for `Rc`, `RefCell`, window, script VM, and thread-affine objects.
  - **Depends on:** ECS-P0-040.
  - **Done when:** Rayon tasks cannot obtain references to non-send resources through safe APIs.
- [ ] **ECS-P0-043: Replace `System::update(&mut World)` on the parallel path** — Introduce typed parameters such as `Query`, `Res`, `ResMut`, event readers/writers, local state, and `Commands`.
  - **Depends on:** ECS-P0-022, ECS-P0-041, ECS-P0-042.
  - **Done when:** no parallel worker constructs or receives `&mut World`; the old trait remains only for exclusive systems or is removed.
- [ ] **ECS-P0-044: Derive access metadata from system parameters** — Generate component/resource read-write sets automatically instead of requiring duplicate `component_access` and `component_access_dyn` methods.
  - **Depends on:** ECS-P0-043.
  - **Done when:** forgetting an access declaration is impossible for safe registered systems; static and dynamic metadata cannot diverge.
- [ ] **ECS-P0-045: Prevent concurrent mutation of the storage registry** — Pre-register/freeze component storage entries for a schedule or move interior mutability to independently borrowed per-type cells with a documented protocol.
  - **Depends on:** ECS-P0-043.
  - **Done when:** two systems accessing different component types never concurrently mutate/re-hash the same `HashMap` container.
- [ ] **ECS-P0-046: Add debug borrow validation** — Track active readers/writers per component and resource type and fail immediately on an invalid schedule or unsafe internal borrow.
  - **Depends on:** ECS-P0-044, ECS-P0-045.
  - **Done when:** tests intentionally requesting write/write and read/write overlap receive deterministic diagnostics.
- [ ] **ECS-P0-047: Apply structural commands only at barriers** — Spawn/despawn/add/remove must execute after all systems in the stage release their borrows.
  - **Depends on:** ECS-P0-022, ECS-P0-043.
  - **Done when:** structural changes cannot race queries or invalidate dense vectors during worker execution.
- [ ] **ECS-P0-048: Make execution order a real scheduling contract** — Implement explicit stages/barriers and/or `before`/`after` dependencies; do not allow `EARLY`, `NORMAL`, and `LATE` systems into the same group merely because access metadata is disjoint.
  - **Depends on:** ECS-P0-040, ECS-P0-044.
  - **Done when:** order-only tests prove `EARLY < NORMAL < LATE` even for systems with disjoint accesses; explicit dependency cycles return actionable errors.
- [ ] **ECS-P0-049: Remove raw `SendPtr` and full-World `UnsafeWorldCell` scheduling** — Delete the code path that marks raw pointers as `Send`/`Sync` and recreates `&mut World` in Rayon tasks.
  - **Depends on:** ECS-P0-043 through ECS-P0-048.
  - **Done when:** repository search finds no scheduler conversion from `*mut World` to multiple mutable references.
- [ ] **ECS-P0-050: Add parallel migration tests for every built-in system** — Verify component/resource parameters, exclusive classification, stage ordering, enabled/disabled behavior, events, and panic recovery.
  - **Depends on:** ECS-P0-041 through ECS-P0-049.
- [ ] **ECS-P0-051: Re-enable parallel runtime only after exit gates pass** — Switch application loops back only after Miri, sanitizer, scheduler, lifecycle, and workload benchmarks pass.
  - **Depends on:** all ECS-P0 tasks.
  - **Done when:** the PR includes measured evidence, all release gates below pass, and the experimental feature is no longer needed.

#### P1 - Query soundness and change detection

- [ ] **ECS-P1-001: Add Miri tests for tuple query aliasing** — Exercise all mutable query forms, missing storages, same-type rejection, swap removal, and iterator lifetime boundaries.
  - **Done when:** Miri passes without stacked-borrows/tree-borrows violations.
- [ ] **ECS-P1-002: Remove raw-pointer borrowing through the storage `HashMap`** — Replace repeated `get_storage`/`get_storage_mut` calls through one raw manager pointer with a design that proves entries are disjoint.
  - **Depends on:** ECS-P1-001.
  - **Done when:** mutable tuple queries no longer rely on caller-guaranteed aliasing across a shared map container.
- [ ] **ECS-P1-003: Prove filtered mutable-query soundness** — Remove or justify the simultaneous mutable query borrow plus raw shared manager pointer used by filters.
  - **Depends on:** ECS-P1-002.
  - **Done when:** filter checks use a safe disjoint view or a narrowly documented unsafe primitive covered by Miri.
- [ ] **ECS-P1-004: Remove fictitious `'static` references from `par_query`** — Build Rayon iterators directly from correctly borrowed slices/storages where possible.
  - **Done when:** `UnsafeStorageCell` lifetime erasure is removed or reduced to a formally documented minimal primitive with Miri coverage.
- [ ] **ECS-P1-005: Introduce `Mut<T>` change-tracking guards** — Mark a component dirty on `DerefMut` or explicit `set_changed`, not merely when a mutable handle is requested.
  - **Done when:** immutable use of a mutable handle does not report a change; actual writes through direct get and queries do.
- [ ] **ECS-P1-006: Make mutable queries participate in change detection** — Return `Mut<T>` or equivalent from every mutable query permutation, including filtered and multi-mutable queries.
  - **Depends on:** ECS-P1-005.
  - **Done when:** mutating through `query::<&mut T>()` is visible to a later changed query in the documented frame window.
- [ ] **ECS-P1-007: Drive changed queries from dirty entities** — Iterate dirty IDs and fetch required components instead of scanning the complete normal query and HashSet-filtering every row.
  - **Depends on:** ECS-P1-005, ECS-P1-006.
  - **Done when:** cost scales with dirty entities plus result validation rather than all entities in the driver storage.
- [ ] **ECS-P1-008: Reuse changed-query buffers correctly** — Remove the `mem::take` capacity loss or introduce a pool/owned scratch object returned on iterator drop.
  - **Done when:** repeated changed queries reach steady state without allocating a new large HashSet each call.
- [ ] **ECS-P1-009: Document and test union semantics** — Multi-component changed queries currently mean “any queried component changed”; codify this and add unchanged/read-only/mixed-removal cases.
  - **Done when:** public docs and tests agree on frame boundaries and union behavior.

#### P1 - Event, panic, and deterministic behavior

- [ ] **ECS-P1-020: Define panic behavior for system execution** — Decide whether a panic aborts the tick, isolates one system, or disables it; ensure systems, events, and command buffers remain internally consistent.
  - **Done when:** sequential and future parallel paths share a tested recovery contract.
- [ ] **ECS-P1-021: Make scheduler build failure observable** — Return a typed error from update/schedule construction instead of printing to stderr and silently skipping the frame.
  - **Done when:** callers can log, fail startup, or fall back to sequential execution deliberately.
- [ ] **ECS-P1-022: Stabilize same-order execution semantics** — Define whether equal-order systems preserve registration order and add deterministic tests.
  - **Done when:** repeated runs produce the same observable ordering or the API explicitly declares ordering unspecified where safe.

#### P2 - Query and storage performance

- [ ] **ECS-P2-001: Choose the smallest immutable query driver** — Select the smallest component storage at query construction rather than always iterating the first tuple component.
  - **Done when:** tuple order no longer causes large performance differences for read-only queries; result tuple order remains unchanged.
- [ ] **ECS-P2-002: Design driver selection for mutable queries** — Determine when a mutable storage must drive iteration and how to select among multiple mutable storages without violating alias rules.
  - **Depends on:** ECS-P1-002.
  - **Done when:** the decision is documented and benchmarked before implementation.
- [ ] **ECS-P2-003: Add sparse-join benchmarks** — Benchmark 1%, 10%, 50%, and 100% overlap at 1K/10K/100K entities with rare/common tuple order reversed.
  - **Done when:** Criterion reports make driver-selection regressions visible.
- [ ] **ECS-P2-004: Benchmark changed queries with sparse dirtiness** — Measure 0, 1, 10, 1%, and 100% dirty entities across one- and multi-component queries.
  - **Depends on:** ECS-P1-007.
- [ ] **ECS-P2-005: Benchmark destruction across component-type counts** — Measure entity destruction with 1, 8, 32, and 128 registered component storages; assess replacing the all-storage scan with per-entity membership metadata.
  - **Done when:** the chosen design has evidence for expected project scale.
- [ ] **ECS-P2-006: Profile sparse-page memory behavior** — Measure page overhead and fragmentation for dense IDs, widely separated IDs, churn, and many low-cardinality component types; evaluate `PAGE_SIZE = 1024`.
  - **Done when:** page size and representation are justified by data or adjusted.
- [ ] **ECS-P2-007: Add capacity reservation APIs** — Allow entity, component-storage, event, and command buffers to reserve based on scene/import size.
  - **Done when:** bulk scene loading avoids predictable reallocations without exposing unsafe internals.

#### P2 - Scheduler and system performance

- [ ] **ECS-P2-020: Benchmark scheduler overhead** — Measure sequential versus parallel ticks for 0-32 systems, tiny/medium/large workloads, conflicts, disabled systems, and different thread counts.
  - **Depends on:** safe parallel architecture completion.
- [ ] **ECS-P2-021: Replace whole-group barriers with a ready queue if justified** — Start a system when its own dependencies complete instead of waiting for every system in the previous group.
  - **Depends on:** ECS-P2-020.
  - **Done when:** implementation is adopted only if representative benchmarks show a material gain without harming determinism or debuggability.
- [ ] **ECS-P2-022: Avoid per-frame enabled-system allocation** — Reuse scheduler scratch storage or represent enabled systems without allocating a new `Vec` for every parallel group.
  - **Depends on:** safe scheduler redesign.
- [ ] **ECS-P2-023: Add ECS system timing instrumentation** — Record per-system queue delay and execution time for the profiler overlay with negligible disabled cost.
  - **Done when:** real Katla workloads can identify systems that do not benefit from parallel scheduling.

#### P2 - Transform hierarchy performance

- [ ] **ECS-P2-030: Cache hierarchy topology** — Maintain parent-to-children adjacency and topological order incrementally instead of rebuilding HashMaps/HashSets every frame.
  - **Depends on:** ECS-P0-030.
  - **Done when:** unchanged hierarchy frames do not rebuild topology.
- [ ] **ECS-P2-031: Use a dirty-root queue** — Track changed local transforms/parent links directly and propagate only affected subtrees.
  - **Depends on:** ECS-P1-005, ECS-P2-030.
  - **Done when:** a static scene with zero dirty transforms performs O(1) or near-O(1) hierarchy work, excluding instrumentation.
- [ ] **ECS-P2-032: Add hierarchy workload benchmarks** — Cover flat scenes, deep chains, wide trees, 0/1/10/30/100% dirty, and parent changes.
  - **Depends on:** ECS-P2-030, ECS-P2-031.

#### P2 - Benchmark quality

- [ ] **ECS-P2-040: Remove wall-clock thresholds from unit tests** — Move machine-dependent “100K operations under N ms” assertions to Criterion or a dedicated performance job.
  - **Done when:** unit tests verify correctness only and do not fail on slower CI hardware.
- [ ] **ECS-P2-041: Store benchmark baselines** — Add documented representative hardware/configuration and retain Criterion summaries as CI artifacts.
  - **Done when:** performance changes can be compared rather than judged from one-off numbers.
- [ ] **ECS-P2-042: Define regression budgets** — Establish thresholds only after stable baselines exist for spawn, query, sparse join, change detection, destruction, and system ticks.
  - **Depends on:** ECS-P2-003 through ECS-P2-041.

#### P1 - CI and soundness gates

- [ ] **ECS-P1-030: Add an ECS-focused GitHub Actions workflow** — Run formatting, clippy, tests, doctests, and feature combinations for `katla_ecs` independently of GPU availability.
  - **Done when:** every PR receives an ECS status check on supported Linux, macOS, and Windows runners.
- [ ] **ECS-P1-031: Pin the Rust toolchain and MSRV policy** — Add `rust-toolchain.toml` and package/workspace `rust-version`; document edition/toolchain expectations.
  - **Done when:** local and CI builds use the same supported compiler family.
- [ ] **ECS-P1-032: Enforce workspace quality commands** — CI must run:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace --all-features`
  - `cargo test --workspace --doc`
- [ ] **ECS-P1-033: Add Miri CI for `katla_ecs`** — Run focused query, sparse-set, allocator, and lifecycle tests under Miri on a pinned nightly.
  - **Depends on:** ECS-P1-001 through ECS-P1-004.
- [ ] **ECS-P1-034: Add sanitizer coverage for parallel execution** — Run ThreadSanitizer where supported and Address/UndefinedBehavior sanitizers for stress binaries/tests.
  - **Depends on:** safe parallel architecture being runnable.
- [ ] **ECS-P1-035: Add state-machine/property tests** — Compare random World operation sequences against a reference model and call `World::validate()` after every step.
  - **Done when:** seeds are reproducible and failures minimize to a readable operation trace.
- [ ] **ECS-P1-036: Add fuzz targets** — Fuzz raw ID decoding, allocator reuse, sparse-set operation sequences, hierarchy edits, and deferred command application.
  - **Done when:** targets run locally and in a scheduled CI budget with a retained corpus.
- [ ] **ECS-P1-037: Add dependency and license checks** — Run vulnerability/advisory and license-policy checks for the workspace; explicitly track known future-incompatible dependencies.
  - **Done when:** accepted exceptions are documented with owner and review date.

#### P1 - Production exit criteria

- [ ] **ECS-P1-050: Complete the sequential ECS release gate** — Entity generation, clear, lifecycle cleanup, events, transform cycles, Miri query tests, and ECS CI must all pass while runtime remains sequential.
  - **Depends on:** ECS-P0-010 through ECS-P0-034 and ECS-P1-001 through ECS-P1-037.
- [ ] **ECS-P1-051: Complete the parallel ECS release gate** — No raw full-World aliasing, all worker data is `Send`/`Sync`, non-send systems are exclusive, order is enforced, deferred commands are deterministic, Miri/sanitizers pass, and benchmarks show a benefit.
  - **Depends on:** ECS-P0-040 through ECS-P0-051 and ECS-P2-020 through ECS-P2-023.
- [ ] **ECS-P1-052: Run an extended headless soak test** — Execute representative scenes for a large frame count with repeated spawn/despawn, scene reload, scripts, physics, and resource cleanup; fail on validation errors or leaked tracked resources.
  - **Depends on:** ECS-P1-050; repeat after ECS-P1-051.

#### Long-term architecture evaluation

- [ ] **ECS-LT-001: Explore sparse-set to archetype migration** — Research archetype storage, archetype graphs, add/remove costs, iteration gains, serialization impact, and migration strategies. Do not implement until current sparse-set correctness and lifecycle work is stable.
  - **Done when:** the output is an ADR and a sequence of independently shippable tasks with representative benchmark evidence.
- [ ] **ECS-LT-002: Explore generic component serialization registry** — Design type-erased registration of serialize/deserialize/clone/inspect operations and define ID/version migration behavior.
  - **Done when:** scene serialization no longer depends on hand-written component lists and stale raw entity references are remapped safely.

#### Documentation

- [ ] **ECS-DOC-001: Rewrite the ECS architecture overview** — Document actual sparse-set storage, generational identity, typed queries, resources, structural commands, stages, and safe parallel boundaries; do not refer to a removed archetype implementation as current behavior.
- [ ] **ECS-DOC-002: Write the system authoring guide** — Show parallel typed parameters, exclusive systems, non-send resources, ordering, commands, events, and common unsafe mistakes.
- [ ] **ECS-DOC-003: Document entity and component lifecycle** — Include precise callback/event ordering and cleanup ownership for scripts, physics, audio, renderer, and editor.
- [ ] **ECS-DOC-004: Document unsafe invariants** — List every remaining unsafe module/function, its proof obligations, and the Miri/sanitizer tests that protect it.
- [ ] **ECS-DOC-005: Document performance characteristics** — Explain query-driver selection, sparse joins, change detection, structural mutation costs, page memory, and when parallelism is expected to win.
- [ ] **ECS-DOC-006: Add migration guidance** — Explain API changes from the current `System::update(&mut World)` model and provide before/after examples for built-in and game systems.

#### Completed groundwork retained for context

- [x] Add `Send + Sync` bounds to `Component`.
- [x] Add `ResourceAccess` types and scheduler conflict tests.
- [x] Add `World::validate()` and `validate_entities()`.
- [x] Remove the unused archetype module.
- [x] Concentrate `par_query` lifetime erasure in `UnsafeStorageCell` pending its removal under ECS-P1-004.

### katla_agent - Critical Issues (Block Production)

- [x] **Eliminate 24 `unwrap()`/`expect()` calls** — Replaced the single production `Runtime::new().unwrap()` in mcp.rs with proper error handling. All other unwraps are in test code.
- ~~**Eliminate 7 `panic!()` calls**~~ — False positive. All 7 are in test assertion code (`*_test.rs`), which is standard Rust test practice.
- [x] **Fix test compilation errors** — Tests fail to compile due to feature-gated items (`llm-assistant` feature) not being available in test context. Use `#[cfg(feature = "llm-assistant")]` on tests or provide mock implementations
- ~~**Implement `std::error::Error` for `LlmError`**~~ — Already implemented at `llm/mod.rs:132`.

### katla_agent - Major Issues (Should Fix Before Production)

- [x] **Add LLM feature availability checks** — Already properly feature-gated with #[cfg(feature = "llm-assistant")] and #[cfg(feature = "mcp-server")]
- [x] **Add co-creator tool error handling** — Tools in `co_creator/` module should return `Result` instead of using `unwrap()`/`expect()`
- [x] **Improve configuration validation** — `config.rs` should validate configuration on load and provide clear error messages
- [x] **Add rate limiting for LLM calls** — Prevent rapid successive LLM calls from overwhelming API rate limits or causing excessive costs

### katla_agent - Documentation

- [ ] **Add agent architecture overview** — Document how co-creator, LLM integration, and tool execution work together
- [ ] **Add tool development guide** — How to create new tools for the co-creator system
- [ ] **Document LLM configuration** — Configuration options, API keys, rate limits, model selection

## Rendering - katla_gfx

### P0 - Critical Issues (Block Production)
- [x] **Remove all `#[allow(dead_code)]` violations** — Project rule: never suppress dead code warnings. Removed from: `pipeline.rs:32` (`CompareOp` enum), `render_graph/compiler.rs:38` (`PassDagNode` struct), `lib.rs:175` (backend/pipeline modules), `vulkan/material/builder.rs:144` (`with_push_constant_range` method), `shadow/cascade.rs:137` (`cascades()` method). Also removed dead code: `with_push_constant_range` method (never used) and unused re-exports in `backend/mod.rs`.
- [x] **Add `Drop` impl for `VulkanRenderer`** — Currently requires manual `destroy()` call. Missing `Drop` can cause resource leaks if user forgets to call it
- [x] **Fix Metal backend parity for `update_texture()`** — Default impl is no-op, Vulkan implements it, Metal inherits no-op. Either implement for Metal or remove default impl

### P1 - Backend Parity (Must Fix)
- [ ] **Explore frame lifecycle unification across backends** — Vulkan uses frame graph via `render()`, Metal uses hardcoded `render_frame()`. Research how to route Metal through `FrameGraph<B>::execute()`, identify which passes need `RenderGraphBackend` dispatch implementations on Metal. Produce concrete implementation TODO items.
- [ ] **Implement `recompile_materials_for_shader()` for Metal** — Currently no-op (inherited default). Metal backend needs real implementation
- [ ] **Implement `init_animation_pipeline()` for Metal** — Currently no-op (inherited default). Metal backend needs real implementation
- [ ] **Remove `render_frame()` from Metal backend** — Replace with frame graph execution through `render()` method once parity is achieved. **Depends on:** section E completion + frame lifecycle unification exploration.
- [ ] **Remove Metal-specific methods from `AnyRenderer`** — `queue_metal_picking_readback()`, `check_metal_picking_readback()`, `has_pending_metal_picking_readback()` should be moved to `GpuRenderer` trait or removed. `set_geometry_hdr_view()` / `set_tonemap_output_view()` are covered by section B. **Depends on:** section B completion.

### P2 - Resource Management
- [x] **Fix pending readback cleanup** — Upgraded warn to error log level in `VulkanRenderer::destroy()` and fixed stale comment referencing nonexistent `cleanup_on_exit()`.
- [x] **Use `Option` for nullable Vulkan handles** — GlobalParticleBuffer converted to Option<vk::Buffer>; remaining structs still need conversion.
- [x] **Add runtime bindless texture limit warnings** — `MAX_BINDLESS_TEXTURES = 4096` has no runtime check. Add warning when approaching limit, error when exceeded

### P3 - Error Handling
- [x] **Preserve error context in `RendererError`** — Many places drop original error: `format!("Failed to create {}", label)` loses `e`. Use `format!("{}: {:?}", label, e)`
- [x] **Make default trait impls fail explicitly** — `update_texture()`, `recompile_materials_for_shader()`, and other default no-ops should return `Err(RendererError::InvalidOperation(...))` instead of `Ok(())`
- [x] **Add Metal error types** — Standardized error types across lighting, shadow buffers, and entire particles subsystem. Added `From<String>` and `From<&str>` for `RendererError`.

### P4 - Performance
- [x] **Reduce `Rc<VulkanContext>` cloning in initialization** — `VulkanRenderer::init()` clones context 8+ times. Pass `&Rc<VulkanContext>` where possible
- [x] **Cache frame graph barrier compilation** — Added PassBarrierCache with dirty flag, only recompiles when graph structure changes.
- [x] **Batch single-time commands** — Batched particle buffer initialization from 4 submit-wait cycles to 1.

## Future Bugs (spotted, not in scope)

These were observed while working on other tasks and noted here for future cleanup. Each is real but not blocking the current roadmap.

- [x] **SceneSnapshot doesn't preserve physics components** — Fixed: `spawn_from_descriptor()` now restores RigidBody, ColliderShape, PhysicsMaterial, TriggerVolume, CollisionFilter from EntityDescriptor. Physics bodies survive play/stop cycles.

- [ ] **Metal particle subsystem is entirely `#[cfg(test)]`** — `katla_gfx/src/metal/particle.rs` (967 lines) has `create_emitter`, `update_emitter`, `destroy_emitter`, `burst`, `get_emitters`, `update`, and `dispatch_compute` all gated behind `#[cfg(test)]`. The subsystem is initialized by `MetalRenderer::init_particle_system()` (line ~1220) but is **unreachable from production code** — it can never receive emitters, never step compute, never render. **Impact**: particles only work on Vulkan. The "Particles pass dispatch through `RenderGraphBackend` on Metal" TODO item (E in Backend Abstraction Cleanup) is the correct unblocker. **Fix scope**: multi-day. Remove the `#[cfg(test)]` gates, then wire `MetalParticleSubsystem::update` + `dispatch_compute` into the Metal frame render path (mirroring `katla_app/src/application/renderer.rs:300-352` which currently does `unwrap_vulkan().particle_system` only), and add a `ParticlePass` for Metal through the render graph.

- [x] **Default scene path inconsistency between test sync and runtime** — Fixed: added `default_scene_path()` that resolves via `CARGO_MANIFEST_DIR` to the workspace root, used by both tests and runtime. Tests now write/read the canonical workspace-root `assets/scenes/default.katla` regardless of cwd.

### P5 - API Design
- [ ] **Reduce public API surface** — `lib.rs` exposes 80+ items. Many internal modules shouldn't be public. Audit and restrict one module at a time:
  - [ ] Audit `lib.rs` exports — identify which items are used outside katla_gfx vs internal-only
  - [ ] Make `barrier` module `pub(crate)` — likely internal-only
  - [ ] Make `sync` module `pub(crate)` — likely internal-only
  - [ ] Make `pipeline` module `pub(crate)` — likely internal-only
  - [ ] Review remaining 80+ items and restrict visibility where possible
- [ ] **Consolidate frame lifecycle methods** — `begin_frame()`/`end_frame()` vs `render()` vs `wait_for_frame()` — confusing, multiple ways to do same thing
- [ ] **Replace `Rc<RefCell<ShaderCache>>` with better pattern** — Interior mutability + reference counting. Consider `Arc<Mutex<ShaderCache>>` or restructure to avoid shared mutation

### P6 - Code Quality
- ~~**Remove unused imports in `backend/mod.rs`**~~ — Stale. No `#[allow(unused_imports)]` annotations remain in katla_gfx.
- [ ] **Fix inconsistent naming** — rename to consistent patterns one group at a time:
  - [ ] Unify frame data naming — `swap_data` vs `frame_context`, pick one name
  - [ ] Unify resource manager naming — `asset_registry` vs `mesh_manager` vs `texture_manager`, pick consistent suffix
  - [ ] Unify GPU resource naming — `bindless_manager` vs `storage_manager`, pick consistent prefix/suffix
- [ ] **Add missing documentation for `GpuRenderer` trait** — many public items lack `///` docs. Add docs grouped by functionality:
  - [ ] Document lifecycle methods — `init()`, `begin_frame()`, `end_frame()`, `render()`, `wait_for_frame()`, `destroy()`
  - [ ] Document resource creation methods — texture, buffer, pipeline creation methods
  - [ ] Document drawing methods — draw, dispatch, and pass-related methods
  - [ ] Document query/state methods — timestamp queries, readback, synchronization
