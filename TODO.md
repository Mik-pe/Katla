# Katla Engine TODO

## Editor

- [x] Add click-to-focus for editor UI panels -- eagerly update focused_panel during window_event using stored panel bounds, eliminating the one-frame delay where the first click on a panel was consumed without forwarding input.
- [ ] Wire up Ctrl+S save shortcut in the editor -- scene save/load works via code but there is no keyboard shortcut to trigger save while editing.
- [x] Audit and clean up top menu bar -- removed Undo/Redo no-ops (Edit menu), removed Help > About no-op (entire Help menu). Wired up File > New Scene, Open, Save, and Quit to actual scene manager and app exit. DuplicateEntity and ResetParticleSystem remain as stubs.

- [ ] Handle window minimize/restore -- minimizing the app causes swapchain extent to become zero, which crashes or stalls the renderer. Need to skip rendering while minimized and recreate swapchain on restore.

## Scene Serialization

- [ ] GPU resource leak on scene load -- `clear_entities` does not release meshes, textures, materials, skeletons. Renderer does not expose per-resource destroy APIs yet. Needs renderer integration first. (katla_app/src/scene/mod.rs)
- [ ] No integration tests for load/spawn code path -- all existing tests only cover RON serialization round-trips. The animation restore, parent resolution, and EntitySource dispatch have zero runtime test coverage. (katla_app/src/scene/mod.rs)
- [ ] Scene version migration framework -- `load_scene` reads version but takes no action. When format v2 introduces breaking changes, old scenes will load incorrectly. Not needed until v2 format changes are introduced. (katla_app/src/scene/mod.rs)

## ECS Infrastructure

- [ ] Component removal hooks in katla_ecs -- `World::destroy_entity` silently removes components without notifying systems. This forces systems like ParticleSystem to either diff against all entities each frame (doesn't scale) or require explicit cleanup at every call site (fragile, easy to miss). Add a removal event/hook mechanism (e.g., `OnRemove<T>` callbacks or an event queue) so systems can register cleanup logic once and have it fire automatically on entity/component destruction. (katla_ecs/src/world.rs)
