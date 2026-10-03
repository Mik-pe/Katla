# Scene and material authoring for agents

Connect to the running editor using [shared editor MCP](shared-editor-view.md).
Author in edit mode; use `simulation` for gameplay verification. `editor_view` returns a committed viewport PNG;
use it before editing and again to verify the result. Native Vulkan/Metal output
is the visual authority. Geometry queries alone cannot establish occlusion.
`editor_view observe` with `limit: 0` returns the native image and camera/picking
metadata without candidate rows. The candidate count and truncation remain visible.

For an authored example, launch `cargo run -- --scene assets/scenes/material-studio.katla`.
Select a material sphere to explore presets and live surface controls alongside
a small furnished lounge.

## Find things first

Use `query_entities` with `name_filter`, `component_filter` or a world-space
`position` and `radius`. Returned generational IDs are **decimal strings**: retain
them verbatim. `get_scene_hierarchy` gives parent relationships;
`list_available_components` and `get_component_attributes` expose editable fields.
Query rows and viewport candidates expose `material_editable`; choose true rows
before building a material batch.

`search_assets` searches recursively under the discovered resource root. All
whitespace-separated words must match the relative path, case-insensitively.
Results are sorted and include `total` and `truncated`; the default limit is 64,
maximum 256. Symlinks are skipped. Search before choosing a model filename:

```json
{"query":"chair", "extensions":["glb", "gltf"], "limit":32}
```

Pass the returned path directly to `spawn_model`, for example
`{"path":"models/Lantern.glb", "position":[1,0,-2]}`. Model paths are relative
to the resource root, independent of the editor's working directory. Absolute
paths and parent traversal are rejected. An empty search query lists assets. `assets` paths are resource-relative for
`spawn_model` and script attachment. `project_paths` include the resource-root
directory and are ready for the project-relative `prefab` tool.
Model spawn receipts include `root_entity_id`, all created `entities` and
`material_entity_ids`. Multi-primitive models have a transform/animation
controller and separate material-editable children. Apply material patches to
those drawable IDs; the controller is not material-editable. Each child preserves
its own material and textures across save/load. A controller's playback drives
skinned children unless they have explicit playback overrides.
Use `list_resources`/`read_resource` for project files such as scene documents.

## Edit surfaces without touching GPU handles

The `material` tool and Inspector → Material edit the same per-object PBR factors.
They preserve model textures, mesh geometry and GPU material handles. Presets
are flat PBR tints, rather than scanned wood, concrete or fabric textures.

```json
{"action":"presets"}
```

```json
{"action":"inspect", "entity_id":"4294967302"}
```

```json
{"action":"set", "entity_ids":["4294967302","4294967303"],
 "preset":"plaster", "base_color":[0.88,0.85,0.79,1.0], "roughness":0.85}
```

Available presets: `plaster`, `oak`, `concrete`, `ceramic`, `brushed_metal`, `fabric`.
Preset discovery and inspection return `capabilities`, including alpha's current
effect, supported surface factors, unavailable texture editing and the batch contract. Presets supply
isotropic scalar factors; `brushed_metal` installs no directional brushing.
Explicit fields override the preset. Omitting the preset patches only supplied
fields. `base_color` is sRGB RGB with linear alpha; `metallic`, `roughness` and `ao` are linear
factors within 0..=1. `emissive_factor` is nonnegative linear RGB and accepts values
above one for HDR self-illumination, including materials without an emissive texture.
`normal_scale` is finite and multiplies the decoded normal map's X/Y components; zero flattens it and negative values invert both tangent axes.
`alpha_mode` selects `opaque`, `mask`, or `blend`; changing base alpha alone keeps
that mode. `alpha_cutoff` is finite and nonnegative (default 0.5); values above
one hide masked surfaces. `double_sided` enables both faces with reversed
back-face shading normals. Blended surfaces draw after opaque geometry from
far to near, test scene depth without writing it, and do not cast binary
shadow-map shadows. Nonzero-alpha blended fragments remain pickable through
independent picking depth; zero-alpha fragments are discarded. Presets start
with opaque, single-sided coverage. These controls share inspector editing,
undo/redo and scene persistence.

`occlusion_strength` is within 0..=1 and blends the occlusion texture's influence on
ambient lighting. Zero ignores that texture. Every number must be finite. Receipts
name base-color and emissive color spaces separately. These factors share inspector
preview, undo and scene persistence. Batch edits accept 1–256
distinct mesh IDs and preflight every target before changing any object.
One batch is one agent undo step. Inspector sliders preview continuously and
group a pointer gesture into one editor undo step. Undo and redo restore the
exact linear color, including an originally absent tint.

Base color multiplies the existing texture. Alpha edits the tint factor; it
does not switch the object's pipeline to transparent rendering. Texture replacement is outside this per-object factor editor.

## Inspect and edit texture sampling

`material inspect` reports the five named roles (`albedo`, `normal`,
`metallic_roughness`, `occlusion`, `emission`), their UV transforms and sampler
settings, and `uv_sets` availability for the mesh. `provenance.imported_textures`
identifies each imported image by portable asset reference and image index,
dimensions, mip count, decoded format, source color space and fallback status.
Sampling returns linear values to the shader; color images decode sRGB, while
normal/MR/occlusion images are linear data. The tangent-basis receipt distinguishes
provided tangents, original MikkTSpace coordinates and reconstruction after a
normal-coordinate edit. Procedural materials report no imported images.

```json
{"action":"set_sampling", "entity_ids":["4294967302"], "role":"albedo",
 "patch":{"scale":[2.0,2.0], "offset":[0.25,0.0], "wrap_u":"repeat",
          "minification":"linear_mipmap_linear", "magnification":"linear"}}
```

Patches preserve omitted properties, other roles, image bindings and PBR factors.
Scale applies before rotation, then offset; rotation is in **radians**. UV0 and
UV1 are selectable only when available on every target. Negative scale mirrors
an axis; zero scale is legal. Minification accepts `nearest`, `linear`,
`nearest_mipmap_nearest`, `linear_mipmap_nearest`, `nearest_mipmap_linear` and
`linear_mipmap_linear`. Without a mip suffix it uses level zero. Magnification
accepts `nearest` or `linear`; wrapping accepts `repeat`, `clamp_to_edge` or
`mirrored_repeat`. Anisotropy is 1–16, requires linear min/mag filters above one,
and is clamped to the native device maximum. Every numeric value must be finite.

The batch preflights all targets and sampling policies before changing any
object. One successful call is one agent undo step; failure changes none of the
targets. Scene saving persists sampling separately from surface factors.
Omitting scene sampling retains the imported glTF settings; unavailable
coordinates required by a referenced image reject scene staging atomically.
Image replacement and standalone reusable material assets remain unavailable.

## Build rooms with usable dimensions

Katla uses meters, Y up, box centers for positions, and degrees for spawn-tool
Euler rotations. A unit cube scaled `[6,0.15,8]` is a 6 × 0.15 × 8 meter slab.
Name parts by room and function, e.g. `Study / Floor`, so name search is useful.

The room helper adds a floor and walls around a usable interior with a centered
door opening in the front (+Z) wall. It leaves the current scene in place:

```bash
python3 scripts/author_room.py --dry-run --name Study --size 6 3 8
python3 scripts/author_room.py --socket /tmp/katla-editor.sock \
  --name Study --size 6 3 8 --origin 0 0 -4 --output /tmp/study-receipt.json
```

`--size` is width, height, depth. `--doorway` is width, height; `--ceiling` adds a
ceiling. The receipt lists every part's ID, bounds recipe, preset and undo count.
On an operation failure, the helper undoes its successful edits in reverse order.
Use it while no other author is concurrently editing the scene, because rollback
uses the editor's shared agent history. No physics colliders are generated.

Place props using `spawn_model` or named primitives. Query nearby bounds before
placing furniture and leave space for door approaches and circulation. Observe
from inside the room and from above. `editor_view focus` can fit a particular
object; `set_camera` takes world-space position and target.

## Connect prefab behavior

Start with `prefab describe` to obtain complete mesh and prefab JSON examples.
Write referenced `.katmesh` recipes first, then `.katprefab` composition; validate
before writing and instantiate for a native preview. Instantiation returns named
`nodes` with lossless IDs, parents and scene keys. Pick the specific child by its
role rather than assuming a template ID survives instantiation. Mesh parts with
one material/lifecycle combine into one geometry stream; independent materials
or behaviors belong on separate child entities. See [prefabs](prefabs.md).

`behavior describe` returns the actual complete particle descriptor and a sample
script using `on_spawn` for one-time subscriptions. These operations are shared by MCP and the co-creator:

```json
{"action":"set_script","entity_id":"4294967302","path":"scripts/prefab-effect.luau"}
```

`set_script` requires an existing resource-relative `.luau` file below the scripts
root. It compiles before replacing the attachment. `set_particles` requires a
`document` matching the scene particle descriptor, validated before mutation;
use the example returned by `describe`. It replaces authored configuration while
preserving a live native emitter handle. Both edits have agent undo/redo. Explicit
`path: null` or `document: null` detaches; omitting the field is an error.
`inspect` reports the current script, full particle descriptor and world position.
Particle colors use linear RGBA; material tool colors use sRGB.

Create a sensor using `trigger create_box` with empty rules, parent it under the
prefab root, attach particles/script, then `set_rules`. Ordered trigger actions
support animations, `set_particles_active`, `burst_particles` and named Luau
`emit` events. `behavior burst` previews 1–100,000 particles on an active emitter;
`set_active` is undoable during authoring and transient during simulation.
The shipped `scripts/prefab-effect.luau` listens to `prefab_activated`, filters by
its own trigger identity, activates its emitter and queues a burst. Include the
sensor and referenced visitor in the same captured subtree; external references
reject capture instead of binding to an unrelated object.

Use `simulation play`, inspect trigger diagnostics and `behavior inspect`, then
`editor_view observe` for native output. Pause and resume are explicit; `play`
while already paused leaves it paused. Stop reconstructs the authored snapshot,
replaces runtime IDs and clears history. Query fresh IDs afterward. Script/particle
attachment authoring and prefab instantiate/capture/remove require edit mode;
bursts/toggles can preview at runtime. Capture after Stop persists authored
behavior, not transient gameplay state. Particle simulation continues visually
while paused; the pause gate applies to gameplay scripts and physics.

The [native prefab acceptance script](../scripts/validate_prefabs.py) drives this
complete workflow in a disposable editor and writes PNGs plus a receipt.

## Verify and persist

Use `material inspect`, scene queries, then `editor_view observe` to check actual
appearance. `simulation inspect` also reports whole-scene completed GPU particle
counters with source submission, so they can lag the current frame.
`editor_view undo` reverses the latest agent edit. Scene saves preserve
base color, metallic, roughness and occlusion through the existing v3 format.
`save_scene` writes to the explicit destination you supply; loading a scene
replaces the current document and clears its history.

Material MCP results include structured content; application failures set
`isError: true` with `success: false` and a message. Successful set receipts report
each affected ID, its `before` factors and resulting `values`. Invalid argument
errors are handled by the MCP transport before an application operation runs.

`scripts/validate_authoring.py` exercises asset discovery, room geometry, batch
material preflight, committed pixel changes, undo and scene save/reload in a
disposable editor (requires ImageMagick). Its output describes native engine behavior; it is not a
semantic evaluation of an external model.

The [material agent study](material-agent-study/README.md) records an independent
LLM's actual MCP discovery, factor edits, native image checks, invalid-batch
recovery, undo and persistence. It includes the full transcript and observed
limits; it does not establish preferences for all models.
