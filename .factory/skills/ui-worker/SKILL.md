---
name: ui-worker
description: Implements UI system features for the Katla engine (katla_ui crate)
---

# UI Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Features that modify the `katla_ui` crate:
- Layout consistency (removing one layout style)
- Performance improvements (font loading without Box::leak)
- Widget ergonomics (at_cursor(), cursor advancement)
- Polish (border color cleanup)

## Required Skills

None. All verification is through `cargo test`, `cargo clippy`, and `cargo check`.

## Work Procedure

### 1. Read Context

Read `mission.md` and `AGENTS.md` for mission boundaries and conventions. Read `.factory/library/architecture.md` for codebase patterns.

### 2. Understand Existing Code

Before writing any code, read and understand the files you will modify:
- `katla_ui/src/context/layout.rs` — layout system (horizontal/vertical closures, begin_row/end_row)
- `katla_ui/src/context/widgets.rs` — widget behavior helpers
- `katla_ui/src/context/widgets/basic.rs` — internal widget rendering
- `katla_ui/src/widgets/mod.rs` — public builder widgets
- `katla_ui/src/text/font_loading.rs` — font loading with Box::leak
- `katla_ui/src/lib.rs` — public API

Also read consumers in `katla_app/src/ui/` to understand how layout and widget APIs are used.

### 3. Write Tests First (TDD)

For each feature, write failing tests BEFORE implementation:
- Create test functions in `katla_ui/src/context/tests.rs` (existing test module)
- Tests must cover the assertions listed in the feature's `fulfills` field
- Run tests to confirm they fail: `cargo test -p katla_ui`

### 4. Implement

Make the tests pass. Follow existing code patterns.

**Critical constraints:**
- `katla_ui` must NOT depend on: `katla_ecs`, `katla_app`
- `katla_ui` CAN depend on: `katla_math`, `katla_gfx`
- When removing a layout style, update ALL consumers in katla_app first
- Keep the public API surface small

**Layout style removal:**
- Audit all call sites in katla_app/src/ui/ for both patterns
- Choose the more commonly-used style as primary
- Update all consumers to primary style before removing the non-primary
- Document the chosen style

**Font loading fix:**
- The `'static` lifetime requirement comes from `FontRef<'static>`
- Consider using `owning_ref` or storing bytes + font separately
- FontSystem must outlive all font references it creates

### 5. Run Full Verification

```bash
cargo fmt
cargo test -p katla_ui
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Fix any failures before proceeding.

### 6. Update Library

If you discover patterns, gotchas, or architectural insights, add them to `.factory/library/architecture.md`.

## Example Handoff

```json
{
  "salientSummary": "Removed begin_row/end_row layout style in favor of closure-based horizontal/vertical. Updated all consumers in katla_app. Added at_cursor() to Checkbox, Slider, TextInput, Label widgets.",
  "whatWasImplemented": "Removed begin_row(), end_row(), begin_column(), end_column() from UiContext. Updated 12 call sites in katla_app to use horizontal()/vertical() closures. Added at_cursor() builder method to Checkbox, Slider, TextInput, Label. Standardized cursor advancement for all widgets that draw at cursor.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {"command": "cargo test -p katla_ui", "exitCode": 0, "observation": "All UI tests pass"},
      {"command": "cargo test --workspace", "exitCode": 0, "observation": "All workspace tests pass"},
      {"command": "cargo clippy --workspace -- -D warnings", "exitCode": 0, "observation": "No warnings"}
    ],
    "interactiveChecks": []
  },
  "tests": {
    "added": [
      {"file": "katla_ui/src/context/tests.rs", "cases": [
        {"name": "test_checkbox_at_cursor", "verifies": "VAL-UI-003"},
        {"name": "test_slider_at_cursor", "verifies": "VAL-UI-003"},
        {"name": "test_cursor_advances_after_label", "verifies": "VAL-UI-004"}
      ]}
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- Feature requires changes to katla_gfx or katla_ecs that are outside your boundary
- Existing tests in other crates start failing due to your changes and you can't fix them
- Removing a layout style affects too many consumers and requires coordination
- Font loading fix requires a different approach than described
