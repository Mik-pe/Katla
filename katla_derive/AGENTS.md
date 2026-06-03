# katla_derive

Proc-macro crate. Generates trait implementations.

## Rules

- This is a proc-macro crate — no runtime dependencies. Keep it self-contained.
- `#[derive(Component)]` also generates `Inspect` impls when the `editor` feature is enabled on katla_ecs.
- `#[inspect(...)]` attributes control inspector behavior. See `memory-bank/systemPatterns.md` for the full attribute reference.
