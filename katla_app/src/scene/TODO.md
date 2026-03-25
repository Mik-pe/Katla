# Scene Format TODO

## Critical

- [x] Fix color space double-conversion on save/load round-trip -- save stores linear-space colors, load calls `.to_linear()` again, progressively darkening toward black. Either store sRGB in scene file or add `to_srgb()` during save. (mod.rs, spawning.rs)
- [x] `spawn_sphere_with_material` missing `EntitySource` -- PBR material grid entities saved as cubes on reload. Add `EntitySource::Sphere` with optional PBR fields, or route through `spawn_primitive_with_color`. (spawning.rs)
- [x] AnimationPlayer not created on GLTF reload -- `spawn_gltf_model` called with `default_animation: None`, so `AnimationPlayer` component is never created. The entire animation restore block is dead code during load. Animated models reload frozen. (mod.rs, animation/mod.rs)
- [x] `AnimationPlayer.duration` never serialized -- resets to 0.0 on reload, breaking `is_complete()`, `seek()`, and `time` progression. Added `duration` field to `AnimationDescriptor` with fallback to player's current duration. (descriptors.rs, mod.rs)

## Important

- [x] Animation blending/crossfade state lost on save/load -- `AnimationDescriptor` is missing `blending`, `target_clip`, `blend_weight`, `blend_time`, `blend_duration`, `loop_count`. Extend with `#[serde(default)]` fields. (descriptors.rs)
- [x] `AnimationDescriptor` missing `target_duration` -- crossfade-in-progress reloads with `target_duration: 0.0`, causing incorrect blend timing. (descriptors.rs, mod.rs)
- [ ] GPU resource leak on scene load -- `clear_entities` does not release meshes, textures, materials, skeletons. Renderer does not expose per-resource destroy APIs yet. Needs renderer integration first.
- [x] Duplicate entity names cause silent parent resolution failure on save -- added `HashSet` uniqueness check with `warn!`. (mod.rs)
- [ ] Duplicate entity names on load -- warning fires but `HashMap::insert` still overwrites, causing silent parent misrouting. Should fail load or keep first mapping. (mod.rs)
- [x] Parent resolution assumes undocumented topological entity ordering -- confirmed two-pass approach already handles arbitrary ordering. Added clarifying comment. (mod.rs)
- [x] Unnamed child entities lose parent relationship -- parent resolution uses `desc.name` to look up child entity ID, but unnamed entities get `None`. Need index-based lookup as fallback. (mod.rs)
- [x] Particle emitter entities without `TransformComponent` silently dropped by save. Fixed in both scene reload path and `setup_particle_emitters`. (mod.rs, spawning.rs)
- [ ] `ParticleEmitterDescriptor` missing `timed_emission` field -- emitter with active timed emission reloads as infinite emitter. (descriptors.rs, components/rendering/particle.rs)
- [x] EntitySource fallback silently misattributes as Cube -- entities without `EntitySource` are now skipped with a warning instead of silently serialized as cubes. (mod.rs)
- [ ] `DrawableDescriptor` missing `emission` field -- runtime emission changes lost on save/load. (descriptors.rs)
- [ ] No integration tests for load/spawn code path -- all existing tests only cover RON serialization round-trips. The animation restore, parent resolution, and EntitySource dispatch have zero runtime test coverage. (mod.rs)

## Minor

- [x] `EmitterShape` serialized as raw `u32` -- fragile to enum reordering. Serialize as enum directly or use a local mirror. (descriptors.rs)
- [x] `EntitySource` tightly coupled to spawn functions -- adding new primitives requires changes in 4 places. Consider data-driven primitive dispatch. (entity_source.rs, mod.rs)
- [x] No scene-level metadata fields (author, timestamps, engine version) -- can add later with `#[serde(default)]` when needed. (descriptors.rs)
- [x] `created_at` always equals `modified_at` -- `save_scene` creates a fresh Scene each call. To preserve original `created_at`, callers should pass the loaded Scene through. Documented as known limitation. (mod.rs)
- [ ] No scene version migration framework -- `load_scene` reads version but takes no action. When format v2 introduces breaking changes, old scenes will load incorrectly. (mod.rs)
- [ ] Forward compatibility claim incorrect for enum variants -- RON fails on unknown `EntitySource` variants. Adding new primitives is a breaking change for old loaders. (descriptors.rs, entity_source.rs)
