# katla_script

Luau scripting via mlua. Scripts attach to entities via ECS components.

## Rules

- `ScriptEngine` is NOT thread-safe (`!Send + !Sync`). Don't share across threads.
- Sandbox is enforced: `debug`, `io`, `package`, `require`, dangerous `os` functions are stripped.
- Instruction limit is 10M, timeout is 5s. These are safety guards, not configurable.
- Scripts communicate with the engine via pending-command resources with a one-frame delay. Don't try to make synchronous calls.
- After 10 consecutive errors on a script instance, it's disabled. This prevents log spam.

## Conventions

- Script paths are relative to the scripts directory. Bare names resolve as `.luau` (e.g. `"player"` → `"player.luau"`).
- When adding new script bindings, add the Lua function, a `ScriptCommand` variant, a pending-command resource, and processing logic in katla_app.
- Read `memory-bank/systemPatterns.md` for the full pending-command resource table and lifecycle hooks.
