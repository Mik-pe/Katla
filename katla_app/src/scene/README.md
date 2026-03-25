# Scene Format

Runtime-mutable scene serialization for the Katla engine. Scenes are saved as human-readable `.scene` files using RON (Rusty Object Notation).

## File Structure

- `mod.rs` -- `SceneManager` (save/load), RON config, tests
- `entity_source.rs` -- `EntitySource` ECS component
- `descriptors.rs` -- RON-serializable data types (`Scene`, `EntityDescriptor`, etc.)

## Format Versioning

The `Scene.version` field tracks the format version. Current version is **1**.

### Migration Rules

When changing the scene format:

1. **Adding a new optional field** to an existing descriptor (e.g. adding `emissive_intensity: Option<f32>` to `DrawableDescriptor`) -- just add it with `#[serde(default)]`. No version bump needed. Old scene files without the field will deserialize with the default value.

2. **Adding a new variant** to `EntitySource` (e.g. `Terrain { heightmap: String }`) -- increment `SCENE_VERSION`. Old scene files that don't contain the new variant load fine. New scene files containing it will fail to load on older engine versions (RON cannot construct unknown enum variants). This is intentional -- it prevents silent data corruption.

3. **Removing a variant** from `EntitySource` -- increment `SCENE_VERSION`. Add a migration in `SceneManager::load_scene` that maps the removed variant to a fallback before spawning.

4. **Renaming or restructuring** fields -- increment `SCENE_VERSION`. Add a migration function that transforms old-format descriptors to the new format before spawning.

5. **Adding a new descriptor type** (e.g. `AudioSourceDescriptor`) -- add it as an `Option<T>` field on `EntityDescriptor` with `#[serde(default)]`. No version bump needed.

### Writing a Migration

In `SceneManager::load_scene`, check the version before the spawn loop:

```rust
let entities = if scene.version < CURRENT_VERSION {
    migrate_scene(scene.version, scene.entities)?
} else {
    scene.entities
};
```

Create `fn migrate_scene(from_version: u32, entities: Vec<EntityDescriptor>) -> Result<Vec<EntityDescriptor>, String>` that handles the transformation. Each migration step should be idempotent and handle missing data gracefully (use defaults or skip).

### Testing Migrations

- Add a test in `mod.rs` that deserializes a hand-written RON string from the old format and verifies the migrated result matches the new format.
- Keep the old-format RON string as a test fixture -- it serves as documentation of the change.
- Run `cargo test -p katla_app -- scene` to verify all round-trip and migration tests pass.

## What Gets Serialized

Per entity: name, parent, transform (pos/rot/scale), entity source type, drawable material params, point light params, particle emitter config, animation state, velocity.

GPU handles (MeshHandle, MaterialHandle, TextureHandle, SkeletonHandle, EmitterHandle) are never serialized. Scene files store *what to load*, and spawn functions re-create GPU state on load.

## What Does NOT Get Serialized

- GPU handles -- re-created on load from source descriptions
- `WorldTransform`, `TransformDirty` -- computed at runtime by systems
- `EditorHidden` -- editor state, not scene state
- Camera components -- editor-only
