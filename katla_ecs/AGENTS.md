# katla_ecs

Custom ECS framework. Zero dependencies on other Katla crates.

## Rules

- `EntityId` is created only via `World::create_entity()` or `world.spawn()`. Never construct manually outside of tests.
- Worker systems implement `TypedSystem` and declare access once through sealed `SystemParam` types. Never add manual access declarations or pass World/registry references to workers.
- Full-World `System` implementations register through `register_exclusive_system` and run on the caller thread, including non-send resources.
- Structural commands apply only after a batch releases all typed borrows, in system registration order and FIFO within each system.
- Don't add query arities beyond 8 without explicit reason. Each arity multiplies impl combinatorics.
- Query filter types must be disjoint from query component types. The system panics at runtime if they overlap — this is intentional.
- `ImmutableQuery` sealed trait on `query_ref()` exists for soundness. Don't bypass it.
- When using the `editor` feature, all `#[derive(Component)]` structs also get `Inspect` impls. Use `#[inspect(skip)]` to exclude fields.

## Dependencies

- `katla_derive` (proc-macro for `#[derive(Component)]`)
- `rayon` (parallel system execution)
- `serde_json` (optional, `editor` feature)

## Conventions

- Components are pure data. Systems contain the logic.
- Resources (`Resource` trait) are global singletons, not per-entity.
- Use `world.spawn((A, B, C))` for entity creation — don't manually call `create_entity` + `add_component` for each.
- Read [ECS ownership and authoring](../docs/ecs.md) for storage, queries, systems and lifecycle contracts; use [benchmarks](../docs/ecs_benchmarks.md) for storage/performance decisions.
