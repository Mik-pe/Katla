---
name: app-worker
description: Implements scene serialization, editor improvements, and integration features for the Katla engine
---

# App Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Features in the following milestones:
- **scene-serialization**: Scene serialization hardening, integration tests, version migration
- **editor**: Ctrl+S shortcut, minimize/restore, inspector inline editing

## Required Skills

None. Verification is through `cargo test`, `cargo clippy`, and manual `cargo run` where specified.

## Work Procedure

### 1. Read Context

Read `mission.md` and `AGENTS.md` for mission boundaries and conventions. Read `.factory/library/architecture.md` for codebase patterns.

### 2. Understand Existing Code

Before writing any code, read and understand the files you will modify:
- For scene features: `katla_app/src/scene/mod.rs`, `katla_app/src/scene/descriptors.rs`, `katla_app/src/scene/entity_source.rs`
- For editor features: `katla_app/src/ui/editor_ui.rs`, `katla_app/src/ui/editor_ui/inspector.rs`, `katla_app/src/application/mod.rs`, `katla_app/src/application/editor/mod.rs`

### 3. Write Tests First (TDD)

For each feature, write failing tests BEFORE implementation:
- Unit tests for serialization round-trips go in `katla_app/src/scene/mod.rs` (existing test module)
- Integration tests for load/spawn go in `katla_app/src/scene/mod.rs` as well
- Tests prefixed with `test_`
- Run tests to confirm they fail, then implement

### 4. Implement

Make the tests pass by implementing the feature. Follow existing code patterns.

**Scene serialization guidance:**
- Keep the descriptor pattern (EntityDescriptor, TransformDescriptor, etc.)
- Migration framework: `SceneMigrator` trait in `katla_app/src/scene/migration.rs` (new file)
- Migration registered in SceneManager, runs during load_scene when version < SCENE_VERSION
- Do NOT change existing EntityDescriptor field names — use migration to transform data

**Editor guidance:**
- Use deferred action pattern: push EditorAction variants to pending_actions
- Inspector editing uses katla_ui widgets: `Slider::new(&mut value, range).bounds(rect)`
- Transform editing: position/rotation/scale sliders with real-time update during drag
- Ctrl+S: check `ui_input_state.keys_pressed` for Control+S in the editor input handling code
- Minimize/restore: handle in `katla_app/src/application/mod.rs` frame loop, check for zero swapchain extent
- Follow the existing widget pattern in `katla_app/src/ui/editor_ui/inspector.rs`

### 5. Run Full Verification

```bash
cargo fmt
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

For editor features that require manual verification, note what needs manual checking in the handoff.

### 6. Manual Verification (Editor Features Only)

For editor features (inspector editing, minimize/restore, Ctrl+S):
- Run `cargo run` to verify visually
- For automated checks where possible, use `cargo run -- -s`
- Document what was manually verified in the handoff

### 7. Update Library

Add discovered patterns or insights to `.factory/library/architecture.md`.

## Example Handoff

```json
{
  "salientSummary": "Added Ctrl+S save shortcut, window minimize/restore handling, and inspector inline editing with transform sliders. Scene serialization integration tests cover all entity types. 18 tests pass.",
  "whatWasImplemented": "Added Ctrl+S keyboard shortcut detection in editor input handling, pushing EditorAction::SaveScene. Added minimized flag to Application, skip rendering when swapchain extent is zero, recreate swapchain on restore. Replaced read-only inspector text with interactive Slider widgets for Transform position/rotation/scale. Added scene version migration framework with SceneMigrator trait and v1→v2 stub.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {"command": "cargo test -p katla_app", "exitCode": 0, "observation": "All app tests pass"},
      {"command": "cargo clippy --workspace -- -D warnings", "exitCode": 0, "observation": "No warnings"},
      {"command": "cargo run -- -s", "exitCode": 0, "observation": "Single-frame validation passes"}
    ],
    "interactiveChecks": [
      {"action": "cargo run — verify Ctrl+S saves scene", "observed": "Status bar shows save confirmation, file updated on disk"},
      {"action": "Minimize window via taskbar", "observed": "No crash, no Vulkan errors, rendering resumes on restore"},
      {"action": "Select entity, drag position slider", "observed": "Entity moves in viewport in real-time"}
    ]
  },
  "tests": {
    "added": [
      {"file": "katla_app/src/scene/mod.rs", "cases": [
        {"name": "test_primitive_round_trip_cube", "verifies": "VAL-SCENE-001"},
        {"name": "test_gltf_model_round_trip", "verifies": "VAL-SCENE-002"},
        {"name": "test_migration_runs_on_version_mismatch", "verifies": "VAL-SCENE-010"}
      ]}
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- Feature requires changes to katla_gfx or katla_ecs that are outside your boundary
- Existing tests in other crates start failing due to your changes
- You discover the scene format needs breaking changes that require migration design input
- Editor UI layout breaks in ways that require design decisions
