# katla_app

Application framework and editor for the Katla engine.

## Frame Order

Each frame in `RedrawRequested` follows a strict ordering -- do not reorder without understanding the GPU sync implications:

1. `world.update(dt)` -- ECS systems (animation, transforms)
2. `particle_system.update()` -- sync ECS emitters to GPU
3. `poll_background_loader()` -- process completed asset loads
4. Update viewport bindless index (must be before UI gen)
5. `generate_ui_draw_list()` -- immediate mode UI -> GPU draw list
6. `upload_font_atlas()` -- CPU atlas to GPU (after UI gen, before render)
7. `render_frame()` -- execute frame graph
8. `process_editor_actions()` -- apply deferred UI actions

## Input Routing

winit events -> `InputMapper` (`KeyCombo`/`MouseCombo` -> `Action`) -> `World` input state. Game input only fires when `FocusedPanel::Viewport` is active. UI input flows through `ui_context.input` independently.

## Gotchas

- Colors in spawning functions are **sRGB**, converted to linear internally
- `FocusedPanel` gates game input and editor keyboard shortcuts
- `ResourceManager::discover()` finds `resources/` from any runtime location -- use its path helpers, never hardcode
