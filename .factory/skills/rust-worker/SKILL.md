---
name: rust-worker
description: General-purpose Rust worker for implementing features, refactoring, and fixing issues in the Katla 3D engine workspace.
---

# Rust Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

All features in this mission: structural decomposition, editor features, billboard GPU picking, robustness improvements, code reusability, and polish.

## Required Skills

None — this worker uses standard Rust tooling (cargo, clippy, rustfmt).

## Work Procedure

### Step 1: Understand the Feature

Read the feature description carefully. Identify:
- Which crate(s) and file(s) are affected
- What behavioral changes are required
- What tests exist that must not break

Read relevant source files to understand the current code structure and patterns before making changes.

### Step 2: Write Tests First (Red)

Before implementing any change, write failing tests that verify the desired behavior:
- For new functionality: write tests that exercise the new API/behavior
- For refactoring: verify existing tests cover the behavior being preserved
- For bug fixes: write a test that reproduces the bug (should fail), then fix it

Run `cargo test -p <crate>` to confirm tests fail as expected.

### Step 3: Implement (Green)

Make the minimal changes needed to pass the tests:
- Follow existing code patterns in the file/module
- Respect crate dependency restrictions (see AGENTS.md)
- Use `pub(crate)` by default; only promote to `pub` with clear need
- No AI slop comments
- No backwards compatibility paths — remove old code entirely

### Step 4: Verify

After implementation, run the full verification suite in order:

1. **Format:** `cargo fmt`
2. **Typecheck:** `cargo check --workspace`
3. **Lint:** `cargo clippy --workspace`
4. **Test:** `cargo test --workspace`
5. **Feature flags:** `cargo check -p katla_app --no-default-features` (if editor code was touched)
6. **Headless GPU:** `cargo run -- -s` (if rendering/GPU code was touched, timeout 90s)

Fix any failures before proceeding.

### Step 5: Commit

Stage and commit changes following project conventions:
- Imperative mood summary (50-72 chars)
- Optional bullet points with hyphens
- No Co-Authored-By tag
- Run `cargo fmt` before committing

### Step 6: Handoff

Report what was done, what tests were added, and what (if anything) was left incomplete.

## Example Handoff

```json
{
  "salientSummary": "Split scene/mod.rs into 4 submodules (tests.rs, default_scene.rs, serialization.rs, spawning.rs). All 50 scene tests preserved. mod.rs reduced from 3680 to 47 lines.",
  "whatWasImplemented": "Extracted scene test module (~2636 lines) to tests.rs, build_default_scene() (~415 lines) to default_scene.rs, serialization functions (~180 lines) to serialization.rs, spawn_entity() (~200 lines) to spawning.rs. Updated mod.rs with submodule declarations and re-exports.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      { "command": "cargo test -p katla_app scene::", "exitCode": 0, "observation": "All 50 scene tests pass" },
      { "command": "cargo check --workspace", "exitCode": 0, "observation": "Clean build" },
      { "command": "cargo clippy --workspace", "exitCode": 0, "observation": "No warnings" },
      { "command": "cargo fmt -- --check", "exitCode": 0, "observation": "All formatted" }
    ],
    "interactiveChecks": []
  },
  "tests": {
    "added": []
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- Feature depends on code that doesn't exist yet (e.g., another feature's API)
- Requirements are ambiguous or contradictory
- Existing bugs in unrelated code block this feature
- `cargo check --workspace` fails and the error is clearly from another feature's incomplete work
- Headless GPU validation fails and the cause is not from this feature's changes
