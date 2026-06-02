# Active Context

What's being worked on right now. **Update this file when starting or finishing a task. Remove entries when work are complete.**

## Current Work

- **Unified headless rendering** (committed):
  - Removed separate `HeadlessApplication` — headless mode now uses the same `Application` code
  - `window: Window` → `window: Option<Window>` — `None` in headless mode
  - `Application::run_headless()` drives N frames using `render_editor_frame()` (same scene, UI, render graph)
  - Loads the same scene via `SceneManager` (assets/scenes/default.katla, 41 entities)
  - Editor UI renders to offscreen texture (51 UI commands, 612 instances)
  - Builder `build_headless()` creates `Application` without winit event loop
  - All window accesses guarded with `if let Some(ref window) = self.window`

## Recent Decisions

- The offscreen texture is cloned before passing to the renderer because `render_frame` takes ownership via `.take()` — the clone remains valid for CPU readback (Shared storage mode)
- Headless frame loop manually calls ECS systems, then `render_editor_frame` — identical to the windowed `RedrawRequested` handler minus winit event processing

## Open Questions

<!-- Unresolved issues that need a decision. Remove when resolved. -->
