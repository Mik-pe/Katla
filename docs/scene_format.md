# Scene format and document ownership

A `.katla` document stores authored state as human-readable RON. The current
format is version 3. JSON carries the same descriptors through tools. Persistent
scene keys and reproducible asset descriptions survive saving; ECS generations,
GPU identities, physics handles, script VMs and audio voices are recreated.
Composition belongs to `odin/app`.

## Current document

```ron
#![enable(implicit_some)]
(
    version: 3,
    name: "Level",
    next_entity_id: 3,
    entities: [
        (id: 1, name: "Root"),
        (
            id: 2,
            name: "Crate",
            parent: 1,
            transform: (position: (0.0, 1.0, 0.0)),
            source: Cube(size: (1.0, 1.0, 1.0)),
            rigid_body: (kind: Dynamic),
            collider_shape: Box((0.5, 0.5, 0.5)),
            components: {
                "game.health": (version: 1, data: "(current:42)"),
            },
        ),
    ],
)
```

Fixed arrays accept RON tuples; variable arrays use brackets. Missing transforms
use zero position, identity XYZW rotation and unit scale; individual transform
fields can be omitted. An omitted source is `Empty`. Built-in field names and
variants are strict. Optional component absence is preserved, including the
distinction between a rigid body, collider, material and collision filter.

Sources cover primitive geometry, `MeshAsset`, `StlModel`, `GltfModel`, `GltfGroup`, `GltfPrimitive`, `Empty`,
`Light`, `ParticleEmitter` and `Trigger`. Sources and optional components are
independent: a mesh may also emit light, particles or audio. Point/directional
lights, perspective, drawable factors, animation/fades, particles, scripts,
velocity, audio/reverb, physics shapes/materials/filters, joints and trigger rules
have explicit descriptors. Additional registered components, including authored
Billboard settings, use `components`.

## Identity and reference mapping

An entity `id` is a positive, unique `u64` document key; `next_entity_id` is
strictly greater than every entity key. Both are parsed without floating-point
rounding or unsigned overflow. Keys are independent of names, entity order and
runtime slots. Duplicate or absent names do not redirect references. New authored
entities receive monotonically allocated keys; deletion does not reuse them.

Parents, joints, trigger visitors and explicit action targets use document keys.
Staging allocates every entity before remapping references. Capturing a stale
reference or a reference outside the captured scene/subtree fails. Runtime entity
IDs in tool arguments/results are complete decimal strings; query fresh IDs after
load, Stop or create/delete restoration.

Every visible captured entity needs `Scene_Transform`. Unregistered live component
types fail capture instead of disappearing. `Editor_Hidden` entities are omitted
and survive document replacement. Exact column-major hierarchy matrices preserve
nonuniform scale and shear for rendering/bounds. Native physics resolves rigid
poses and bakes affine deformation into collider geometry.

## Asset origins and confined access

| Reference | Resolution |
| --- | --- |
| `Resource("models/Fox.glb")` | Below the installed resource root |
| `Scene("objects/chair.katmesh")` | Below the opened document's directory |
| `File("/absolute/path/ship.glb")` | An explicitly selected external file |

Relative paths use UTF-8 and forward slashes. Traversal, empty/dot segments,
drive prefixes, alternate streams, backslashes and NUL are rejected. Installed
roots retain directory handles. An intentional File source retains its selected
parent and a confined basename; decoders receive bounded bytes and cannot reopen
unrestricted filenames. Reads/traversal refuse symlinks and Windows reparse points.
See [retained filesystem capabilities](../odin/resources/README.md).

Scene and prefab origins apply to models, mesh recipes, scripts and audio.
Save As rebases built-in references: assets below its destination become `Scene`,
assets below the installed resource root become `Resource`, and other assets
remain `File`. It does not copy assets. Custom opaque payload paths belong to
their application codec and cannot be rebased automatically.

External script File references acquire exact capabilities during explicit scene,
prefab or picker admission. Failed staging revokes newly added capabilities.
Resource script names normalize to `scripts/name.luau`; explicit Project/File
origins retain their identity with the `.luau` extension. Script source admission
checks bounded UTF-8; Play and code saving use the actual Luau compiler. Audio,
mesh and model decoding/validation happen before scene publication. Native GPU
participants prepare actual uploads before accepting the replacement.

## Registered component codecs

Install application types before startup loading using the same registry as the
inspector, agent and history:

```odin
Health :: struct { current: i32 }
editor.editor_register(&owner.world, &owner.registry,
    "game.health", Health{100}, spawn_default=false)
```

Registration supplies typed fields, owned encoding/decoding, defaults and component
policy. Components owning strings, arrays or runtime data supply `ecs.Value_Ops`
clone/destroy hooks and appropriate owned codecs. Ordinary `ecs.Entity_Id` fields
and `u64` fields tagged `inspect:"entity_ref"` use typed reference mapping.
Reference-bearing maps/unions need an explicit `reference_map`; a strict map
rejects references outside the captured set. Register each type/name once.

Extension envelopes contain positive `version` and an owned RON `data` string.
Current registered codecs use version 1. Scene files preserve unknown names and
unsupported versions as opaque payloads without instantiating components. Those
payloads round-trip unchanged, including ordinary objects whose keys resemble
internal numeric markers. Prefabs require supported registered codecs so all
references can be mapped safely. Removing a known component removes its saved
payload. Application version conversion must be explicit before admission.

## Migration, limits and failure

The reader examines the version header before current descriptors. Missing
versions mean v0. v0/v1/v2 migration converts name-based parents/trigger targets,
string asset paths, split body settings and obsolete collider descriptions.
Former implicit light/particle defaults are retained. Ambiguous or missing legacy
references fail; unsupported future versions fail before future entity variants
are interpreted. Reading does not rewrite the source; saving writes v3.

Legacy `resources/...` paths become `Resource`; other relative paths become
`Scene`; absolute paths become `File`. Bare script names use the resource scripts
directory. Actual older fixtures live in
[`odin/app/scene_migration_fixtures`](../odin/app/scene_migration_fixtures).

Preparation checks unique keys/counters, complete parents and cycles, reference
dependencies, finite values, normalized rotations, nonsingular scales, shape
budgets and component ranges. FOV is in degrees, strictly between zero and 180;
camera near distance and aspect are positive. Reads/snapshot wire data are bounded
by 64 MiB and scenes by 100,000 entities. Generated meshes have their own vertex,
index and tessellation limits; heightfields contain at most one million heights.
These CPU budgets do not imply that a GPU upload will succeed.

Typed `editor.Scene_Error` results distinguish invalid fields/operations, missing
components/entities, decoding, protected entities, edit-mode requirements and
native admission failures. Parsing, migration, component decode, reference mapping,
baseline preparation or native upload failure preserves the accepted world,
document path/baseline and history. Partially prepared owners are released.

## Saving, loading, history and preview

Capture preserves name/author/creation metadata. Actual saves set the modified
timestamp and engine version; timestamps are Unix seconds stored as strings.
The writer regenerates formatting/comments. A save syncs
a sibling temporary file and atomically publishes it before recording its path or
saved baseline. The publication flag distinguishes a failed prepublication write
from an error after publication.

Load prepares DTOs, source capabilities, entities, reference maps, native resources
and the document baseline before retiring the old scene. UI Open and public
`load_scene` share this path. `save_scene` accepts an explicit destination or the
currently bound path. New/Open/Quit use Save/Discard/Cancel when authored state is
dirty. Save As handles overwrite confirmation. Document actions require editing
mode; stop preview before entering document actions.

Inspector edits, agent mutations, asset drops and hierarchy actions share one
history. Existing-entity edits restore only changed components; unrelated live
state remains current. Create/delete restoration uses owned component clones and
fresh generations. Undo/Redo goes through native preparation and keeps both stacks
unchanged when admission fails. Geometry/source snapshots do not re-read changed
asset files merely to redo an accepted command.

Play captures owned authored state and preflights native physics/scripts. Stop
restores through shared staging with fresh entity IDs and preserves pre-play dirty
state. Failed capture prevents Play; failed restore leaves the snapshot available
for retry. Animation clocks alone do not dirty the document. Snapshot data
reconstructs authored descriptors; it does not capture live particles, VM state or
running voices. Runtime overlap/once/error state resets for the reconstructed
play session. Pause/Resume changes execution without replacing authored entities.

## Independent material assets

Drawable descriptors retain authored `surface`, five-role `sampling`, and
`textures` separately. `GltfGroup` controllers retain a source path;
`GltfPrimitive` children retain that path plus `node_index` and
`primitive_index`, so each primitive remains independently editable. A legacy
`GltfModel` descriptor expands to that hierarchy through atomic scene staging.

Reusable `.katmat` files use RON version 1 with a name, complete material values,
sampling, and explicit neutral/file/glTF-image sources. `material_asset` supports
Describe/Read/Validate/Write/Capture/Apply through the same application owner.
Capture resolves inherited images to reproducible sources; Apply creates owned
independent copies. Editing or rewriting an asset file does not mutate existing
copies. New image assignment reads the selected source revision, while Undo/Redo
retains the decoded revision that was accepted with the original command.

Image decoding preserves eight-bit, sixteen-bit and floating-point samples.
Integer color roles decode sRGB; linear data roles retain their precision.
Floating HDR stays linear and must fit finite half-float range for native upload.
Portable documents contain source descriptors rather than pixels or GPU owners.

## Canonical implementation and verification

| Source | Responsibility |
| --- | --- |
| `odin/app/scene_document*.odin` | Current descriptors, registered extensions and export |
| `odin/app/scene_migration.odin` | Explicit older readers/conversion |
| `odin/app/asset_path.odin`, `odin/resources` | Origins and retained filesystem capabilities |
| `odin/app/scene_snapshot.odin`, `scene_staging.odin` | Owned capture, remapping and rollback |
| `odin/app/scene_file.odin`, `document` | Atomic publication and document baseline/dialogs |
| `odin/app/scene_action*.odin`, `odin/editor` | Shared mutation/restoration history |
| `odin/app/simulation.odin` | Preview capture and restore |

`odin run tools/build -- --tests --sanitize` builds pinned native
dependencies and runs the configured CPU ownership suites. Actual regressions
cover older files, unknown payloads, full-width keys, external origins, script/audio/
model admission, Save As metadata, failed native preparation, captured subtrees and
corrupt-source Undo/Redo. Rendering acceptance additionally uses
[`tools/build validate render`](../tools/build/validation.odin) with explicit native
adapters. Windows retained-root/junction and installed absolute-script tests run on
the native Windows CI host; cross typechecks do not prove their runtime behavior.
