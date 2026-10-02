# Scene and material authoring for agents

Connect to the running editor using [shared editor MCP](shared-editor-view.md).
Keep the editor in edit mode. `editor_view` returns a committed viewport PNG;
use it before editing and again to verify the result. Native Vulkan/Metal output
is the visual authority. Geometry queries alone cannot establish occlusion.

For an authored example, launch `cargo run -- --scene assets/scenes/material-studio.katla`.
Select a material sphere to explore presets and live surface controls alongside
a small furnished lounge.

## Find things first

Use `query_entities` with `name_filter`, `component_filter` or a world-space
`position` and `radius`. Returned generational IDs are **decimal strings**: retain
them verbatim. `get_scene_hierarchy` gives parent relationships;
`list_available_components` and `get_component_attributes` expose editable fields.

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
paths and parent traversal are rejected. An empty search query lists assets.
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
Explicit fields override the preset. Omitting the preset patches only supplied
fields. `base_color` is sRGB RGBA; `metallic`, `roughness` and `ao` are linear
factors. Every number must be finite and in 0..=1. Batch edits accept 1–256
distinct mesh IDs and preflight every target before changing any object.
One batch is one agent undo step. Inspector sliders preview continuously and
group a pointer gesture into one editor undo step. Undo and redo restore the
exact linear color, including an originally absent tint.

Base color multiplies the existing texture. Alpha edits the tint factor; it
does not switch the object's pipeline to transparent rendering. Emission and
texture replacement are outside this per-object factor editor.

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

## Verify and persist

Use `material inspect`, scene queries, then `editor_view observe` to check actual
appearance. `editor_view undo` reverses the latest agent edit. Scene saves preserve
base color, metallic, roughness and occlusion through the existing v3 format.
`save_scene` writes to the explicit destination you supply; loading a scene
replaces the current document and clears its history.

`scripts/validate_authoring.py` exercises asset discovery, room geometry, batch
material preflight, committed pixel changes, undo and scene save/reload in a
disposable editor (requires ImageMagick). Its output describes native engine behavior; it is not a
semantic evaluation of an external model.
