# Trigger rules and gameplay events

`odin/app` owns authored rules and ordered action dispatch. Native Box3D owns
completed sensor intersections; `odin/script` owns Luau instances/subscriptions
and typed deferred world commands. The GPU core has no gameplay policy. MCP and
the editor share one trigger service and shared mutation history.

## Authoring

Find current generational entity IDs through scene queries and use `animation`
inspection for clip names. Tool IDs are full decimal strings:

```json
{
  "action": "create_box",
  "name": "Entrance",
  "position": [0, 1, 0],
  "half_extents": [2, 1, 2],
  "rules": [{
    "event": "enter",
    "other_entity": "42",
    "once": false,
    "actions": [
      {"action":"play_animation","target":{"kind":"other"},"clip":"Run","fade_seconds":0.25},
      {"action":"emit","name":"entered_entrance"}
    ]
  }]
}
```

Omit `other_entity` to accept any intersecting visitor; an exit rule uses
`"event":"exit"`. Collision layer/mask settings control native pair admission.
`once` defaults false; true consumes a matching rule once in the play session.

Action targets are the trigger, visitor (`other`) or an explicit live entity.
`play_animation` shares [animation playback/fade semantics](animation_odin.md),
with looping true, speed 1 and a 0.25-second default fade. An incompatible active
fade fails explicitly; zero fade requests an intentional cut. Actions after a
failed action still execute, and inspection retains the errors.

`burst_particles` requires an active emitter and count 1–100,000. Activate an
emitter with `set_particles_active` earlier in the same rule when needed.
Explicit particle targets must already have the component. A trigger targeting
itself needs that attachment before rules are installed; create it with empty
rules first. Queues hold at most 1,024 bursts and the renderer consumes only the
queue prefix accepted with its GPU submission.

`create_box` validates the complete request before creating a kinematic sensor
with `Scene_Transform`, `Physics_Body`, `Trigger_Volume` and `Trigger_Rules`.
There is no drawable allocation. Half extents are positive local dimensions and
position is finite. Transform edits move the sensor through native preparation.
`set_rules` replaces the entire list and resets once state; an empty list removes
behavior. `inspect` returns current rule data, simulation state, directed overlaps,
consumed rule indices and the latest activation errors.

There are at most 64 rules and 32 actions per rule. Types live in
`odin/agent/scene/events.odin`; owned application components and dispatch live in
`odin/app/events.odin`. Changes require editing mode and go through the same
atomic history/native admission as inspector and hierarchy mutations.

## Completed-step semantics

While Playing, `simulation_step` advances animation, dispatches animation signals,
steps native physics, dispatches sensor actions and then runs scripts. Pairs come
from actual completed Box3D sensor intersections and are sorted by complete entity
identity. Exits precede enters. The sensor is always `trigger`; two sensors emit
both directed transitions. A sustained overlap emits one enter. Removing a
visitor yields an exit at the next completed step; deleting the trigger removes
its actions. Sensor admission includes fixed/kinematic pairs.

Actions run in authored order after physics releases its native state. They do
not recursively step physics. An animation started by a rule advances its clock
on the following tick. Pause freezes execution; Resume continues the same preview
entities. Beginning/restoring a play session resets transient overlap, once and
error state. Invalid delta time is rejected and zero delta does not produce a new
physics transition.

Named emissions use the bounded `Script_Signals` owner. Native script dispatch
uses lossless entity userdata in `data.trigger` and `data.other`; diagnostic
`trigger_entity`/`other_entity` are decimal strings, not rounded numbers. Script
callbacks may emit arbitrary typed payloads with `world:emit`.

## Luau callbacks

Subscribe once and use the callback's current world argument:

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

Callbacks receive `(name, data, world)`. Commands use typed native host operations
and application admission. Events emitted during dispatch wait for the next script
tick, avoiding recursive cascades. Subscriptions belong to an instance and are
released on destruction, replacement, hot reload or repeated-error disablement.
Use the fresh callback world for mutations; a previously captured proxy describes
an earlier snapshot. See [script runtime contracts](katla_script_architecture.md).

## Persistence and deletion

Scene v3 stores document-local keys for visitors and explicit action targets.
Capture maps live generational references; staging maps them after all entities
exist. Duplicate/renamed/absent labels do not redirect rules. Runtime overlaps,
once consumption and diagnostics are reconstructed, rather than saved.

The v0/v1/v2 reader converts legacy name-based references. Missing or ambiguous
names fail migration; absent rules are empty. Document admission validates sizes,
action values, component dependencies and references before publication. A stale
reference rejects capture/Save/Play instead of binding to a replacement entity.
See [scene format](scene_format.md).

Authored deletion prunes incoming visitor filters, explicit actions and overlaps
in the same command, including prefab removal. Native rejection leaves all
references intact. Undo restores fresh IDs and remaps surviving rules/joints.
Runtime deletion retains authored stale targets for diagnostics; Stop restores the
pre-play scene. These policies are application composition, not a second event bus.

## Verification and bounds

`odin run tools/build -- --tests --sanitize` runs configured native
CPU suites. [`tools/build validate physics`](../tools/build/validation.odin) exercises
real sensor transitions/body combinations and native hierarchy/mesh/joint owners;
[`tools/build validate luau`](../tools/build/validation.odin) exercises actual protected
VM calls and event/deferred command lifecycle. Combined particle/render acceptance
uses `tools/build validate render --particles` with both adapters and explicit Luau,
Box3D and Vulkan dependencies.

Tests cover once/reset, filtering, action failure, generational reference rejection,
deleted visitors, persistent rules, restored IDs and native GPU particle/animation
consumers. Cross typechecks do not prove unavailable native devices or OS behavior.

Rules describe enter/exit transitions; they do not imply swept sensor detection,
stay/timer events or arbitrary smooth fade interruption. Stateful predicates can
use Luau. Keep new actions typed and bounded when a concrete use case needs them.
