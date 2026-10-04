# Odin editor completion contract

The Odin application replaces the complete Katla editor and runtime. Existing
user-visible features must continue to work; package structure and external
dependencies may change. Completion requires the actual application consumers,
not independently passing library examples. Unresolved implementation work is
tracked in [TODO](../TODO.md#odin-port).

## Required behavior

| Area | Application contract |
| --- | --- |
| Startup | One canonical editor executable; resource discovery, scene selection, bounded/headless modes, screenshots and actionable initialization errors |
| UI | Retained declarative nodes, stable state, flex/grid layout in Odin, typed drained actions, clipping, popup/modal layering, focus, UTF-8 text, IME and scrolling |
| Docking | Active tabs, movable panels, splitters, floating panels and persisted layout without hidden-panel input or stale state |
| Documents | New/Open/Save/Save As/Quit; dirty title; Save/Discard/Cancel and overwrite decisions; failure preserves the current scene, path and history |
| Hierarchy | Search through collapsed ancestors, selection, create/delete/duplicate, component inspection and cycle-safe reparenting |
| Inspector | Registered component metadata, actual field editing, add/remove components and one shared agent/gesture undo history |
| Viewport | Orbit/pan/focus, manual camera takeover, W/E/R transforms, axis-specific snapping and focus-gated game input |
| Materials | Independent primitive surfaces, HDR emission, alpha coverage, five role images and UV/sampler policies, portable reusable material capture/apply, grouped drags with pointer capture |
| Assets | Confined discovery/read/create/write, supported model formats, reusable mesh/prefab authoring, browser selection and actual spawn/drop consumers |
| Animation | Model skin/morph sampling, clips, transitions, inspector controls and timeline behavior |
| Gameplay | Play/Pause/Resume/Stop, restored authored state with remapped identities, real Luau hooks, sandbox, input, events, hot reload and exposed world operations |
| Physics | Bodies, colliders, constraints, trigger/filter behavior, spatial queries and native resource ownership |
| Particles | GPU simulation/rendering, script/trigger requests, lifecycle, retired emitter slots and inspector controls |
| Rendering | Native Metal/Vulkan, textured/skinned models, directional/point lights, shadows, environment/grid, selection outlines and exposed tone-map settings |
| Audio | Supported files/metadata/streams, native playback, voices, categories, meters, scheduling/cues, spatial controls, DSP and the editor mixer/preview |
| Preferences | Bounded camera/grid/snap/font/theme/debug/audio/connection settings and coherent persistence |
| Assistant | Selected existing external conversation, streaming and cancellation, connection preferences and committed viewport context |
| MCP | Canonical tools on the live application owner, private socket/stdio routing, full generational identities, bounded admission and deferred viewport replies |
| Diagnostics | Console, layout/graph dumps, screenshots, interaction receipts and retained native readbacks |
| Shader builds | Separate compiler process, owned validated artifacts, source/options/compiler identity cache, refresh only when needed and failure retaining live pipelines |

Viewport images, picking samples and metadata describe the same committed GPU
submission. Selection is optional. Frustum intersection means a geometric
candidate and does not claim pixel visibility or semantic room membership.
Editor camera changes do not alter game cameras or scene files.

## Replacement and acceptance

The GPU core owns generic resources, frames and execution. UI draw lists and
texture identities remain independent of GPU ownership. The application owns
scene composition, document transitions, selection and integration services.
Shader compilation may use an external compiler build tool; the running Odin
editor does not link a Rust shader-compiler or layout runtime.

The owner routes retained widget actions after UI hit testing and before native
frame acquisition. Scene and inspector gestures can therefore stage their
resources without conflicting with an acquired frame. Action and descriptor
strings remain owned through the consumers of that frame. All mounted views,
picking and UI use one accepted graph submission.

Visible asset thumbnails admit at most four bounded background jobs per owner
frame. Confined source identities and content revisions select a native GPU/UI
image cache; failed reads, decoding or uploads preserve the previous accepted
image. Frame imports retain the exact images sampled by the UI.

An outer executable owner installs the bounded Console logger before creating
the editor and restores its caller's logger after the inner loop has joined all
workers. Shutdown logging remains valid until the final producer is retired.

Every application flow uses the canonical application owner and its history.
Staged file/asset/native failures preserve the last accepted state. Native GPU
acceptance enables backend validation and checks real output, resource lifetime
and recovery; CPU tests or screenshots alone do not establish that behavior.
Operating-system input, playback and external conversation attachment require
their own actual consumer evidence.

Authored deletion includes incoming joint and trigger-reference cleanup in the
same reversible command. Rules whose explicit other-entity filter is deleted
are removed; actions targeting a deleted entity are removed, and empty rules
are removed. Surviving actions and overlap identities remain ordered. Undo
restores the complete rules and maps their targets to fresh runtime identities.
Native admission rejects the whole proposal before any old identity is retired.
During play, runtime deletion instead retains authored trigger descriptors;
stale recipients produce diagnostics, and Stop restores the authored baseline.

Undo and Redo on existing entities apply only the component changes authored
by that command. Unchanged live components, animation clocks and queued events
keep their current values. A conflicting edit to an affected component rejects
the complete command before publication. Create/Delete commands retain complete
owned snapshots and remap references when restoring fresh identities. Gesture
previews use the same rule, so read-only observations do not cancel a drag.

Remove each superseded Rust subsystem together with its remaining consumers,
manifests, build steps and instructions after its replacement is integrated and
validated. Final completion requires the whole editor contract above and a
default build/run/CI path that uses the Odin application. There is no retained
parallel Rust editor or silent fallback to it.
