# katla_ecs

Custom ECS framework. Zero dependencies on other Katla crates.

## Rules

- `EntityId` is created only via `World::create_entity()` or `world.spawn()`. Never construct manually outside of tests.
- Systems **must** override `component_access()` and `resource_access()` — even if they only read. Default "no declared access" silently makes parallel execution unsafe.
- Don't add query arities beyond 8 without explicit reason. Each arity multiplies impl combinatorics.
- Query filter types must be disjoint from query component types. The system panics at runtime if they overlap — this is intentional.
- `ImmutableQuery` sealed trait on `query_ref()` exists for soundness. Don't bypass it.
- When using the `editor` feature, all `#[derive(Component)]` structs also get `Inspect` impls. Use `#[inspect(skip)]` to exclude fields.

## Dependencies

- `katla_derive` (proc-macro for `#[derive(Component)]`)
- `paste` (query macro hygiene)
- `rayon` (parallel system execution)
- `serde_json` (optional, `editor` feature)

## Conventions

- Components are pure data. Systems contain the logic.
- Resources (`Resource` trait) are global singletons, not per-entity.
- Use `world.spawn((A, B, C))` for entity creation — don't manually call `create_entity` + `add_component` for each.
- Read `memory-bank/systemPatterns.md` for the full architecture description of sparse sets, queries, storage, and events.
