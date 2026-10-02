# Trigger rules and gameplay events

The app owns gameplay rules. `katla_physics` owns overlap detection;
`katla_script` owns Luau subscriptions and deferred commands. The graphics core
has no event or gameplay policy. MCP and the editor agent share the `trigger`
operation and its validation. This is a small declarative event → visitor filter
→ ordered actions model, suitable for agent authoring and later editor controls.

## Agent authoring

Use the existing scene query tools to find entity IDs and `animation.inspect`
to discover clip names. Trigger tool inputs and inspection results use decimal
strings for complete generational entity IDs, preserving every bit in JSON clients. Call the `trigger` tool with, for example:

```json
{
  "action": "create_box",
  "name": "Entrance",
  "position": [0, 1, 0],
  "half_extents": [2, 1, 2],
  "rules": [
    {
      "event": "enter",
      "other_entity": "42",
      "once": false,
      "actions": [
        {
          "action": "play_animation",
          "target": {"kind": "other"},
          "clip": "Run",
          "fade_seconds": 0.25
        },
        {"action": "emit", "name": "entered_entrance"}
      ]
    }
  ]
}
```

`other_entity` is optional: omit it to accept any colliding visitor. Existing
`CollisionFilter` layer/mask fields control which pairs Rapier considers.
`once` defaults false; true consumes the rule on its first matching transition
in a play session. An exit rule uses `"event": "exit"`.

`play_animation` targets the trigger, visitor (`other`), or a specific live
entity (`{"kind":"entity","entity":"99"}`). Fade, looping and speed use the
[same playback contract](animation-transitions.md) as direct animation commands.
Defaults are 0.25 seconds, looping true, speed 1. An active fade rejects another
positive fade. There is no automatic retry; use a zero fade for an intentional
cut or design the rule to wait through scripted state. Actions after a failure
still execute, and inspect exposes the failure.

`create_box` validates the complete request before spawning. It creates a
kinematic sensor with a transform and no drawable/GPU resources. The box can
move through ordinary transform edits. Half extents are finite positive local
sizes; position is finite. An existing trigger can replace its entire rule list:

```json
{"action":"set_rules","entity_id":"17","rules":[]}
```

An empty list removes all behaviors. Replacement resets once-only state. Inspect
returns name, position, shape, simulation state, rules, current overlapping IDs,
consumed rule indices and the most recent
activation's errors:

```json
{"action":"inspect","entity_id":"17"}
```

There are at most 64 rules per trigger and 32 actions per rule. Rules are pure data
in `katla_agent::events`; `katla_app::events::TriggerRules` owns transient state.
A Rust app registers `RapierPhysicsSystem`; it dispatches authored actions after
each completed step. The existing application builders install physics and
script pending resources. The game registers physics before `ScriptSystem`.

## Execution and overlap semantics

Physics runs only under `PhysicsActive(true)`. Each completed Rapier step samples
actual intersecting sensor pairs, compares them with the previous set and emits
one directed transition per pair. The sensor is always `trigger_entity`. Two
sensors emit both directions. Pairs are ordered by complete entity ID; exits run
before enters. An overlap lasting many frames emits one enter. Removing a
visitor produces exit on the next step, even though its native collider is gone.
Destroying the trigger removes its rules, so it cannot act on its own removal.
Zero, negative or nonfinite delta time produces no new transition.

Sensor detection enables all body-type combinations, including fixed/kinematic
pairs; ordinary solids retain their existing policy. This follows Rapier's
[active collision type contract](https://www.rapier.rs/docs/user_guides/rust/colliders/).
The sorted overlap set costs O(k log k) for k directed intersecting pairs per
step and avoids dependence on callback ordering or dead collider lookups.
This is a correctness-oriented baseline; large overlap workloads need measured
profiling before changing representation.

Actions run in rule/list order after simulation releases its borrows. They do
not recursively step physics. Animation actions call the shared playback helper
without building an unused JSON inspection response. Simulation pause clears
once-only state, cached overlaps and undelivered physics signals; resuming starts
fresh enter detection. The current game advances animation before physics, so
the new playback clock advances on the next tick.

`PendingPhysicsEvents` retains only the latest physics batch until consumed by
`ScriptSystem`; no script consumer can leave an unbounded multi-frame backlog.
Named signals and `collision_enter`/`collision_exit` use the existing Luau event
bus. Signals contain `trigger_entity` and `other_entity`; the collision payload's
`entity_a`/`entity_b` fields identify the same pair. Lua can still emit arbitrary
script events with `world:emit(name, data)`.

## Custom behavior in Luau

Subscribe once, then use the callback's fresh `world` argument for commands:

```lua
local subscribed = false

function on_update(entity, world, dt)
    if subscribed then return end
    subscribed = true
    world:on_event("entered_entrance", function(name, data, current_world)
        current_world:play_sound("entrance.wav", 1, false)
        current_world:emit("entrance_notified", data)
    end)
end
```

Callbacks receive `(name, data, world)`. Commands use the same deferred app bridge
as update hooks. Callbacks emitted during dispatch wait until the next script
tick; no recursive event cascade runs in the current tick. Subscriptions belong
to the entity's script instance and are released on destruction, component
replacement, hot reload or disabling after repeated errors. An old captured
world proxy is a snapshot; use the current callback argument for mutations.
`ScriptsActive(false)` discards undelivered signals.

## Persistence and validation

Scene version 3 uses stable document-local keys for visitor filters and explicit
animation targets. Saving maps complete live generational IDs to those keys;
loading resolves keys after every entity has spawned. Names may be duplicated,
changed or absent without redirecting a rule. Runtime overlaps, consumed once
rules and diagnostics reset rather than entering the scene file.

Version 2 introduced the sensor-only `Trigger` source and name-based references.
The legacy reader migrates v0/v1/v2 into the current schema; a missing or ambiguous
legacy target rejects migration. Absent `trigger_rules` default empty.

Loading validates rule sizes, action parameters, component dependencies and all
references before preparing the new scene. A stale or non-serializable live target
rejects capture, Play snapshots and file saving before replacing the saved file.
It cannot silently bind to a replacement entity. See the
[scene format contract](../katla_app/src/scene/README.md).

## Validation and remaining scope

CPU tests exercise actual Rapier box transitions, orientation/body combinations,
visitor filtering, once/reset, deletion, stale generation rejection, action
failure, animation fades and Luau command/next-tick dispatch. A native app fixture
saves/loads rules, asserts new entity IDs and executes the restored trigger.
A shared native fade fixture also drives an agent-authored box through real
Rapier enter detection to GPU joint matrices at source, midpoint and target.
Linux and the existing `macos-26` workflow run the portable tests; native scene
construction runs where the graphics device supports the required backend.

This does not yet provide a visual rule editor, tag/predicate language, timers,
spatial stay events, arbitrary smooth interruption of animation fades, or
swept sensor detection for visitors crossing a whole box between two steps.
Custom state/conditions live in Luau. Extend typed actions when a concrete use
case needs them rather than adding an unbounded node graph or a second event bus.
