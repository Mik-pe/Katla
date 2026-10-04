# Mesh recipes and prefabs

Reusable geometry and scene composition share the canonical Odin asset services.

| Asset | Purpose | Result |
| --- | --- | --- |
| `.katmesh`, version 1 | Named geometry parts with baked local transforms | Validated indexed CPU geometry, bounds and collider input |
| `.katprefab`, version 1 | One rooted scene v3 subtree | Fresh editable entities and mapped component references |
| `.katla`, version 3 | Complete authored scene | Expanded instances, persistent keys and explicit asset origins |

`odin/app` owns recipes, documents, staging and history. `odin/app/render` prepares
native GPU resources from `Scene_Mesh`/`Scene_Model`; the GPU core receives ordinary
resources. GLTF/GLB models retain skins, materials and animation through the model
pipeline; ASCII/binary STL produces canonical static geometry.

## Build an object

Use parts for geometry sharing one material and lifecycle. The
[chair frame](../resources/meshes/chair-frame.katmesh) combines a seat, back and
four legs; its cushion is a separate entity so its material can vary independently.
[chair.katprefab](../resources/prefabs/chair.katprefab) contains an identity root
and these children. The [workshop](../assets/scenes/prefab-workshop.katla) shows
expanded instances with different placements and cushion colors.

Asset-browser double-click or drag inserts real mesh/model/prefab entities and
selects their roots. A mixed drop prepares the entire batch before publishing one
native revision and one history group. External file drops use exact retained
File capabilities. Transform inspection edits local placement; rendering, culling
and queries resolve current hierarchy matrices. Focus includes drawable descendants
of an empty root. Combining recipe parts does not weld, union, simplify or add LODs.

## Mesh contract

A recipe contains `version`, `name` and `parts`. Parts have unique `id`, optional
`transform` and strict `geometry`. Kinds are `cube`, `sphere`, `plane`, `cylinder`,
`cone`, `torus` and `triangles`. The `prefab` tool's `describe` action returns the
actual transport shape. Disk RON uses tuples for fixed arrays:

```ron
(
    version: 1,
    name: "Seat",
    parts: [(
        id: "seat",
        transform: (position: (0.0, 0.45, 0.0)),
        geometry: (kind: "cube", size: (0.8, 0.1, 0.8)),
    )],
)
```

Indexed triangles contain XYZ `positions`, CCW `indices`, optional XYZ `normals`
and UV `uvs`. Attribute counts match positions and indices form valid triangles.
Missing normals are area weighted. Missing UVs use zero UVs and orthogonal tangents.
Zero-area triangles and undefined generated normals fail; small valid triangles
remain valid. UV seams/hard edges need duplicated vertices. Part transforms bake
positions, inverse-transpose normals and orthogonalized tangents; reflections
reverse winding and tangent handedness.

Reads are limited to 64 MiB, recipes to 1,024 parts, combined output to one million
vertices and six million indices. Values are finite, scales nonzero, dimensions
positive and tessellation bounded before allocation. Geometry/transform magnitudes
are bounded by one million. `write` compiles the complete recipe before atomic
publication. Native GPU caches belong to the render consumer; source descriptors
and prepared CPU geometry belong to application components. Do not serialize GPU
handles or depend on an implicit sharing/batching policy.

## Prefab, persistence and history

A prefab contains `version: 1`, a `root` scene key and a `scene` using the
[current scene schema](scene_format.md). Exactly one root has no parent and its
transform is identity; every other entity descends from it. Instantiate supplies
root placement, allocates unused scene keys and fresh ECS identities, then maps
parents, joints, trigger targets and registered custom references. Names and
ordering have no identity role.

`Resource`, `Scene` and `File` references resolve from the template's origin.
Capture and Save As rebase built-in references against their destination. Project
paths and explicitly selected absolute `.katprefab` files use retained filesystem
capabilities; decoders cannot reopen arbitrary paths. Unknown/unsupported prefab
component codecs fail because their references cannot be safely mapped. Ordinary
scene files preserve their opaque extension payloads.

Capture discards the root's external parent and placement while preserving child
locals and component settings. References outside the subtree fail before writing.
Templates expand into editable entities; saving the containing scene preserves
instance edits. There is no implicit inheritance or live override synchronization.
A revised recipe affects subsequent loading/instantiation; existing prepared
geometry remains accepted until an explicit replacement.

Instantiate and Remove use the shared application history and native restoration
path. Remove includes incoming joint/trigger cleanup in the same transaction.
Undo restores fresh identities and maps surviving references. Native rejection
preserves entities, references, queues and history stacks. Accepted history owns
component settings and prepared geometry; consumed particle bursts are not replayed
by Undo/Redo. File-load descriptors may explicitly schedule an initial burst, which
is consumed once by the render owner. Capture writes an asset and does not add a
scene mutation history entry.

Mesh colliders bake full affine deformation and use rigid world poses in Box3D.
Static/kinematic poses follow hierarchy movement; dynamic results convert back to
local pose while preserving authored scale. Collider shape/deformation replacement
uses native preparation. Convex hulls and moving concave meshes use the actual
native contracts described in [Box3D](../tools/box3d/README.md).

## Agent iteration

MCP and the in-editor agent use the same services. Entity IDs are complete decimal
strings. Mesh-authoring tool paths are project-relative `.katmesh`; templates also
accept intentionally selected absolute `.katprefab` paths. Traversal/link escapes
fail rather than acquiring authority.

1. `{"action":"describe"}` returns supported operations. `read` loads an actual
   existing mesh/template document.
2. Edit the complete document and call `validate` with `path` and `document`.
   Mesh feedback includes vertices, triangles and bounds.
3. `write` compiles/prepares before atomic publication. Write referenced mesh
   files before their prefab. Failed validation keeps the prior file.
4. `{"action":"instantiate","path":"resources/prefabs/chair.katprefab","position":[2,0,0]}`
   inserts a preview. Rotation defaults to identity XYZW and scale to `[1,1,1]`.
   Retain returned root/entity IDs; use current queries after restoration.
5. Inspect committed pixels/metadata through `editor_view`. Insert the next valid
   revision, then `{"action":"remove","root_entity":"..."}` removes the old
   subtree reversibly. Failure leaves the accepted preview available.
6. Attach scripts/particles and [trigger rules](scene-events.md), then use
   `simulation` Play/Pause/Resume/Stop. Authoring mutations require edit mode.
7. `{"action":"capture","path":"resources/prefabs/custom.katprefab","root_entity":"..."}`
   exports the edited subtree. Save the scene to persist its placement.

Canonical application entrypoints are `asset_authoring_execute`,
`scene_action_execute_batch`, `asset_capture_prefab` and `asset_remove_prefab`.
They share `Scene_Snapshot`/`Scene_Stage`, typed component ownership and participant
preparation; there is no separate legacy prefab engine.

## Verification

`python3 scripts/build_katla_odin.py --tests --sanitize` builds real pinned
native dependencies and runs CPU ownership/admission/history tests. Tests cover
geometry budgets/transforms, actual recipes, custom/reference remapping, absolute
model/template batches, capability rollback, consumed bursts, external joint/rule
cleanup and corrupt-source Undo/Redo.

Native Metal/Vulkan acceptance in
[`validate_odin_render.py`](../scripts/validate_odin_render.py) uses actual mesh/
model entities and retained readback. Its asset fixture exercises Save/Load,
Capture/Instantiate/Remove, native preparation rejection and restored pixels/cache
owners. Enable `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` before Metal launches
and supply explicit Vulkan loader/ICD paths for the selected host. Compilation,
CPU fixtures and screenshots alone do not establish native GPU behavior.
