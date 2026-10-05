# katla_derive

Proc-macro crate. Generates trait implementations.

## Rules

- This is a proc-macro crate — no runtime dependencies. Keep it self-contained.
- `#[derive(Component)]` also generates `Inspect` impls when the `editor` feature is enabled on katla_ecs.
- `#[inspect(...)]` attributes control inspector behavior. See the [derive API documentation](src/lib.rs) for supported attributes and [component inspection](../docs/architecture.md#component-inspection) for ownership.
