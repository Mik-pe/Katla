# Scene Format TODO

## Critical

- [ ] Fix color space double-conversion on save/load round-trip -- save stores linear-space colors, load calls `.to_linear()` again, progressively darkening toward black. Either store sRGB in scene file or add `to_srgb()` during save. (mod.rs, spawning.rs)
- [ ] `spawn_sphere_with_material` missing `EntitySource` -- PBR material grid entities saved as cubes on reload. Add `EntitySource::Sphere` with optional PBR fields, or route through `spawn_primitive_with_color`. (spawning.rs)

## Important

- [ ] Animation blending/crossfade state lost on save/load -- `AnimationDescriptor` is missing `blending`, `target_clip`, `blend_weight`, `blend_time`, `blend_duration`, `loop_count`. Extend with `#[serde(default)]` fields. (descriptors.rs)
- [ ] GPU resource leak on scene load -- `clear_entities` does not release meshes, textures, materials, skeletons. Iterate entities and drop GPU resources before clearing. (mod.rs, renderer integration)
- [ ] Duplicate entity names cause silent parent resolution failure -- `HashMap::insert` overwrites. Add uniqueness check in `save_scene` or use `MultiMap`. (mod.rs)
- [ ] Parent resolution assumes undocumented topological entity ordering -- hand-edited scene files can break parent links. Sort entities before spawning or add multi-pass resolution. (mod.rs)
- [ ] Particle emitter entities without `TransformComponent` silently dropped by save. Add transform in the `ParticleEmitter` spawn arm. (mod.rs)

## Minor

- [ ] `EmitterShape` serialized as raw `u32` -- fragile to enum reordering. Serialize as enum directly or use a local mirror. (descriptors.rs)
- [ ] `EntitySource` tightly coupled to spawn functions -- adding new primitives requires changes in 4 places. Consider data-driven primitive dispatch. (entity_source.rs, mod.rs)
- [ ] No scene-level metadata fields (author, timestamps, engine version) -- can add later with `#[serde(default)]` when needed. (descriptors.rs)
