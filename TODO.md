# Katla Engine TODO

## Editor

- [x] Add click-to-focus for editor UI panels -- eagerly update focused_panel during window_event using stored panel bounds, eliminating the one-frame delay where the first click on a panel was consumed without forwarding input.
- [x] Wire up Ctrl+S save shortcut in the editor -- scene save/load works via code but there is no keyboard shortcut to trigger save while editing.
- [x] Audit and clean up top menu bar -- removed Undo/Redo no-ops (Edit menu), removed Help > About no-op (entire Help menu). Wired up File > New Scene, Open, Save, and Quit to actual scene manager and app exit. DuplicateEntity and ResetParticleSystem remain as stubs.
- [x] Handle window minimize/restore -- minimizing the app causes swapchain extent to become zero, which crashes or stalls the renderer. Need to skip rendering while minimized and recreate swapchain on restore.
- [x] Inspector inline editing -- replace read-only property display with interactive Slider widgets for Transform, PointLight, and ParticleEmitter properties.

## Scene Serialization

- [x] GPU resource leak on scene load -- `clear_entities` does not release meshes, textures, materials, skeletons. Renderer does not expose per-resource destroy APIs yet. Needs renderer integration first. (katla_app/src/scene/mod.rs)
- [x] No integration tests for load/spawn code path -- all existing tests only cover RON serialization round-trips. The animation restore, parent resolution, and EntitySource dispatch have zero runtime test coverage. (katla_app/src/scene/mod.rs)
- [x] Scene version migration framework -- `load_scene` reads version but takes no action. When format v2 introduces breaking changes, old scenes will load incorrectly. Not needed until v2 format changes are introduced. (katla_app/src/scene/mod.rs)

## Font Rendering

- [ ] Switch font rendering framework to skrifa -- replace the current font rendering implementation with skrifa for improved font support and text shaping.

## Shadow Validation

- [x] Shadow validator tests fake data, not real shadows -- Added `render_quad_to_atlas_region()` that renders a fullscreen quad into the depth atlas via a real Vulkan graphics pipeline. Tests `test_shadow_real_geometry` and `test_shadow_depth_bias_pipeline` use real rendered depth.
- [x] Validator atlas size (256) doesn't match production (2048+) -- `SHADOW_ATLAS_SIZE` changed from 256 to 2048, depth image is now 2048x2048, shader reads atlas size from `shadow_bias.w` instead of hardcoding.
- [x] Validator slope_bias doesn't match production -- Validator now only uses `constant_bias` (`.x`), matching production. Slope bias is handled by Vulkan pipeline depth bias, not the shader.
- [x] Cascade blending only tested in trivial case -- Added `test_shadow_asymmetric_blend` that renders a quad into cascade 0 only, creating shadowed/lit boundary and verifying blend zone produces ~0.5.
- [x] Pancake projection never exercised in GPU tests -- Added `test_shadow_real_csm_matrices` that calls `CascadeShadowMap::update()` with a real camera and light to get actual VP matrices including pancake.
- [x] No edge-case cascade selection tests -- Added `test_shadow_cascade_edge_cases` (view_z at split boundary, view_z=0, beyond last split, negative view_z) and `test_shadow_zero_cascades` (num_cascades=0 returns fully lit).
- [x] Validator uses textureLoad, not textureSampleCompare -- Documented in shader header: PCF softness, comparison sampler edge cases, and UV clamping at cascade boundaries are explicitly listed as out-of-scope for the validator.
- [x] No test for Vulkan pipeline depth bias -- Added `test_shadow_depth_bias_pipeline` that renders a quad at known depth and verifies both biased and unbiased sampling at the same projected depth.
- [x] `shadow_bias.z` (normal offset) is dead code -- Clarified in production shader and Rust that .z is unused padding. GPU struct layout preserved for alignment.

## ECS Infrastructure

- [x] Component removal hooks in katla_ecs -- `World::destroy_entity` silently removes components without notifying systems. This forces systems like ParticleSystem to either diff against all entities each frame (doesn't scale) or require explicit cleanup at every call site (fragile, easy to miss). Add a removal event/hook mechanism (e.g., `OnRemove<T>` callbacks or an event queue) so systems can register cleanup logic once and have it fire automatically on entity/component destruction. (katla_ecs/src/world.rs)
