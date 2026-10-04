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

`material` supports `presets`, `inspect` and `set`. A set accepts 1–256 unique
`entity_ids`, an optional preset and optional color/metallic/roughness/occlusion
patches. All targets and values are validated before any target changes. The
presets are plaster, oak, concrete, ceramic, brushed_metal and fabric. Material tool colors
are sRGB RGBA; particle colors are linear RGBA.

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
