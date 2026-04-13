---
name: todo-worker
description: Pick TODO items from TODO.md, implement them via subagents, validate, commit, and tick off completed items. Use when the user asks to work through TODO items, grab tasks from TODO, or clear out the backlog.
---

# TODO Worker

Pick actionable TODO items from `TODO.md`, implement them in parallel via subagents, validate, commit, and tick them off. Keep going — process batch after batch until no suitable candidates remain.

## Workflow

### 0. Loop

Repeat steps 1–5 continuously. After committing a batch, immediately rescan TODO.md and start the next batch. Only stop when:
- All unchecked items are blocked, architectural megatasks, or otherwise unsuitable
- A validation step fails and cannot be fixed within the batch
- The user explicitly asks to stop

### 1. Scan TODO.md

Read `TODO.md` at the project root. Identify candidate items that are:
- Self-contained (touch few files, ideally 1-3)
- Low-to-medium risk (refactors, visibility tightening, dead code removal, small API additions)
- Not blocked on other work
- Not architectural megatasks (skip "split 4000-line file" items unless explicitly asked)

Skip items already marked with `[x]` or `~~strikethrough~~`.

### 2. Pick 3-4 items per batch

Pick items that can be implemented in parallel by subagents. Group by:
- **True fixes**: Items where the code change is clear from the description
- **Investigations**: Items that might be stale or false positives (dead code, unused fields)

Prefer items across different crates for parallelism (e.g., one from katla_gfx, one from katla_ui, one from katla_app).

### 3. Launch parallel subagents

For each item, launch a `worker` subagent with a detailed prompt containing:
- The exact file paths to read and modify
- The problem description and expected fix
- Verification steps (`cargo check`, `cargo clippy`, `cargo test`)
- Constraints (which files/crates to modify, what NOT to change)

### 4. Validate all changes

After all subagents complete, launch a single validation subagent that:
- Reads modified sections to verify correctness
- Runs `cargo check` (full workspace)
- Runs `cargo clippy` on affected crates
- Runs `cargo test` on affected crates
- If any modified crate is `katla_gfx`, also runs `cargo run -- -s -v` and checks that stderr contains zero `VUID` or `ERROR` lines from Vulkan validation layers
- Reports any issues

### 5. Commit and tick off

- Stage only the files that were actually changed (use `git add` with specific paths)
- Review the diff before committing
- Commit with a concise summary following the project's commit conventions (imperative mood, 50-72 char summary line)
- Update TODO.md:
  - Change `- [ ]` to `- [x]` for successfully completed items
  - Change `- [ ]` to `- ~~description~~ — False positive. <reason>.` for items confirmed as stale/incorrect
- Commit the TODO.md update separately

## Handling False Positives

Many TODO items may be stale (code already changed since the TODO was written). When a subagent reports that an item is a false positive:
- Do NOT make code changes
- Mark it in TODO.md with `~~strikethrough~~` and explain why
- Example: `~~**Remove dead code**~~ — Stale. Methods already removed in commit abc123.`

## Commit Message Format

```
Summary line (50-72 chars, imperative mood)

- Optional bullet points for details
```

## Verification

After each batch, run:
```bash
cargo check              # Full workspace typecheck
cargo clippy -p <crate>  # Lint affected crates
cargo test -p <crate>    # Test affected crates
cargo fmt                # Format before committing
```

If `katla_gfx` was modified, also run:
```bash
cargo run -- -s -v       # 100-frame validation run, stderr must have zero VUID/ERROR lines
```

## Project Constraints

- **Dependency rules**: katla_gfx must NOT depend on katla_math, katla_ecs, katla_app, katla_ui. katla_ui can depend on katla_math and katla_gfx. katla_app can depend on all others.
- **Visibility**: Prefer `pub(crate)` until there's a clear external use case. Keep public API surface small.
- **Code style**: Follow existing patterns. Use `cargo fmt` after changes.
- **No AI slop comments**: Don't add obvious comments.

## Response Format

After all batches are complete, report a summary:

```
Batch 1:
- [x] Item 1 — <brief summary of change>
- [x] Item 2 — <brief summary of change>
- ~~Item 3~~ — False positive: <reason>

Batch 2:
- [x] Item 4 — <brief summary of change>
- [x] Item 5 — <brief summary of change>

Commits:
- <hash> <summary>
- <hash> <summary>

Remaining unchecked items: <count>
Stopped because: <reason — e.g. "no more suitable candidates" or "validation failed on batch 2">
```
