---
name: ecs-worker
description: Implements ECS infrastructure features for the Katla engine (katla_ecs crate)
---

# ECS Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Features that modify the `katla_ecs` crate:
- Soundness fixes (query_ref, unsafe patterns)
- Architecture changes (module moves, change detection optimization, SparseSet refactor)
- Query iterator macro-ification
- Doctest fixes
- Cross-workspace validation

## Required Skills

None. All verification is through `cargo test`, `cargo clippy`, and `cargo check`.

## Work Procedure

### 1. Read Context

Read `mission.md` and `AGENTS.md` for mission boundaries and conventions. Read `.factory/library/architecture.md` for codebase patterns.

### 2. Understand Existing Code

Before writing any code, read and understand the files you will modify:
- `katla_ecs/src/world.rs` — World struct, query methods, change detection
- `katla_ecs/src/storage.rs` — ComponentStorage, ComponentStorageManager
- `katla_ecs/src/sparse_set.rs` — SparseSet implementation
- `katla_ecs/src/query/mod.rs` and `katla_ecs/src/query/iter*.rs` — query iterators
- `katla_ecs/src/lib.rs` — public API exports

### 3. Write Tests First (TDD)

For each feature, write failing tests BEFORE implementation:
- Create test functions prefixed with `test_` in appropriate test modules
- Tests must cover the assertions listed in the feature's `fulfills` field
- Run tests to confirm they fail: `cargo test -p katla_ecs`
- Each test should have a clear pass/fail assertion

### 4. Implement

Make the tests pass by implementing the feature. Follow existing code patterns in the crate.

**Critical constraints:**
- `katla_ecs` must NOT depend on: `katla_app`, `katla_gfx`, `katla_math`, `katla_ui`
- Keep the public API surface small — `pub(crate)` by default
- Use `#[inline]` on hot path functions
- No `unwrap()` in production code
- No backwards compatibility shims — remove old code entirely

**For unsafe code:**
- Every unsafe block needs a SAFETY comment
- Consolidate unsafe patterns into helpers with single SAFETY comments

**For macro work:**
- Test generated code by verifying existing tests still pass
- Keep macro readable — prefer declarative macros over proc macros

### 5. Run Full Verification

```bash
cargo fmt
cargo test -p katla_ecs
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Fix any failures before proceeding.

### 6. Update Library

If you discover patterns, gotchas, or architectural insights, add them to `.factory/library/architecture.md`.

## Example Handoff

```json
{
  "salientSummary": "Fixed query_ref soundness by adding sealed ImmutableQuery marker trait. Only immutable patterns (&T, (&T, &U)) satisfy the bound. query (with &mut self) remains unrestricted.",
  "whatWasImplemented": "Added ImmutableQuery sealed trait in katla_ecs/src/query/mod.rs with implementations for &T and tuples of &T types. Bound query_ref's Q parameter on ImmutableQuery. Added compile-fail documentation. Added unit test verifying immutable queries work through query_ref.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {"command": "cargo test -p katla_ecs", "exitCode": 0, "observation": "All ECS tests pass"},
      {"command": "cargo test --workspace", "exitCode": 0, "observation": "All workspace tests pass"},
      {"command": "cargo clippy --workspace -- -D warnings", "exitCode": 0, "observation": "No warnings"}
    ],
    "interactiveChecks": []
  },
  "tests": {
    "added": [
      {"file": "katla_ecs/src/world.rs", "cases": [
        {"name": "test_query_ref_immutable_single", "verifies": "VAL-ECS-002"},
        {"name": "test_query_ref_immutable_tuple", "verifies": "VAL-ECS-002"}
      ]}
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- Feature depends on API that doesn't exist yet and you can't create it within this crate's boundary rules
- You discover a dependency boundary violation (e.g., katla_ecs needs something from katla_app)
- Existing tests in other crates start failing due to your changes and you can't fix them within scope
- Moving the input module requires coordination with katla_app changes beyond what's in the feature description
