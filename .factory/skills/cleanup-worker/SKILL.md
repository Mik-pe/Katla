---
name: cleanup-worker
description: Removes dead code, fixes clippy warnings, cleans sloppy comments, and removes worthless tests in the Katla Rust project.
---

# Cleanup Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Features involving dead code removal, clippy warning fixes, comment cleanup, or test removal in the Katla repo. Used for milestones: dead-code-cleanup, clippy-and-test-cleanup, comment-cleanup, verification.

## Required Skills

None.

## Work Procedure

### 1. Read Feature Description

Read the feature description from features.json carefully. It contains specific file paths, line numbers, and item names to target.

### 2. Read Affected Files

For each target file mentioned in the feature description:
- Read the full file to understand context
- Identify the exact code to remove or modify
- Check for any dependencies (other code that references what you're removing)

### 3. Make Changes

**Dead code removal:**
- Delete the dead code (functions, structs, methods, imports, modules)
- Also delete any `mod` declarations, `use` imports, and re-exports that become unused
- If removing a file, also remove its `mod` declaration in the parent
- Run `cargo check` after each significant removal to catch cascading issues

**Clippy fixes:**
- Fix each warning as specified
- For `derivable_impls`: replace the manual impl with `#[derive(Default)]`
- For `collapsible_if`: merge nested if conditions
- For `unnecessary_cast`: remove the cast
- For `needless_borrow`: remove the extra `&`
- For `len_without_is_empty`: add an `is_empty` method
- For `new_without_default`: implement `Default` trait
- For `new_ret_no_self`: fix return type
- For `empty_line_after_doc_comments`: remove the empty line
- For `too_many_arguments`: refactor to use a config struct or reduce parameters
- Run `cargo clippy` after each fix to verify

**Test removal:**
- Remove the specified test functions
- Remove any `#[cfg(test)]` modules that become empty after removal
- Remove any imports that were only used by the removed tests
- Run `cargo test -p {crate}` to verify remaining tests pass

**Comment cleanup:**
- Remove comments that merely restate what the code does (e.g., `// Create a new vector` before `Vec::new()`)
- Remove commented-out code blocks (not TODOs)
- Remove redundant section header dividers (`//====`, `//----`) between related items
- PRESERVE: Vulkan synchronization explanations, meaningful TODOs, non-obvious algorithm notes, public API documentation (`///`)
- When in doubt about a comment, preserve it

**Deduplication:**
- For the duplicated debug readback in application/mod.rs: keep one copy, remove the duplicate
- Verify both blocks are truly identical before removing

### 4. Format and Verify

After all changes:
```bash
cargo fmt
cargo check
cargo clippy
cargo test -p {affected_crate}
```

Fix any issues that arise. If removing code breaks a dependent crate, check whether the dependency was on dead code (remove the usage) or on live code (restore and re-evaluate).

### 5. Commit

Commit with a clear message describing what was done. One logical change per commit.

## Example Handoff

```json
{
  "salientSummary": "Removed 17 dead_code warnings across katla_ui and katla_gfx, deleted orphaned rendering/ directory and 4 stub files, removed blanket allow(dead_code) from katla_app. All 1,120 tests pass.",
  "whatWasImplemented": "Removed dead code: orphaned katla_gfx/src/rendering/ directory, stub files (buffer.rs, mesh.rs, render_pass/passes/mod.rs, animation/mod.rs), blanket #![allow(dead_code)] from katla_app/src/lib.rs, 15 dead items in katla_ui (unused methods in context/drawing, context/interaction, context/layout, context/popup, context/widgets), 2 dead functions in katla_gfx sphere.rs, 2 unused imports in katla_app asset_browser. Cleaned up entities shell module.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {"command": "cargo check 2>&1 | grep -c dead_code", "exitCode": 0, "observation": "0 dead_code warnings"},
      {"command": "cargo test -p katla_math", "exitCode": 0, "observation": "332 tests passed"},
      {"command": "cargo test -p katla_gfx", "exitCode": 0, "observation": "432 tests passed"},
      {"command": "cargo test -p katla_app", "exitCode": 0, "observation": "164 tests passed"},
      {"command": "cargo test -p katla_ui", "exitCode": 0, "observation": "77 tests passed"}
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

- Removing dead code breaks a dependent crate and it's unclear whether the usage is also dead
- A clippy fix requires an API change that seems risky
- A comment seems like it might contain important information but it's ambiguous
- cargo check or cargo test fails after changes and the cause is unclear
