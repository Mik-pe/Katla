# Agent scene authoring

The Odin editor exposes scene actions through one application owner. MCP clients,
editor panels and the Co-Creator use the same registered components, validated
asset services and undo history. The complete tool schemas live in
[`odin/agent/tools.json`](../odin/agent/tools.json); read `tools/list` from the
running owner before issuing requests. The [shared viewport guide](shared-editor-view.md)
explains launch, transport and image provenance.

## Start a disposable editor

Build and launch through the [canonical Odin scripts](odin_build.md). For a
shared-room session on macOS or Linux:

```sh
python3 scripts/build_katla_odin.py --output target/katla-authoring
katla_socket_dir=$(mktemp -d "${TMPDIR:-/tmp}/katla-editor.XXXXXX")
chmod 700 "$katla_socket_dir"
python3 scripts/run_katla_odin.py --build-dir target/katla-authoring --no-build -- \
  --scene assets/scenes/shared-room.katla --gpu-validation \
  --mcp-socket "$katla_socket_dir/editor.sock"
```

The launcher supplies built native dependencies and enables Metal validation on
macOS. The endpoint requires an owner-only parent directory and mode 0600;
existing endpoints are rejected. Run scripts from another terminal against that
same socket. Loading a scene replaces the current document and clears history,
so use a disposable editor for the validation journeys below.

## Discover before editing

`search_assets` searches the installed resource tree, including its prefab/mesh
assets. Its result includes resource-relative `assets`, project-relative
`project_paths` for those same discovered files, `total`, `truncated`, `root` and `path_contract`. Query matching
uses Unicode lowercase matching and all whitespace-separated words. Extension filters
accept a leading dot. An omitted/null limit defaults to 64; unsigned limits clamp
to 1–256.

`list_resources` and `read_resource` use the project root. For example, list
`resources/models` for the default project layout. An omitted/null list path means
the project root; `filter` is an optional extension. `create_resource` and
`write_resource` also write project-relative paths through retained root
handles. Creation is exclusive; replacement is atomic. A failed validation does
not overwrite an existing file. Templates and explicit text content use the same
service as the asset browser.

`spawn_model` accepts supported GLTF/GLB, STL and `.katmesh` sources. Relative model
paths use the installed resource root. An intentionally supplied absolute path
creates a confined File capability for that source and its validated dependencies;
it is not permission to read arbitrary sibling files. Prefabs use `prefab`
`instantiate`. See [scene assets and File capabilities](scene_format.md) and
[prefab authoring](prefabs.md).

## Construct and inspect

Use `spawn_entity` for named primitives, with optional world position, XYZ Euler
rotation in degrees and scale. Scene coordinates are meters, Y up. Supported
shapes are cube, sphere, plane, cylinder, torus and cone. A cube's geometry has unit
size before its transform scale. A successful scene reply is the canonical
`{entity_ids, data}` envelope. Entity IDs are decimal strings retaining the full
generational u64 value; zero can be a valid entity ID.

`query_entities` supplies names, components, parents, positions and drawable
bounds. `get_scene_hierarchy`, `list_available_components` and
`get_component_attributes` expose the current registered application data. Do not
infer component names or fields from the old engine. For a transform, read
`SceneTransform`, modify its `local` value, then submit that complete field:

```json
{"entity_id":"4294967302","component":"SceneTransform","field":"local","value":{"position":[0,1,0],"rotation":[0,0,0,1],"scale":[1.5,1,1]}}
```

This is a `set_field` argument object. Preserve the other values obtained from the
attribute query when changing one axis. `set_parent` validates the entire
relationship before publishing; null/omitted `parent_id` detaches. Duplicate,
destroy, component edits and native renderer preparation share atomic application
history. Undo/redo can recreate entities with fresh IDs and remap references;
query again after recreation or scene replacement.

## Edit surfaces without touching GPU handles

The `material` tool and Inspector → Material edit the same per-object PBR factors.
Inspector → Texture images and sampling uses the same validated role image and
sampling edits. Its Save material/Apply material buttons use `material_asset`
capture/apply; image drags, discrete edits and continuous gestures share editor
undo semantics. Agent history remains separately available through `agent_undo`.
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
effect, supported surface factors, named texture roles and the batch contract. Presets supply
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

Base color multiplies the effective image. Alpha mode explicitly selects coverage;
changing the alpha factor preserves that mode. `set_texture` replaces only the
selected image; `set` changes surface factors.

### Reusable surfaces

Use `material_asset describe` for a complete JSON example or `capture` with one
mesh `entity_id` and a project-relative `.katmat` path. Capture writes effective
factors, all five image choices, and independent UV/sampler policies. Imported
images become explicit glTF-image references; missing/fallback images become
neutral. Reusable definitions reject `inherit`, so applying to a different mesh
does not silently substitute its original maps. Omitted `textures` means five
neutral roles; a provided object must specify every role.

`read` returns JSON. Edit it, then `validate` and `write` with `path` and
`document`. Validation decodes images and checks formats/limits without GPU
uploads. Writes publish atomically and preserve existing live copies. Image
Resource roots use the resource directory; Scene roots use the **material file's
directory**; File references remain intentionally absolute. Capture prefers
portable Resource/Scene references where possible. Keep referenced images/glTF
files with the material when moving an asset bundle.

`apply` takes `path` and 1..256 distinct mesh `entity_ids`. It preflights every
target's required UV sets before replacing complete factors, sampling and image
choices as one undoable batch. Geometry, transforms and pipelines stay owned by
their original objects. Copies remain independently editable and share immutable
image generations; file writes do not change them. Reapply to read a revision,
inspect through `material`, observe through `editor_view`, and save the scene to
persist the copied surface. Search resource materials using extension `katmat`;
`project_paths` can be passed directly to `material_asset`.

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
Image assignment uses `set_texture`, independently of sampling and factors:

```json
{"action":"set_texture", "entity_ids":["4294967302"], "role":"albedo",
 "source":{"kind":"file", "asset":{"Resource":"textures/wood.png"}}}
```

An asset reference explicitly selects `Resource` (resource-relative), `Scene`
(relative to the opened scene file) or `File` (absolute). `kind: gltf_image` also
requires `image_index`, allowing embedded glTF images reported by inspection to
be reused. `kind: neutral` selects the role's neutral image; `kind: inherit`
restores the mesh source's original binding. All target/UV validation precedes
image preparation and mutation. Failed decoding/upload changes no target.
Successful assignment is one image-only undo step and preserves sampling,
factors, other roles, mesh and shared material state. Color roles decode integer
sRGB; data roles remain linear. Precision/HDR limits match glTF image uploads.
Identical authored uploads share immutable image generations; modifying a file
and assigning it again creates a new generation without changing earlier live
assignments. History retains the exact old generation until discarded. Scene
reload reads referenced files again; save does not embed images.

Inspection distinguishes original `imported_textures` (with `active` flags) from
`authored_textures`; each image source object can be reused in `set_texture`.
`original_generation_uv` describes the stored generated basis and
`current_normal_uv` the normal coordinates used for current shading. Scene Save As
rebases assigned image references along with model/script/audio assets. Writing
or replacing a file does not automatically reassign live objects. Reusable material assets use the complete capture/apply contract above.


## Room recipes

The [room builder](../scripts/author_room.py) emits a reviewable plan without
connecting:

```sh
python3 scripts/author_room.py --dry-run --name Study --size 6 3 8
```

Apply that plan to the selected editor:

```sh
python3 scripts/author_room.py --socket "$katla_socket_dir/editor.sock" \
  --name Study --size 6 3 8 --origin 20 0 -4
```

The floor, walls and optional ceiling are real cube entities. Walls sit outside
the usable interior and the front (+Z) wall leaves a centered doorway. Materials
are applied in validated batches. A failed operation unwinds only the successful
edits made by this invocation. Run the recipe without concurrent edits: rollback uses the shared chronological
history. The script leaves the document unsaved unless
`--save DESTINATION.katla` is supplied.

[`furnish_shared_room.py`](../scripts/furnish_shared_room.py) loads the prepared
room, adds the 15 unit-cube proxies from `teen-room-plan.json`, checks their bounds,
undoes them, and places them again. It preserves the base room, doors, window and
cabinet. Clearance checks describe a central passage and door approaches; they do
not prove navigation or door swing. These are explicit geometric proxies, not
claims that furniture models were discovered.

## Behavior, preview and persistence

`behavior describe` returns the actual particle descriptor and shipped script
path. `set_script` compiles the selected source before replacing an attachment;
bare resource names normalize below `scripts` and extensions normalize to
`.luau`. Explicit File scripts are admitted only through the selected scene,
prefab or source capability. `set_particles` validates its complete document.
Explicit null `path`/`document` detaches; omission is an error. Attachments and
emitter configuration share authoring history. `burst` previews 1–100,000 particles
on an active emitter; consumed bursts are transient and do not replay on undo/redo.

`trigger` creates sensors and edits ordered rules. Rules can play animations,
change emitter activity, burst particles or emit named Luau events. Use the
[scene event contract](scene-events.md) for targets, dependencies and delivery
order. Capture requires internal references to stay inside the captured subtree.

`simulation` supplies explicit inspect/play/pause/resume/stop transitions. Pause
retains preview state. Stop restores the authored snapshot with fresh runtime IDs
and resets history. Attachments, hierarchy and prefab mutations require Editing;
`editor_view observe` remains available during Play/Pause.

`save_scene` accepts an explicit destination, or omitted/null path for the current
bound document. An unbound document needs a destination. `load_scene` accepts a
project-relative or intentional absolute `.katla` path. Complete decode, asset
loading, reference validation and native preparation precede publication. A failed
load leaves the previous world, document baseline and native owners intact. See
[scene persistence](scene_format.md).

## Validation

Run the native journeys against a disposable socket owner:

```sh
python3 scripts/validate_shared_view.py "$katla_socket_dir/editor.sock"
python3 scripts/furnish_shared_room.py --socket "$katla_socket_dir/editor.sock"
python3 scripts/validate_authoring.py --socket "$katla_socket_dir/editor.sock"
python3 scripts/validate_prefabs.py --socket "$katla_socket_dir/editor.sock"
```

The authoring journey compares exact captured RGB pixels with a bounded stdlib
reader for the editor's native PNG output. For an isolated project/resource copy,
pass its project directory as `--project` to
the prefab validator. These scripts produce PNGs, exact frame/submission metadata
and receipts. The prefab journey verifies actual mesh writes, rejected replacement,
Capture/Instantiate/Remove, fresh-ID undo/redo, preview gating and saved hierarchy.
Particle/Luau delivery and allocation-failure rollback also have dedicated native
application tests; this script does not manufacture GPU counters.

Protocol fixtures run with
`python3 -m unittest discover -s scripts -p test_katla_mcp_client.py -v`.
They test transport envelopes and recovery independently of native rendering.
Neither deterministic recipes nor local host fixtures certify a live model's room
understanding, OS interaction or attachment to a user's existing conversation.
