# katla_app

Application framework and editor for the Katla engine.

For scene/rendering changes, read [graphics ownership](../docs/graphics_core.md);
for UI appearance, read [editor visual design](../docs/editor_ui_design.md).

## Frame Order

Each frame in `RedrawRequested` follows a strict ordering -- do not reorder without understanding the GPU sync implications:

1. `world.update(dt)` -- ECS systems (animation, transforms)
2. Poll completed asset loads and synchronize CPU scene state
3. Resize scene buffers and graph attachments before acquiring a frame
4. Update viewport bindless index (must be before UI gen)
5. `generate_ui_draw_list()` -- immediate mode UI -> GPU draw list
6. `upload_font_atlas()` -- CPU atlas to GPU (after UI gen, before render)
7. `render_frame()` -- acquire a token, prepare scene buffers and pass bindings for its slot, submit geometry, execute the graph, then present
8. Publish particle rollover and picking entity maps after accepted GPU submission, including submissions that require surface recreation
9. `process_editor_actions()` -- apply deferred UI actions

## Feature Ownership

`SceneFeatures` owns scene shaders, animation, particles, lights, default material textures, and pass bindings. `EditorFeatures` owns the font atlas and picking snapshots. Select `FrameGraphRuntime` before initializing either service or loading fonts. `GraphOnly` executes its graph without scene initialization or editor UI generation.

Mutable GPU data uses one ordinary buffer handle per frame slot. Write only the acquired token's buffers, then rebind graph imports. Picking and frame capture use typed exported image sources and readback tickets; entity mapping belongs to the committed source frame. Queue picking at click time and consume every completed ticket; only the latest click may change selection.

## Input Routing

winit events -> `InputMapper` (`KeyCombo`/`MouseCombo` -> `Action`) -> `World` input state. Game input only fires when `FocusedPanel::Viewport` is active. UI input flows through `ui_context.input` independently.

## Gotchas

- Colors in spawning functions are **sRGB**, converted to linear internally
- `FocusedPanel` gates game input and editor keyboard shortcuts
- `ResourceManager::discover()` finds `resources/` from any runtime location -- use its path helpers, never hardcode
