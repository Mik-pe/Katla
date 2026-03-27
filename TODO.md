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

- [ ] Shadow validator tests fake data, not real shadows -- The GPU shadow sampling phase never renders geometry into the depth atlas. It fills a 256x256 texture with `vkCmdClearDepthStencilImage` and samples it with a compute shader using hand-crafted VP matrices. This only tests `a <= b` arithmetic, not the actual shadow pipeline. Need to render real geometry (even a simple quad) from a light's perspective into the atlas, then sample with the production shadow logic.
- [ ] Validator atlas size (256) doesn't match production (2048+) -- The hardcoded `ATLAS_SIZE: u32 = 256u` in the validator shader and `SHADOW_ATLAS_SIZE: u32 = 256` in Rust mean texel_size values used in bias tests are wrong relative to production. Sub-texel precision issues that only manifest at 2048 are invisible. Tests should use production resolution or at minimum derive ATLAS_SIZE from the cascade params.
- [ ] Validator slope_bias doesn't match production -- The validator's `sample_shadow_manual` subtracts `shadow_bias.y` (slope bias) as a flat scalar, but the production `shadow_sampling.wgsl` never reads `.y` at all -- it only uses constant bias (`.x`). The validator is testing code that doesn't exist in production. Either remove slope_bias from the validator or implement it in production.
- [ ] Cascade blending only tested in trivial case -- The blending test clears the entire atlas to 1.0, so both cascades always return lit. Never tested: asymmetric shadow/lit boundaries (cascade 0 says shadowed, cascade 1 says lit) producing a blended ~0.5 at the blend zone midpoint.
- [ ] Pancake projection never exercised in GPU tests -- `apply_pancake` modifies the VP matrix to clamp geometry behind the light. The GPU shadow tests use hand-crafted VP matrices that skip this. Tests should use real VP matrices from `CascadeShadowMap::update()` which include pancaking.
- [ ] No edge-case cascade selection tests -- Missing: view_z exactly equal to split distance, view_z=0.0 (camera plane), view_z beyond last split, num_cascades=0. These are common failure modes in real scenes.
- [ ] Validator uses textureLoad, not textureSampleCompare -- The production path uses 16-sample PCF Poisson disc via `textureSampleCompare`. The validator uses single-point `textureLoad`. While acceptable for validating depth comparison logic, the validator should explicitly document that PCF softness, comparison sampler behavior, and UV clamping at cascade boundaries are untested.
- [ ] No test for Vulkan pipeline depth bias -- The production shadow pass sets `depthBiasSlopeFactor` from `CascadeParams.depth_bias_slope` via `set_shadow_cascade_params`. This Vulkan-level depth bias is never validated. A test rendering a known quad from the light should verify bias is sufficient to prevent self-shadowing.
- [ ] `shadow_bias.z` (normal offset) is dead code -- Documented in production but never read by any shader. Either implement normal-offset bias in the shadow sampling shader or remove the field to avoid confusion.

## ECS Infrastructure

- [x] Component removal hooks in katla_ecs -- `World::destroy_entity` silently removes components without notifying systems. This forces systems like ParticleSystem to either diff against all entities each frame (doesn't scale) or require explicit cleanup at every call site (fragile, easy to miss). Add a removal event/hook mechanism (e.g., `OnRemove<T>` callbacks or an event queue) so systems can register cleanup logic once and have it fire automatically on entity/component destruction. (katla_ecs/src/world.rs)
