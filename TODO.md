# TODO

## Backend Abstraction Cleanup

### C. Unify pipeline initialization — eliminate Metal-specific methods on AnyRenderer

- [ ] Add `set_geometry_hdr_view` and `set_tonemap_output_view` to `GpuRenderer` trait — needs backend-agnostic texture view type

### D. Clean up `cfg(target_os = "macos")` gating in AnyFrameGraph / AnyFrame

- [ ] Remove `transient_image_view_metal()` and `transient_texture_metal()` Metal-only methods from `AnyFrameGraph` — requires backend-agnostic texture view type
- [ ] Audit `AnyFrameGraph` for all `#[cfg(target_os = "macos")]` branches that could be collapsed — verified: only enum variants, match arms, and two Metal-specific accessors (`transient_image_view_metal`, `transient_texture_metal`) remain; the accessors require a backend-agnostic texture view type to remove (same blocker as D item above)

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

- [ ] Add background decode thread infrastructure — create a thread that owns `StreamingDecoder` instances and fills ring buffers ahead of the audio thread's read position
- [ ] Refactor `StreamingVoice::fill_ring_buffer()` to consume from the pre-filled ring buffer without performing I/O
- [ ] Wire background decode thread lifecycle (start/stop) into `AudioEngine` init/shutdown

### Phase 15: Audio quality and robustness
- [x] Add automatic fade-in/fade-out on voice start/stop — voices currently start and stop instantly with no gain ramp, causing audible clicks/pops. Add a short (1-5ms) linear fade-in when a voice begins playback and a fade-out when stopped, before marking it finished. This is standard practice in all production audio engines (Kira, FMOD, Wwise).
 - [x] Add configurable tween duration — `Voice::tween_smoothing` and `StreamingVoice::tween_smoothing` are hardcoded to 0.3 with no API to change them. Kira uses time-based tweens (e.g., `Tween { duration: 200ms }`). Expose tween duration or speed as a parameter on `VoiceHandle::set_volume_tweened()` etc.
- [ ] Add per-voice aux send levels — aux buses currently accumulate a copy of the entire main mix at a fixed `send_level`. Production audio engines allow each voice to have its own send level to each aux bus (e.g., a specific SFX sends 50% to reverb while music sends 0%). Add a `sends: Vec<(AuxBusId, f32)>` field to `Voice` and `StreamingVoice`.
- [ ] Add voice steal/priority system — there is no limit on the number of simultaneous voices. With enough concurrent sounds, the mix saturates and quality degrades. Add a maximum voice count and a priority-based voice stealing mechanism (lowest priority voice is stopped to make room for a new one).
- [ ] Add voice pooling — voices are allocated and deallocated every time a sound plays/stops, causing allocation pressure in the audio thread's Mutex. Pre-allocate a fixed pool of Voice objects and reuse slots.
- [ ] Improve resampling quality — both `Voice` and `StreamingVoice` use linear interpolation for sample rate conversion and pitch shifting. For production quality, add at least cubic (Catmull-Rom) interpolation, optionally sinc for offline/bounce. Linear interpolation causes audible artifacts with high-frequency content.
- [ ] Add proper reverb stereo decorrelation — `ReverbEffect` processes a mono sum of the input and applies the same mono reverb to both channels, collapsing stereo image. Use separate delay lines for left/right with slightly different delay times, or process L/R independently with decorrelation filters.
- [ ] Add audio device hot-swap — when the output device disconnects (headphones unplugged, Bluetooth disconnected), `cpal` fires the error callback but there is no recovery. Detect device changes via cpal's device change events and recreate the audio stream on the new default device.
 - [x] Add silence detection for streaming voices — `StreamingVoice::mix_into()` processes the full output buffer even when volume is 0.0 (only skips when `voice_volume == 0.0`, but tweening can make this check imprecise). Add an early-out when the voice has been silent for multiple consecutive frames.

### Phase 16: Feature parity with production audio engines
- [ ] Add audio clock/timeline — no way to schedule audio events at specific times or sync playback to game time. Add an audio clock (sample-accurate position counter) and the ability to schedule play/stop/volume changes at specific clock positions. Required for music synchronization and cutscene audio.
- [ ] Add audio file metadata query — no way to query duration, sample rate, or channel count of an audio file without fully decoding it. Add `AudioBuffer::from_path_metadata()` or similar that reads headers only (WAV fmt chunk, OGG/MP3 frame headers) without decoding the entire file. Needed for the asset browser duration display.
- [ ] Add looping crossfade support — seamless loop transitions currently just jump from `loop_end` to `loop_start`, which can cause clicks if the waveform doesn't align. Add a short crossfade region at the loop point (mix the tail of the loop with the head of the next iteration).
- [ ] Add playback position query — no way to query the current playback position of a voice (in seconds or samples). Add `VoiceHandle::position() -> f32` and `StreamingVoiceHandle::position() -> f32` for UI scrub bars, subtitle sync, and gameplay triggers.
- [ ] Add seek API for streaming voices — `StreamingVoiceHandle` has no seek method. Add `StreamingVoiceHandle::seek(position: Duration)` to allow scrubbing to arbitrary positions in a streaming file.
- [ ] Add audio recording/bounce — no way to capture the final mix output to a file. Add an offline render mode that writes the mixed output to a WAV file, useful for exporting game audio or cutscene bounces.

### Phase 17: Audio system activation and global settings
- [ ] Add AudioSettings to Preferences — `Preferences` struct has no audio fields. Add: `master_volume: f32`, `sfx_volume: f32`, `music_volume: f32`, `ambient_volume: f32`. Serialize to `preferences.toml`. Apply to `AudioEngine` on startup and on change.
- [ ] Add Audio tab to preferences panel — currently only General, Viewport, and AI tabs exist. Add an Audio tab with: master volume slider, SFX volume slider, music volume slider, ambient volume slider. Changes should apply immediately (live preview) and persist to `preferences.toml` on save.
- [ ] Apply saved audio settings on startup — after `AudioSystem::new()`, read `Preferences::audio_settings` and call `engine.set_master_volume()`, `engine.set_category_volume()` for each category. Currently all volumes reset to 1.0 every launch.
- [ ] Add AudioSource inspector UI — `AudioSource` component exists but has no inspector section. Add a read-only section showing: source file path, sample rate, channel count, duration. Add a "Play Preview" button to audition the clip.
- [ ] Add AudioListener indicator in inspector — `AudioListener` component exists but has no UI. Add a minimal inspector section showing which entity is the active listener (there should be only one). Warn if multiple AudioListener components exist.

### Phase 18: Audio mixer UI
- [ ] Add peak/RMS level computation in `AudioMixer::render()` — compute per-category and master peak and RMS levels during the render callback
- [ ] Add atomic double-buffered level snapshots — write levels in audio thread, read in UI thread without locking; one write buffer, one read buffer, swap on read
- [ ] Add VU meter widget to katla_ui — vertical bar showing peak/RMS with peak hold falloff, color-graded (green/yellow/red)
- [ ] Add mixer panel layout — dockable panel with master bus fader + VU meter, SFX/Music/Ambient sub-buses with faders + VU meters, aux bus sends with wet/dry controls
- [ ] Add voice pool status display — show active voice count, peak voice count, and which voices are playing (with name/category/volume) in the mixer panel or a debug overlay
- [ ] Add reverb zone visualizer — `ReverbZone` components exist but are invisible in the editor. Draw wireframe boxes/spheres showing reverb zone extents with color-coding for decay/wet parameters, similar to physics collider visualization.

## Physics

### Phase 6: Physics component scene serialization

- [ ] **Add `RigidBodyDescriptor`** — enum with Static, Dynamic, Kinematic variants; add to `EntityDescriptor`
- [ ] **Add `ColliderShapeDescriptor`** — enum with Sphere(radius), Box(half_extents), Capsule { half_height, radius } variants; add to `EntityDescriptor`
- [ ] **Add `PhysicsMaterialDescriptor`** — struct with friction, restitution, density fields; add to `EntityDescriptor`
- [ ] **Add `TriggerVolumeDescriptor` and `CollisionFilterDescriptor`** — trigger volume as unit struct; collision filter with layers/mask; add both to `EntityDescriptor`
- [ ] **Implement save path for Rapier physics components** — In `serialization.rs` scene save, read `RigidBody`, `ColliderShape`, `PhysicsMaterial`, `TriggerVolume`, `CollisionFilter` from ECS entities and convert to their descriptor types. Skip runtime-only fields (handles, velocities, overlapping_entities).
- [ ] **Implement load path for Rapier physics components** — In `serialization.rs` scene load, create ECS components from physics descriptors and add them to spawned entities. The RapierPhysicsSystem will then auto-discover and spawn them in Rapier.
- [ ] **Remove hardcoded `spawn_physics_demo_objects()`** — Once physics components serialize to scene files, replace the hardcoded init spawn with physics objects in the default `.katla` scene. The demo objects currently have no mesh/drawable, making them invisible. The scene-file objects should have visible meshes (cube/sphere primitives) alongside their colliders.
- [ ] **Add physics entities to default.katla scene** — Add a static floor plane with box collider + PBR material, and several dynamic spheres/cubes with colliders + PBR materials at various heights. These should be visible (have drawable + mesh) and demonstrate physics on scene load.

### Phase 7: Physics entity lifecycle

 - [x] **Handle entity destruction for joints** — Same issue: joints referencing destroyed entities leak Rapier joint handles. Add cleanup for `Joint` components whose `entity_a` or `entity_b` no longer exist.
- [x] **Add entity despawn callback for physics** — When the editor removes a `RigidBody` or `ColliderShape` component from an entity, the corresponding Rapier handles should be cleaned up. Wire into the existing `EditorAction::RemoveComponent` handler.

### Phase 8: Collider mesh fitting, shape types, and prefabs

- [ ] **Add geometry data cache for mesh vertex positions** — CPU-side vertex/index data exists in `GLTFModel` at load time but is discarded after GPU upload. `MeshHandle` has no readback path. Add a geometry cache (e.g. `HashMap<MeshHandle, Arc<MeshGeometryData>>`) that retains vertex positions and triangle indices alongside `MeshHandle`, populated during mesh loading before GPU upload discards the data. This is a prerequisite for trimesh, convex hull, and any mesh-derived collider generation.
- [ ] **Extend `ColliderShape` enum with mesh-derived variants** — Add `ColliderShape::Trimesh`, `ColliderShape::ConvexHull`, and `ColliderShape::Heightfield` variants alongside existing Sphere/Box/Capsule. Trimesh stores vertex positions + triangle indices (for static environment geometry). ConvexHull stores vertex positions (for dynamic props). Heightfield stores a 2D height grid (for terrain). All three reference data from the geometry cache rather than duplicating it.
- [ ] **Wire new `ColliderShape` variants through `collider_shape_to_rapier()`** — In `physics_world.rs`, add Rapier `SharedShape` construction for Trimesh (via `SharedShape::trimesh`), ConvexHull (via `SharedShape::convex_hull`), and Heightfield (via `SharedShape::heightfield`). Wire the geometry cache lookup so the system can resolve the mesh data at spawn time.
- [ ] **Implement trimesh collider generation for static environment meshes** — For static environment geometry (floors, walls, level architecture), generate exact trimesh colliders from the mesh's vertex/index data. Add an editor action or auto-detection: when a static `RigidBody` entity has a mesh, default to trimesh collider. Trimesh colliders only work with static bodies in Rapier.
- [ ] **Implement convex hull collider generation for dynamic props** — For dynamic/kinematic objects with complex meshes, compute a convex hull from vertex positions using Rapier's `SharedShape::convex_hull`. Convex hulls support dynamic simulation (unlike trimesh) but are approximate — they enclose the mesh but may have gaps. Add editor action to convert a mesh entity's collider to convex hull.
- [ ] **Implement capsule auto-fit from mesh dimensions** — Capsule colliders are ideal for character-like objects (humanoids, pillars, barrels). When auto-fitting a collider, compute the mesh AABB and check if it is tall and narrow (height > 2 × width). If so, generate a `CapsuleShape { half_height: height/2 - radius, radius: width/2 }` instead of a box. Add capsule as an explicit option in the editor collider type picker so users can override auto-fit.
- [ ] **Implement best-fit shape selection logic** — When auto-generating a collider for a mesh entity, choose the best shape type based on mesh characteristics: (a) sphere if AABB is roughly cubic and small, (b) capsule if tall/narrow (height > 2 × width), (c) box for general shapes, (d) convex hull for complex dynamic props, (e) trimesh for static environment geometry. This replaces the current box-only auto-fit.
- [ ] **Design collider cache system** — Computing convex hulls/trimesh colliders from mesh data is expensive. Design a collider cache that: (a) stores computed Rapier `SharedShape` instances keyed by mesh handle + shape type, (b) reuses cached shapes when multiple entities share the same mesh, (c) invalidates when the mesh changes (hot reload). This avoids recomputing hull decompositions every frame or on every entity spawn.
- [ ] **Update editor collider type picker UI** — The editor inspector for `ColliderShape` currently shows Sphere/Box/Capsule dropdown. Extend to show all shape types: Sphere, Box, Capsule, Trimesh, ConvexHull, Heightfield. When switching type, reset to auto-fit dimensions from the entity's mesh bounds. Disable Trimesh for non-static bodies (Rapier constraint). Disable Heightfield for non-mesh entities.
- [ ] **Design prefab system for physics objects** — Physics entities (e.g., a "bouncy ball" with sphere collider + PBR material + dynamic body) are currently assembled manually each time. Design a prefab/template system that bundles a set of components (mesh, material, collider, rigid body) into a reusable definition that can be instantiated multiple times. This would also benefit non-physics entities.
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

- [ ] **Expose `apply_force` / `apply_impulse` to Luau scripts** — Scripts can raycast but cannot apply forces or impulses to physics bodies. Add `world:apply_force(entity_id, force: Vec3)` and `world:apply_impulse(entity_id, impulse: Vec3)` script bindings.
- [ ] **Expose body velocity read/write to scripts** — Add `world:get_velocity(entity_id) -> Vec3` and `world:set_velocity(entity_id, velocity: Vec3)` for script-driven physics control.
- [ ] **Expose trigger volume queries to scripts** — Scripts should be able to check if an entity with a `TriggerVolume` is currently overlapping with specific entities, not just receive enter/exit events.
- [ ] **Add physics collision event scripting** — Wire `PendingPhysicsEvents` into the script event system so scripts can subscribe to collision events via `world:on_event("collision_enter", callback)` instead of needing a separate resource.

## Rendering

### Metal rendering bugs
- [ ] Billboard icons don't show in Metal
- [ ] Animated fox (skinned mesh) doesn't show in Metal
- [ ] Particle systems don't show in Metal

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
- [ ] Add planar reflection pass — render scene from reflected camera for flat reflective surfaces (water, mirrors)
- [ ] Integrate planar reflections into material system — bind reflection texture on materials with reflective property

## Scripting & Game Logic

### Gameplay framework
- [ ] Design game state machine — states (Menu, Loading, Playing, Paused, Cutscene), transitions, enter/exit hooks
- [ ] Implement `GameState` enum and `GameStateMachine` — state stack (push/pop), transition hooks (`on_enter`, `on_exit`), per-state update dispatch
- [ ] Add `GameStateManager` as ECS resource — accessible by systems and scripts; systems query current state to conditionally run
- [ ] Design gameplay event system — `EventBus<E>` generic typed event bus for gameplay-level events (OnDamage, OnCollect, OnCollision, etc.) decoupled from ECS events
- [ ] Implement `EventBus` — `emit(event)`, `subscribe(handler)`, `drain()` per frame; type-erased storage for multiple event types
- [ ] Design cutscene/timeline data model — `Timeline` asset with tracks (animation, audio, camera, event), keyframes per track, duration
- [ ] Implement timeline playback — `TimelinePlayer` component with play/pause/scrub, evaluate all tracks at current time, dispatch results
- [ ] Add timeline editor UI — track lanes, keyframe diamonds, scrubber bar, playback controls (depends on Editor dockable layout)

## Asset Pipeline

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
- [ ] Double-click (or open in context menu) items in the asset browser to open a dedicated floating window with the selected item as context — model preview, material preview, code editor (for scripts), image viewer, audio player, etc.

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

- [ ] Create `katla_ui/src/declarative/constructors.rs` — module for all free functions. Each is a plain `fn foo(...) -> ViewDescriptor` that fills defaults for optional fields. Start with the trivial ones: `empty()`, `toggle()`, `radio()`, `property_row()`, `color_picker()`.
- [ ] Add leaf free functions with optionals — `text()`, `button()`, `image_button()`, `slider()`, `labeled_slider()`, `textfield()`, `progress()`, `image()`. All optional fields default to `None` / `false` / `0`. These work standalone even before modifier methods exist.
- [ ] Add container free functions with `impl IntoIterator` — `hstack()`, `vstack()` take `impl IntoIterator<Item = ViewDescriptor>`, `zstack()` takes `impl IntoIterator<Item = (Alignment, ViewDescriptor)>`. Single-child containers (`scroll`, `panel`, `overlay`, `statusbar`) take `ViewDescriptor` directly. `draggle_panel`, `menubar`, `tree_view` take their specific struct args.
- [ ] Add modifier methods on `ViewDescriptor` — implement in `descriptor.rs` as `impl ViewDescriptor`. Each method consumes `self`, pattern-matches to the relevant variant(s), updates the field, returns `self`. Start with: `.color()`, `.font_size()`, `.fill()`, `.hover()`, `.border()`, `.on_click()`, `.enabled()`, `.on_submit()`, `.show_value()`, `.precision()`, `.label_width()`, `.uv()`. Add `debug_assert!` in the else branch to catch misapplied modifiers in tests.
- [ ] Add container modifier methods — `.spacing()`, `.padding()`, `.padding_all()`, `.align()` on `HStack`/`VStack`. `.padding()` on `ZStack`. `.header_height()` on `Panel`. `.close_on_outside()` on `DraggablePanel`. `.right_content()` and `.height()` on `MenuBar`. `.row_height()`, `.indent()`, `.on_select()`, `.on_right_click()` on `TreeView`.
- [ ] Re-export everything from `declarative/mod.rs` — `pub use constructors::*` so users write `use katla_ui::declarative::{text, button, hstack};`.
- [ ] Add unit tests — for each free function, verify it produces the correct `ViewDescriptor` variant with expected defaults. For each modifier, verify it sets the field and that misapplied modifiers no-op (test the `debug_assert!` fires).
- [ ] Refactor `StatusBarView` — replace all struct-literal `ViewDescriptor::Text` and `ViewDescriptor::HStack` with `text().color()` and `hstack().spacing().padding_all().align()`. First real consumer, validates the end-to-end feel.
- [ ] Refactor `GizmoButtonsView` — replace `RadioButton` struct literals with `radio()`, `HStack` with `hstack().spacing().padding_all()`.
- [ ] Refactor `EditorRootView` — replace `ZStack` struct literal with `zstack([...])`.
- [ ] Refactor `helpers.rs` — rewrite `section_header()`, `delete_button()` to use free functions + modifiers internally.

#### Prerequisites: layout and diffing infrastructure

- [ ] Replace heuristic text measurement with real font metrics — `measure_text_descriptor()` currently uses `char_count * height * 0.6`. Use the existing `FontSystem` to measure actual glyph advances for the layout string, so Taffy flexbox sizes match what the renderer draws.
- [ ] Add stable child identity for list diffing — add an optional `key: Option<u64>` to `StackDescriptor` children (or a `KeyedChild` wrapper) so diffing can match children by identity instead of index. Prevents state corruption and spurious animations when list order changes.

#### Widget gaps: missing declarative features needed for migration

- [ ] Add `Section` descriptor — collapsible section with header row (label + optional remove button + expand/collapse chevron). Equivalent to the `section_header()` helper but as a proper container variant. Needed by Inspector.
- [ ] Add `TabBar` descriptor — tab strip with selectable tabs, content area below. Equivalent to immediate-mode `begin_row` with styled buttons. Needed by Preferences.
- [ ] Add `Grid` descriptor — `GridDescriptor { columns: usize, cell_size: Vec2, spacing: f32, children: Vec<ViewDescriptor> }` mapped to a wrapping Taffy flex layout. Needed by Preferences and Viewport Grid.
- [ ] Add `Separator` descriptor — horizontal or vertical divider line. Needed by most panels.
- [ ] Add `Icon` descriptor — render a ForkAwesome icon by char code with configurable size and color. Needed by Toolbar and Inspector.
- [ ] Add `ProgressBar` improvements — add optional label text overlay on the progress bar track.
- [ ] Add `Selectable` descriptor — wrapper that highlights on hover and fires on_click, for list items and grid cells. Needed by Asset Browser and Hierarchy.
- [ ] Add `Conditional` descriptor or extend `show_if` helper — support `if`/`else` branching in descriptor trees with stable identity on both branches so diffing doesn't destroy state.

#### Phase 1: Migrate simple panels (build confidence)

- [ ] Migrate Viewport Grid panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `ViewportGridDrawCtx`, inject data via `Environment`, build a `Grid` or `VStack` of `Image` + `Text` cells with hit-testing via `Selectable` descriptors. Remove `set_viewport_grid_ctx`/`take_viewport_grid_ctx`.
- [ ] Migrate Toolbar panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `ToolbarDrawCtx`, inject via `Environment`, build `MenuBar` with `MenuGroup` dropdowns and `ImageButton` descriptors. Remove `set_toolbar_ctx`/`take_toolbar_ctx`.
- [ ] Migrate Gizmo panel fully declarative — already uses `RadioButton` descriptors but reads `GizmoDrawCtx` from `Environment` via thread-local. Move the gizmo data to `Environment` only, remove any thread-local remnants.

#### Phase 2: Migrate medium panels

- [ ] Migrate Preferences panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `PreferencesDrawCtx`, inject via `Environment`. Use `TabBar` for General/Viewport/AI tabs, `Grid` for label+widget rows, `LabeledSlider`/`Toggle` for settings. Remove `set_preferences_ctx`/`take_preferences_ctx`.
- [ ] Migrate Co-Creator panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `CoCreatorDrawCtx`. Use `DraggablePanel`, `ScrollView` with markdown-rendered `Text` rows, `TextField` for input. Remove `set_co_creator_ctx`/`take_co_creator_ctx`.
- [x] Migrate Particle Inspector panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `ParticleInspectorDrawCtx`. Use `DraggablePanel`, `Section` for particle modules (emitter, color over lifetime, size over lifetime), `LabeledSlider`/`Vec3Slider` per module. Remove `set_particle_inspector_ctx`/`take_particle_inspector_ctx`.

#### Phase 3: Migrate complex panels

- [ ] Migrate Hierarchy panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `HierarchyDrawCtx`. Use `TreeView` descriptor with `TreeItem` data from `Environment`, `ContextMenu` for right-click actions, `on_select` callback. Remove `set_hierarchy_ctx`/`take_hierarchy_ctx`.
- [ ] Migrate Inspector panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `InspectorDrawCtx`. Use `DraggablePanel`, `Section` per component with `delete_button`, `LabeledSlider`/`Vec3Slider`/`Toggle`/`ColorPicker` per field, `Modal` for Add Component picker. This is the hardest migration. Remove `set_inspector_ctx`/`take_inspector_ctx`.
- [ ] Migrate Console panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `ConsoleDrawCtx`. Use `DraggablePanel`, `ScrollView` with `Text` rows (colored by log level), `TextField` for command input with `on_submit`. Remove `set_console_ctx`/`take_console_ctx`.
- [ ] Migrate Asset Browser panel from `ViewDescriptor::Custom` to declarative tree — remove thread-local `AssetBrowserDrawCtx`. Use `Grid` or custom `Selectable` grid for thumbnails, `ContextMenu` for right-click, `TextField` for search, `Modal` for rename/delete confirmations. Remove `set_asset_browser_ctx`/`take_asset_browser_ctx`.

#### Cleanup: remove legacy code

- [ ] Remove all thread-local `RefCell<Option<DrawCtx>>` bridges — `set_*_ctx`/`take_*_ctx` functions for every migrated panel. Verify no remaining `thread_local!` blocks in `editor_ui/`.
- [ ] Remove or gate `ViewDescriptor::Custom` escape hatch — make it `#[cfg(test)]` or remove entirely once all panels are migrated. If kept for extensibility, document the constraints (no diffing, no state, no layout).
- [ ] Remove immediate-mode builder widgets that have declarative equivalents — `Button`, `Slider`, `LabeledSlider`, `Vec3Slider`, `ToggleButton`, `TextInput`, `RadioButton`, `ImageButton`, `Panel` from `widgets/mod.rs` public API. Keep only widgets with no declarative counterpart (e.g. `DockArea`).
- [ ] Add `ViewDescriptor` construction tests — unit tests for the builder constructors, diff correctness (including keyed children), and layout for each new container variant.
- [ ] Add declarative integration tests — frame-level tests that build a descriptor tree, run `ViewTree::frame()`, assert bounds, actions, and state mutations for each widget type. Cover the gaps identified in review: no tests for `diff_descriptor`, `ViewTree::sync_tree`, `TransitionContainer`, `DockArea`, `ColorPicker`, `BindingResolver`.

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
- [ ] Design integration test framework — headless app init, entity spawning, frame execution, state assertions
- [ ] Add render test infrastructure — render N frames, read back pixels, compare against golden images
- [ ] Add ECS round-trip tests — spawn entity, add components, serialize, deserialize, verify equivalence
- [ ] Add headless CI test suite — run integration tests without GPU in CI (mock renderer or software rasterizer)
