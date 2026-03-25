# Katla Engine TODO

## Editor

- [ ] Add click-to-focus for editor UI panels -- currently the center panel requires a click before it receives any input. Panels should gain focus automatically on click rather than requiring a separate focus step.
- [ ] Wire up Ctrl+S save shortcut in the editor -- scene save/load works via code but there is no keyboard shortcut to trigger save while editing.
- [ ] Audit and clean up top menu bar -- most menu items are no-ops: File > New Scene, File > Open..., File > Save, File > Quit, Edit > Undo, Edit > Redo, Help > About all just close the dropdown without doing anything. Either implement them or remove them to avoid confusion. DuplicateEntity and ResetParticleSystem are also stubs logged as "not yet implemented".

## Scene Serialization

- [ ] GPU resource leak on scene load -- `clear_entities` does not release meshes, textures, materials, skeletons. Renderer does not expose per-resource destroy APIs yet. Needs renderer integration first. (katla_app/src/scene/mod.rs)
- [ ] No integration tests for load/spawn code path -- all existing tests only cover RON serialization round-trips. The animation restore, parent resolution, and EntitySource dispatch have zero runtime test coverage. (katla_app/src/scene/mod.rs)
- [ ] Scene version migration framework -- `load_scene` reads version but takes no action. When format v2 introduces breaking changes, old scenes will load incorrectly. Not needed until v2 format changes are introduced. (katla_app/src/scene/mod.rs)
