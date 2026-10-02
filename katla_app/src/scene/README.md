# Scene Format

Runtime-mutable scene serialization for the Katla engine. Scenes are saved as human-readable `.katla` files using RON (Rusty Object Notation).

## File Structure

- `mod.rs` -- public scene exports
- `serialization.rs` -- scene save/load, hierarchy validation and resource retirement
- `document.rs` -- chosen path and last saved editor baseline
- `tests.rs` -- format and migration tests
- `entity_source.rs` -- `EntitySource` ECS component
- `descriptors.rs` -- RON-serializable data types (`Scene`, `EntityDescriptor`, etc.)

## Format Versioning

The `Scene.version` field tracks the format version. Current version is **2**.

### Migration Rules

When changing the scene format:

1. **Adding a new optional field** to an existing descriptor (e.g. adding `emissive_intensity: Option<f32>` to `DrawableDescriptor`) -- just add it with `#[serde(default)]`. No version bump needed. Old scene files without the field will deserialize with the default value.

2. **Adding a new variant** to `EntitySource` (e.g. `Terrain { heightmap: String }`) -- increment `SCENE_VERSION`. Old scene files that don't contain the new variant load fine. New scene files containing it will fail to load on older engine versions (RON cannot construct unknown enum variants). This is intentional -- it prevents silent data corruption.

3. **Removing a variant** from `EntitySource` -- increment `SCENE_VERSION`. Add a migration in `SceneManager::load_scene` that maps the removed variant to a fallback before spawning.

4. **Renaming or restructuring** fields -- increment `SCENE_VERSION`. Add a migration function that transforms old-format descriptors to the new format before spawning.

5. **Adding a new descriptor type** (e.g. `AudioSourceDescriptor`) -- add it as an `Option<T>` field on `EntityDescriptor` with `#[serde(default)]`. No version bump needed.

### Writing a Migration

Add a step in `migration.rs` and dispatch it from `run_migrations`. The loader
runs migrations and validates missing parents and cycles before preparing any
entities. A newer unsupported version returns an error and preserves the current
scene.

### Testing Migrations

- Add a test in `mod.rs` that deserializes a hand-written RON string from the old format and verifies the migrated result matches the new format.
- Keep the old-format RON string as a test fixture -- it serves as documentation of the change.
- Run `cargo test -p katla_app -- scene` to verify all round-trip and migration tests pass.

## What Gets Serialized

Per entity: name, parent, local transform, source, drawable parameters, point and
directional lights, particle emitter configuration, animation, velocity, script,
perspective, audio emitter, reverb zone, rigid body type/settings/linear velocity,
collider shape, physics material, trigger, collision filter and optional trigger rules. New optional
`rigid_body_properties` and `reverb_zone` fields default to absent in older files.

Scene files describe what to load. Spawn functions recreate GPU resources and
native physics bodies. The legacy mesh index/generation fields in mesh collider
descriptors are written as zero; loading binds the collider to the new drawable's
mesh rather than trusting a previous process's handle.

Animation snapshots retain source/target completion flags and target looping/count
independently. Reloading a completed clip does not emit completion again, and a
pending fade resumes with the same target policy. These fields use `serde(default)`
under the optional-field rule above.

Trigger rules map live generational references to unique scene names and resolve
them after loading all entities. Version 2 adds a sensor-only `Trigger` source;
version 1 scene data migrates unchanged. Consumed once rules, overlaps and
diagnostics reset on reload. Invalid/stale references reject file saving. See
[scene events](../../../docs/scene-events.md).

## What Does NOT Get Serialized

- GPU handles -- re-created on load from source descriptions
- `WorldTransform`, `TransformDirty` -- computed at runtime by systems
- `EditorHidden` -- editor state, not scene state
- The editor camera -- retained separately; authored perspective components are saved

## Document and scene lifecycle

`SceneManager::save_to_file` takes a mutable application. It serializes to a
sibling temporary file, syncs it and renames it into place before changing the
saved baseline or chosen path. Name, author and creation timestamp follow the
loaded document through repeated saves and Save As. RON comments are accepted
on input but regenerated formatting does not preserve them.

Loading prepares the new entities while the current scene still exists. A
failed spawn removes prepared entities, restores tracked resource counts and
returns an error. A successful load retires old scene entities and unreferenced
resources, clears editor selection/history, retains editor-hidden entities and
records the normalized loaded baseline. CPU geometry is removed with retired
scene meshes. Shared handles and all app-owned protected materials remain live.
Duplicate or unnamed entities receive unique serialized names; parent references
use that same mapping so a round trip cannot silently select the wrong parent.

Play captures the serialized scene and document identity. Stop restores through
the same loader, then restores the path and original saved baseline, preserving
pre-play unsaved changes. A failed restore keeps the snapshot and play mode for
retry. Custom components outside the descriptor schema are not captured.

For native scene lifecycle regressions:

```bash
cargo test -p katla_app --lib document_tests -- --ignored --test-threads=1
```

Regenerate the canonical scene deliberately after changing its serialized form:

```bash
cargo test -p katla_app --lib test_regenerate_default_scene -- --ignored
```
