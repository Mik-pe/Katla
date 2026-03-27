---
name: ecs-infra-worker
description: Implements ECS infrastructure and GPU resource management features for the Katla engine
---

# ECS Infrastructure Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Features in the following milestones:
- **ecs-infra**: Entity/component events, change detection, query conversion (katla_ecs crate)
- **gpu-resources**: Per-resource destroy APIs, scene load GPU leak fix, auto-cleanup (katla_gfx + katla_app)

## Required Skills

None. All verification is through `cargo test`, `cargo clippy`, and `cargo check`.

## Work Procedure

### 1. Read Context

Read `mission.md` and `AGENTS.md` for mission boundaries and conventions. Read `.factory/library/architecture.md` for codebase patterns.

### 2. Understand Existing Code

Before writing any code, read and understand the files you will modify:
- For ECS features: `katla_ecs/src/world.rs`, `katla_ecs/src/storage.rs`, `katla_ecs/src/lib.rs`
- For GPU features: `katla_gfx/src/renderer/mod.rs`, `katla_gfx/src/renderer/registry.rs`, `katla_gfx/src/texture/manager.rs`, `katla_app/src/scene/mod.rs`

### 3. Write Tests First (TDD)

For each feature, write failing tests BEFORE implementation:
- Create test functions prefixed with `test_` in appropriate test modules
- Tests must cover the assertions listed in the feature's `fulfills` field
- Run tests to confirm they fail: `cargo test -p katla_ecs` or `cargo test -p katla_gfx` or `cargo test -p katla_app`
- Each test should have a clear pass/fail assertion

### 4. Implement

Make the tests pass by implementing the feature. Follow existing code patterns in the crate you're modifying.

**ECS-specific guidance:**
- Events go in `katla_ecs/src/events.rs` (new file)
- Change detection uses generation counters in ComponentStorage
- `query_changed` is a new World method, not a replacement for `query`
- Do NOT add serde to katla_ecs

**GPU-specific guidance:**
- `AssetRegistry` needs `remove_mesh`/`remove_material` returning `Option<T>`
- `VulkanRenderer` exposes `destroy_*` methods that delegate to subsystems
- Default textures must NEVER be destroyed — protect with guards
- Reference counting for shared resources is at katla_app layer, not katla_gfx

### 5. Run Full Verification

After implementation:
```bash
cargo fmt
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Fix any failures before proceeding.

### 6. Update Library

If you discover patterns, gotchas, or architectural insights, add them to `.factory/library/architecture.md`.

## Example Handoff

```json
{
  "salientSummary": "Implemented entity lifecycle events (EntityEvent::Spawned/Destroyed) and component events (ComponentEvent::Added/Removed) in katla_ecs. Added change detection via generation counters with query_changed() API. All 26 ECS tests pass.",
  "whatWasImplemented": "Added katla_ecs/src/events.rs with EntityEvent and ComponentEvent enums. Extended World with event queues, entity_events(), component_events_for::<T>(), query_changed::<Q>(). Events flushed at end of update(). Generation counters in ComponentStorage incremented on add_component and get_component_mut. 26 unit tests covering all assertion IDs.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {"command": "cargo test -p katla_ecs", "exitCode": 0, "observation": "All 26 ECS tests pass"},
      {"command": "cargo clippy -p katla_ecs -- -D warnings", "exitCode": 0, "observation": "No warnings"},
      {"command": "cargo test --workspace", "exitCode": 0, "observation": "All workspace tests pass"},
      {"command": "cargo clippy --workspace -- -D warnings", "exitCode": 0, "observation": "No warnings workspace-wide"}
    ],
    "interactiveChecks": []
  },
  "tests": {
    "added": [
      {"file": "katla_ecs/src/events.rs", "cases": [
        {"name": "test_entity_spawn_event_emitted", "verifies": "VAL-ECS-001"},
        {"name": "test_entity_destroyed_event_emitted", "verifies": "VAL-ECS-002"},
        {"name": "test_destroy_invalid_entity_no_event", "verifies": "VAL-ECS-003"},
        {"name": "test_entity_events_flushed_after_update", "verifies": "VAL-ECS-004"},
        {"name": "test_entity_event_ordering", "verifies": "VAL-ECS-005"}
      ]},
      {"file": "katla_ecs/src/storage.rs", "cases": [
        {"name": "test_component_added_event", "verifies": "VAL-ECS-006"},
        {"name": "test_component_removed_event", "verifies": "VAL-ECS-007"},
        {"name": "test_destroy_entity_emits_component_removed_events", "verifies": "VAL-ECS-008"},
        {"name": "test_component_events_type_safety", "verifies": "VAL-ECS-009"},
        {"name": "test_component_events_flushed_after_update", "verifies": "VAL-ECS-010"}
      ]}
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- Feature depends on API that doesn't exist yet and you can't create it within this crate's boundary rules
- You discover a dependency boundary violation (e.g., katla_ecs needs something from katla_app)
- Existing tests in other crates start failing due to your changes
- GPU resource destroy requires Vulkan API knowledge you're unsure about
