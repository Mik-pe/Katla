# TODO

## Backend Abstraction Cleanup

### C. Unify pipeline initialization — eliminate Metal-specific methods on AnyRenderer

- [ ] Add `set_geometry_hdr_view` and `set_tonemap_output_view` to `GpuRenderer` trait — needs backend-agnostic texture view type

### D. Clean up `cfg(target_os = "macos")` gating in AnyFrameGraph / AnyFrame

- [ ] Remove `transient_image_view_metal()` and `transient_texture_metal()` Metal-only methods from `AnyFrameGraph` — requires backend-agnostic texture view type
- [ ] Audit `AnyFrameGraph` for all `#[cfg(target_os = "macos")]` branches that could be collapsed — verified: only enum variants, match arms, and two Metal-specific accessors (`transient_image_view_metal`, `transient_texture_metal`) remain; the accessors require a backend-agnostic texture view type to remove (same blocker as D item above)

### E. Align Metal backend with shared FrameGraph<B> execution path

- [ ] Verify `Frame<'_, MetalRenderer>` execution dispatches all pass types (geometry, shadow, fullscreen, compositing, particles, outline, UI, depth prepass) through `RenderGraphBackend` trait methods — Partial: Metal routes 5 of ~9 pass kinds; missing particles, compositing, stencil-indicator, generic compute
- [ ] Ensure Metal backend's `render_frame()` goes through `FrameGraph<MetalRenderer>::execute()` identically to the Vulkan path, not through a separate hardcoded pass sequence — Metal uses `collect_draw_lists()` + hardcoded `render_frame()`, not `FrameGraph::execute()`
- [ ] Remove any remaining dual-code-path divergence between how Vulkan and Metal execute the same frame graph — Requires migrating Metal from hardcoded pass sequence to data-driven graph execution

## Audio System

### Phase 14: Production bugs and correctness
- [ ] Move streaming decode off the audio thread — `StreamingVoice::fill_ring_buffer()` performs synchronous file I/O (via `StreamingDecoder`) inside the audio render callback under the mixer's Mutex. Disk reads can take milliseconds, causing audible glitching or callback timeouts. Fix: add a background decode thread that fills the ring buffer ahead of the read position, with the audio thread only consuming from the ring buffer (no I/O in callback). This was described in Phase 11 TODO but never implemented.

### Phase 15: Audio quality and robustness
- [ ] Add automatic fade-in/fade-out on voice start/stop — voices currently start and stop instantly with no gain ramp, causing audible clicks/pops. Add a short (1-5ms) linear fade-in when a voice begins playback and a fade-out when stopped, before marking it finished. This is standard practice in all production audio engines (Kira, FMOD, Wwise).
- [ ] Add configurable tween duration — `Voice::tween_smoothing` and `StreamingVoice::tween_smoothing` are hardcoded to 0.3 with no API to change them. Kira uses time-based tweens (e.g., `Tween { duration: 200ms }`). Expose tween duration or speed as a parameter on `VoiceHandle::set_volume_tweened()` etc.
- [ ] Add per-voice aux send levels — aux buses currently accumulate a copy of the entire main mix at a fixed `send_level`. Production audio engines allow each voice to have its own send level to each aux bus (e.g., a specific SFX sends 50% to reverb while music sends 0%). Add a `sends: Vec<(AuxBusId, f32)>` field to `Voice` and `StreamingVoice`.
- [ ] Add voice steal/priority system — there is no limit on the number of simultaneous voices. With enough concurrent sounds, the mix saturates and quality degrades. Add a maximum voice count and a priority-based voice stealing mechanism (lowest priority voice is stopped to make room for a new one).
- [ ] Add voice pooling — voices are allocated and deallocated every time a sound plays/stops, causing allocation pressure in the audio thread's Mutex. Pre-allocate a fixed pool of Voice objects and reuse slots.
- [ ] Improve resampling quality — both `Voice` and `StreamingVoice` use linear interpolation for sample rate conversion and pitch shifting. For production quality, add at least cubic (Catmull-Rom) interpolation, optionally sinc for offline/bounce. Linear interpolation causes audible artifacts with high-frequency content.
- [ ] Add proper reverb stereo decorrelation — `ReverbEffect` processes a mono sum of the input and applies the same mono reverb to both channels, collapsing stereo image. Use separate delay lines for left/right with slightly different delay times, or process L/R independently with decorrelation filters.
- [ ] Add audio device hot-swap — when the output device disconnects (headphones unplugged, Bluetooth disconnected), `cpal` fires the error callback but there is no recovery. Detect device changes via cpal's device change events and recreate the audio stream on the new default device.
- [ ] Add silence detection for streaming voices — `StreamingVoice::mix_into()` processes the full output buffer even when volume is 0.0 (only skips when `voice_volume == 0.0`, but tweening can make this check imprecise). Add an early-out when the voice has been silent for multiple consecutive frames.

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
- [ ] Add audio mixer panel — a dockable panel showing the current mix state: master bus with VU meter + fader, SFX/Music/Ambient sub-buses with VU meters + faders, aux bus sends with wet/dry controls. VU meters should show real-time peak/RMS levels from the mixer's render output.
- [ ] Add real-time level metering to AudioMixer — the mixer currently has no peak/RMS measurement. Add per-category and master level meters computed during `render()` using atomic double-buffered level snapshots (write in audio thread, read in UI thread). Required for the mixer panel VU meters.
- [ ] Add voice pool status display — show active voice count, peak voice count, and which voices are playing (with name/category/volume) in the mixer panel or a debug overlay. Useful for diagnosing voice leaks and tuning voice limits.
- [ ] Add reverb zone visualizer — `ReverbZone` components exist but are invisible in the editor. Draw wireframe boxes/spheres showing reverb zone extents with color-coding for decay/wet parameters, similar to physics collider visualization.

## Physics

### Phase 6: Physics component scene serialization

- [ ] **Add physics descriptors to EntityDescriptor** — `EntityDescriptor` has no fields for Rapier physics components. Add: `rigid_body: Option<RigidBodyDescriptor>` (body_type enum), `collider_shape: Option<ColliderShapeDescriptor>` (sphere/box/capsule variants with dimensions), `physics_material: Option<PhysicsMaterialDescriptor>` (friction, restitution, density), `trigger_volume: Option<TriggerVolumeDescriptor>` (unit struct / empty), `collision_filter: Option<CollisionFilterDescriptor>` (layers, mask).
- [ ] **Implement save path for Rapier physics components** — In `serialization.rs` scene save, read `RigidBody`, `ColliderShape`, `PhysicsMaterial`, `TriggerVolume`, `CollisionFilter` from ECS entities and convert to their descriptor types. Skip runtime-only fields (handles, velocities, overlapping_entities).
- [ ] **Implement load path for Rapier physics components** — In `serialization.rs` scene load, create ECS components from physics descriptors and add them to spawned entities. The RapierPhysicsSystem will then auto-discover and spawn them in Rapier.
- [ ] **Remove hardcoded `spawn_physics_demo_objects()`** — Once physics components serialize to scene files, replace the hardcoded init spawn with physics objects in the default `.katla` scene. The demo objects currently have no mesh/drawable, making them invisible. The scene-file objects should have visible meshes (cube/sphere primitives) alongside their colliders.
- [ ] **Add physics entities to default.katla scene** — Add a static floor plane with box collider + PBR material, and several dynamic spheres/cubes with colliders + PBR materials at various heights. These should be visible (have drawable + mesh) and demonstrate physics on scene load.

### Phase 7: Physics entity lifecycle

- [ ] **Handle entity destruction for joints** — Same issue: joints referencing destroyed entities leak Rapier joint handles. Add cleanup for `Joint` components whose `entity_a` or `entity_b` no longer exist.
- [ ] **Add entity despawn callback for physics** — When the editor removes a `RigidBody` or `ColliderShape` component from an entity, the corresponding Rapier handles should be cleaned up. Wire into the existing `EditorAction::RemoveComponent` handler.

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

- [ ] **Add test for static body spawn tracking** — Verify that static bodies correctly track their spawned state despite having no Rapier `RigidBodyHandle` (related to the invalid-handle fix in Phase 5).
- [ ] **Add test for entity destruction cleanup** — Spawn a dynamic body, destroy the entity, verify that `PhysicsWorld` body/collider counts decrease correctly.
- [ ] **Add test for joint spawning** — Create two entities with `RigidBody` + `ColliderShape`, add a `Joint` component referencing both, run one frame, verify the joint is created in `PhysicsWorld`.
- [ ] **Add test for play-mode gating** — Verify that physics simulation does not advance when play mode is `Editing` or `Paused`, and does advance when `Playing`.
- [ ] **Add integration test for physics scene round-trip** — Create entities with physics components, serialize to RON, deserialize, verify components are recreated correctly and Rapier bodies are spawned.
- [ ] **Add test for kinematic body sync** — Spawn a kinematic body, move its `TransformComponent`, run one frame, verify Rapier body position matches the new transform.
- [ ] **Add stress test for many dynamic bodies** — Spawn 100+ dynamic bodies, step for N frames, verify no panics or deadlocks. Identify performance bottlenecks in the spawn/sync loop.
- [ ] **Add test for `apply_force` and `apply_impulse` through ECS** — Current tests only verify body creation and gravity. Add tests that apply forces/impulses and verify velocity/position changes.

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

- [ ] Migrate remaining panels from ViewDescriptor::Custom to declarative trees
  - **Toolbar**: needs declarative MenuBar with dropdown hover-to-switch, icon buttons with callbacks
  - **Console**: needs declarative ScrollArea with per-row hit-testing, text selection, clipboard
  - **Preferences**: needs declarative DraggablePanel, tab bar, grid layout (begin_grid/grid_item)
  - **Inspector**: needs declarative DraggablePanel, modal (Add Component), ColorPicker overlay, section headers with remove buttons, dynamic enum-driven widget trees
  - **Hierarchy**: needs declarative TreeView with custom row rendering (icons, badges), ContextMenu integration
  - **Co-Creator**: needs declarative DraggablePanel, markdown rendering, multiline TextInput
  - **Particle Inspector**: needs declarative DraggablePanel, dynamic shape-parameter branching
  - **Viewport Grid**: needs dynamic texture grid with per-cell Image + border + label, mouse hit-testing for slot hover
  - **Asset Browser**: needs marquee selection, drag-and-drop, z-index tooltips, keyboard capture, context menu, modal — currently a no-op Custom wrapper
- [ ] Remove thread_local bridges from all migrated panels
- [ ] Remove immediate-mode widgets with declarative equivalents from widgets/mod.rs public API (Button, Slider, LabeledSlider, Vec3Slider, ToggleButton, TextInput, RadioButton, ImageButton, Panel)
- [ ] Restrict or remove ViewDescriptor::Custom escape hatch

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
