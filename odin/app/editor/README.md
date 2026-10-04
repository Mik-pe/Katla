# Canonical Odin editor

This package owns editor views and interaction state around one stationary
`app.Authoring`. `odin/katla` is the persistent native executable. CPU authoring,
tools, document publication and every editor mutation share the same registry,
application executor and undo history. Render, UI and native window services
remain independent owners. Native examples exercise the same production
adapters without creating another editor or history owner.

## Consumer inventory

The executable frame owner is `odin/katla/runtime.odin`. This package builds
the retained panel descriptors and routes their ordered actions. `odin/ui`
owns retained widgets, layout, text editing and docking; `app/document`,
`app/preferences` and `app/assets` own their persistent data and transactions.
`app/render` owns real scene, particle, UI, overlay and picking GPU consumers.

| Area | Actual exposed contract |
| --- | --- |
| Workspace | Resizable dock splits, movable/reordered/closable tabs, persistent layout, inactive tab state, clipped scroll regions, keyboard focus, captured drags outside the original control, modal and popup routing, tooltips, clipboard and native IME. |
| Document | New/Open/Save/Save As/Quit; path submission/cancel; unsaved Save/Discard/Cancel; overwrite confirmation; Stop required before file mutation; failed operations preserve scene, origin, history and native owners; authored dirty baseline excludes runtime animation clocks. |
| Hierarchy | Case-insensitive search with matching descendants exposed through collapsed parents; stable names and identity; expand/collapse, multiselection/range, external-pick reveal, create/delete/duplicate and drag reparent through atomic shared-history operations. |
| Inspector | Registered component add/remove; visible reflected fields with constraints/display names; nested scalar/vector/bool/string/enum/entity references; transform position/rotation/scale; linear material authoring with sRGB controls and six presets; grouped first-before/last-after undo gestures. |
| Components | Name, transform, point and directional lights, perspective, scripts and live variables, particle emitter, audio emitter/source/listener, velocity, reverb, collider/filter/body/physics material, source mesh/model, animation playback, behavior and scene triggers. Internal identity/source ownership remains protected. |
| Viewport | Single, two horizontal, two vertical and four real camera views; persistent camera assignment and active view; orbit/pan/zoom; interruptible focus to actual selection bounds; W/E/R transform gizmos, axis/plane hit testing, snapped manipulated axes, Escape and blur cancellation; committed ID picking; visible selection outline. |
| Simulation | Play/Pause/Resume/Stop restore authored state; actual animation/physics/Luau/particles/audio ticks; game input only reaches a focused viewport while playing, independently of text/UI input. |
| Assets | Confined Resource/Project browsing, accepted-navigation Back/Forward and breadcrumbs, search and multiselection, model/prefab insertion, source editing, native Reveal, delete confirmation/create folder and audio preview. Visible PNG/JPEG/BMP/TIFF thumbnails use bounded asynchronous decoding and native GPU/UI registration; failed replacements retain the accepted image. |
| Console | Actual log rows with timestamps/levels, level filters, clear, bounded retention and scrolling. |
| Mixer | Master/SFX/music/ambient volume, actual category peak/RMS meters and active/peak voice counts; device failure and unavailable output are visible. |
| Preferences | Appearance theme/font scale; viewport grid/stats/physics/reverb debug, camera speed/grid snap/grid size; four audio categories; existing external host socket/thread connection. Persist settings and dock JSON with finite bounds. |
| Animation timeline | Actual selected model clips/playback time/scrub/speed/loop and transitions through canonical animation operations; loaded skeleton/morph output updates native geometry. |
| Code editing | Rooted script tabs own text and saved baselines; multiline UTF8 caret/selection/clipboard, local text undo/redo, syntax/gutter/scrolling, dirty switch/close/quit decisions and atomic Save with Luau syntax admission and matching-instance reload. |
| Co-creator | Attach only the configured existing external conversation; turn/item-scoped messages, attention/error/disconnect, owned text admission, steer active turn, explicit exact-turn interrupt. A question attaches one completed paired color/ID viewport snapshot; it never creates another internal assistant or a new conversation. |
| Rendering/debug | Authored PBR textures, skin/morph animation, directional/point lighting, environment/grid, shadows, selection outline, exposure/tone mapping, particles, physics and reverb overlays. All UI samples real viewport outputs and uses real shaped fonts in the accepted native submission. |

## Ownership and frame contract

Selection uses explicit presence flags: ECS entity zero is valid. Selection can
follow persistent `Scene_Key` after load/undo creates fresh generations, while
queued commands retain their original generational target and reject stale IDs.
Hierarchy and inspector snapshots own their strings. Registry metadata is
borrowed only while its stationary authoring owner lives.

The retained `ui.Context` reconciles stable keys and owns widget state, text,
focus, capture and dock actions. The application mounts only active panel
contents. Native input owns committed/preedit text until UI, camera and game
consumers finish that frame. The NSView is a real first responder implementing
native text composition; drawable extents use backing pixels and UI uses logical
points. Renderer teardown precedes native view/window destruction.

CPU systems, asset/document transactions and cache uploads run before acquire.
Ordered widget actions also run before acquire, after hit routing, so a captured
gizmo or inspector preview can prepare native scene resources while no frame is
owned. Descriptor and action strings stay owned until all consumers finish.
One acquired frame token supplies every mounted viewport, paired
picking capture and the UI pass. The application publishes command results,
particle state and external-agent snapshots only after the native submission is
accepted. Readback completion retains exact frame/image/ID-map provenance across
resize or scene replacement. Deferred editor actions run on the scene owner.

The outer executable owner installs the Console logger and restores its caller's
logger only after the inner editor loop has joined every worker and destroyed its
resources. The bounded sink forwards messages to the original terminal logger;
its owner outlives all producers, including shutdown logging.

## Acceptance boundary

CPU checks prove ownership and command routing. Native renderer validators
exercise real pixels, readback provenance, source reload transactions and
failure preservation on Metal and Vulkan. `--interaction-test` drives retained
input and records accepted full-editor GPU images; it is separate from actual
OS pointer/keyboard acceptance. `--screenshot` captures the active viewport.

Actual macOS pointer journeys exercised snapped gizmo movement and one-step
Undo/Redo on both Metal and Vulkan, plus a Vulkan material drag released outside
Inspector. The Assets panel displayed a real image thumbnail on both backends;
native row selection was checked on Metal. A Vulkan four-view journey changed
all four outputs through the production shader reload family, retained the
accepted family after a compiler error, displayed and filtered the actual
Console warning, and cleared every paused particle view through global Reset.
Direct native committed text input was checked; these receipts do not establish
human IME preedit, every floating-dock gesture or accessibility coverage.

A rendered control, successful CPU test or synthetic input receipt alone does
not establish an OS journey. The windowless `--headless` path uses the same
authoring, four-view graph, UI renderer and committed paired capture owners.
