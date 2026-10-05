# Scene format v3

A `.katla` document is human-readable RON containing authored engine state. It
contains persistent scene keys and reproducible resource descriptions. Runtime
ECS IDs, GPU handles, physics handles and live GPU particles are recreated.
Scene composition and game component codecs belong to `katla_app`.

## Minimal document

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

RON fixed-size arrays use tuples; entity lists and variable-size arrays use
brackets. Missing transforms default to zero position, identity XYZW quaternion
and unit scale; individual transform fields can also be omitted. An omitted
source is `Empty`. Optional components are omitted on save. Built-in fields and
source variants are strict: typos and unsupported fields produce parse errors.
Extension headers make concise option values explicit; `Some(...)` also works.

## Identity and references

`id` is a positive `u64`, unique within one document. It survives renaming,
reordering, reload and Play/Stop. Names are labels: duplicate and absent names
are preserved. Parent and joint references use keys, independent of entity
ordering. Runtime references are mapped after all replacement entities exist.

`next_entity_id` must be greater than every key. The editor allocates new keys
monotonically and retains the counter after deletion. Keys are document-local;
references between separate documents need a separate application asset identity.
Do not write runtime `EntityId::id()` values into files or custom payloads.

Visible scene entities require `TransformComponent`; missing transforms cause
capture to fail instead of silently dropping an entity. Source-less transform
entities serialize as `Empty`. Mark runtime-only entities `EditorHidden`.
The editor camera and other hidden editor entities survive scene replacement.

## Resource roots

Models, mesh recipes, scripts and audio use the same explicit reference type:

| RON reference | Resolution |
| --- | --- |
| `Resource("models/Fox.glb")` | `ResourceManager.root/models/Fox.glb` |
| `Scene("models/ship.glb")` | Relative to the opened scene file's directory |
| `File("/absolute/path/ship.glb")` | An intentional nonportable absolute file |

Resource and scene paths use forward slashes, normal relative components and
no `.`/`..`, empty segments, drive prefixes or NULs. `Scene` requires a file
origin. File loading captures absolute roots once, so subsequent resolution
does not depend on the process working directory. `load_scene` loads an
in-memory scene and supports `Resource` and `File` references.

`MeshAsset(path: Resource("meshes/chair-frame.katmesh"))` reconstructs static
recipe geometry. Prefab instances save as expanded scene subtrees with these mesh
references. See [mesh/prefab authoring](../../../docs/prefabs.md).

The loader checks each built-in referenced file before allocating replacements.
Mesh recipes compile during preflight unless an identical recipe is already active.
Model decoding and GPU preparation happen during staging and can still fail.
Scripts retain the script engine's configured directory restrictions and are
compiled when the script system starts them. Audio decoding occurs in the audio
system. File existence does not establish valid script/audio content.

Save As rebases built-in references against the destination. References outside
both roots become `File`; Save As does not copy assets. Runtime model origins are
rebased too, keeping subsequent saves consistent. A portable scene package should
keep its files below its scene directory or the application's resource root.

## Component state

Built-ins cover drawable PBR overrides; point and directional lights; particle
emitters; animation including both sides of a fade; velocity; scripts; perspective;
audio and reverb; rigid body settings and linear velocity; collider shape;
physics material; triggers and their ordered action rules; collision filters; and joints.

Components are independent of source. A cube can also emit particles, light or
sound. `Light` and `ParticleEmitter` choose editor icons; their optional component
fields explicitly control engine behavior. Editor billboard materials are not
scene material overrides. Particle position comes from the entity transform;
configuration includes end color/scale, active state, destruction policy, timed
emission and pending bursts. GPU emitter handles and living particles are excluded.

Rigid bodies use one descriptor with `kind`, gravity, CCD and velocity. Mesh
colliders use `Trimesh` or `ConvexHull` with no obsolete handle fields; the loader
binds the reconstructed model mesh and its retained CPU geometry. Joint endpoints
must have dynamic or kinematic bodies and colliders. Native physics handles are
created by the physics system. World poses resolve from current locals and Parent links. Rendering and bounds
use exact matrix composition; physics consumes a rigid world pose and baked mesh
deformation.

Trigger visitor filters and explicit animation targets use persistent keys.
Rule references resolve after all entities are staged, so forward references,
renames and duplicate names work. A stale live target rejects capture before
writing a file or starting Play. Once-only state, overlaps and diagnostics reset
on load. `Trigger` is a sensor-only source without GPU allocations. See
[scene events](../../../docs/scene-events.md) for rule and action semantics.

### Game component codecs

Install serializers before startup scene loading, using
`ApplicationBuilder::with_scene_components` or `Application.scene_components`:

```rust
use katla_app::scene::SceneComponentRegistry;

let mut components = SceneComponentRegistry::default();
components.register::<Health>("game.health", 1)?;
let builder = ApplicationBuilder::new().with_scene_components(components);
```

`Health` must implement `Component`, `Serialize` and `DeserializeOwned`. A codec
has a namespaced key, positive version and owned RON payload string. One type and
one key can be registered only once. `katla.*` is reserved. Known versions and DTOs
are checked before staging; decoder failures roll back the replacement scene.

For entity references or native runtime data, register a wire DTO instead:

```rust
components.register_codec::<Target, TargetData>("game.target", 1,
    |component, context| Ok(TargetData { entity: context.id(component.entity)? }),
    |data, context| Ok(Target { entity: context.entity(data.entity)? }),
)?;
```

`TargetData.entity` is a `SceneEntityId`. `SceneWriteContext::id` rejects references
outside the captured scene; `SceneReadContext::entity` rejects dangling keys.
Never use the reference-free `register` helper for runtime IDs or GPU/native
handles. Application codecs own semantic validation and component-version
migrations; version mismatches return errors until converted by the application.

Unknown keys remain opaque and round-trip unchanged, with a warning on load.
They do not instantiate an ECS component until its codec is installed before
loading. Removing a known ECS component removes its saved payload. Unregistered
live game components require a codec to be persisted. Asset paths inside custom
payloads belong to the application codec; opaque payloads cannot be rebased by
Save As. Prefer `Resource` or `File` for such references.

## Validation, migration and errors

`SceneManager::parse` reads the header before the version-specific schema. Missing
versions are v0. v0/v1/v2 readers explicitly convert name-based parents and
trigger targets, string asset paths, split rigid-body properties and obsolete
collider handles into v3. Missing
legacy light/particle fields retain their former implicit defaults. Ambiguous or
missing legacy parents or trigger targets fail migration. Version 2 added
sensor-only trigger sources and name-based rules. Saving writes v3. Unsupported future
versions fail before unknown future entity variants are parsed.

Legacy `resources/...` paths become `Resource`, absolute paths become `File`, and
other relative paths become `Scene`. Bare script names become
`Resource("scripts/name.luau")`. Legacy files are read without modifying them;
Save performs the conversion on disk. Old-format fixtures remain under `fixtures/`.

`Scene::validate` reports multiple `SceneIssue` values with entity keys and field
paths. It checks keys/counters, parents/cycles, joint targets, finite numbers,
normalized rotations, nonsingular scales, geometry dimensions and budgets,
component ranges and asset syntax. Camera FOV is in degrees, matching the engine
component, and must lie strictly between zero and 180. Hierarchy validation is iterative and visits
each ancestry edge once. Limits are 64 MiB per file, 100,000 entities, one million
points per generated mesh/heightfield, 1 MiB per game payload and 1,024 queued
bursts. These are document preparation limits, not GPU capability guarantees.

`SceneError` distinguishes I/O, parsing with source positions, version, migration,
capture, limits, validation, codec and entity preparation failures. No parse,
validation, staging or codec failure replaces the currently loaded document.

## Save, load and Play

Capture is fallible and assigns persistent keys to new entities. `to_ron` validates,
sorts entities by key and uses ordered custom component maps for stable diffs.
Capture does not change timestamps; actual saves preserve name/author/creation
metadata and set the modified timestamp and engine version. Timestamps are Unix
seconds represented as strings. RON comments are accepted but regenerated on save.

Saving writes and syncs a sibling temporary file and atomically replaces the target
before recording the path or baseline. Failed writes retain the previous document.
Loading preflights data, stages entities and resources, binds references and decodes
components, then captures a valid baseline before retiring the old scene. Any
failure rolls back staged entities and tracked resources. Failed shader or skeleton
preparation also releases partial model uploads. Successful replacement retires
unreferenced meshes and CPU geometry while retaining protected/shared resources.

Play captures the scene, its origin, ID allocation counter and saved baseline.
Stop restores through the same loader and reinstates the document, preserving
pre-play unsaved edits. Failed capture prevents entering Play; failed restore keeps
the snapshot for retry. Animation progress alone does not mark the document dirty.
Play snapshots include queued/timed emission, but do not capture live GPU particles,
script VM state or running audio voices. They describe reconstructible component
state rather than an exact checkpoint of every subsystem.

## Implementation and checks

| Module | Responsibility |
| --- | --- |
| `descriptors.rs`, `entity_source.rs` | Strict current schema |
| `identity.rs`, `document.rs` | Persistent keys and editor baseline |
| `assets.rs` | Explicit roots and portable references |
| `validation.rs`, `error.rs` | Diagnostics without engine mutation |
| `migration.rs` | Isolated older readers and conversion |
| `component_registry.rs` | Versioned game codecs and reference contexts |
| `capture.rs`, `spawn.rs` | ECS capture and built-in reconstruction |
| `serialization.rs` | Atomic I/O and replacement transaction |

```bash
cargo test -p katla_app --lib
cargo test -p katla_app --lib document_tests -- --ignored --nocapture --test-threads=1
cargo test -p katla_app --lib test_regenerate_default_scene -- --ignored
```

The deliberate regeneration utility writes the shipped v3 example scenes. Native
regressions check component values, entity remapping, rollback, shader failure,
asset origin, Save As and editor document behavior. Rendering changes also require
native GPU validation; builds and screenshots alone are insufficient evidence.
