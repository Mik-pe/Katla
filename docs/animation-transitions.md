# Animation playback and crossfades

Animation policy belongs to `katla_app`. The graphics core receives prepared
clips, joint data and per-frame parameters through ordinary graph buffers.
MCP and the editor assistant share the `animation` operation defined in
`katla_agent::animation::AnimationOp`; both execute it on the application thread
at their existing request-processing boundary, outside borrowed ECS jobs.

## Agent workflow

Discover the entity with the scene query tools, then inspect its animations:

```json
{"action":"inspect","entity_id":42}
```

The result lists exact clip names and durations in stable name order, along with
current playback and transition progress. Play a clip by name:

```json
{"action":"play","entity_id":42,"clip":"Run","fade_seconds":0.25}
```

The defaults are a 0.25-second fade, looping enabled and speed 1. The engine
resolves clip duration from the loaded model; callers never supply clip indices,
duration or blend weights. `fade_seconds` and `speed` must be finite and
nonnegative. Speed zero freezes clip sampling but allows the fade clock to
advance. Reverse playback is unsupported by this operation. Unknown names,
stale entities and invalid requests return errors before changing playback.
An entity without a current clip starts the requested clip immediately.

A positive fade requires the previous fade to have completed. Inspect reports
its target, elapsed time and progress so an agent can retry. This bounded
source/target contract keeps work and storage predictable and avoids snapping
to an arbitrary source when interrupted. A zero-second request explicitly
switches immediately, including during a fade. Arbitrary continuously
interruptible multi-layer blends require a separate pose/layer design; an
animator state machine can later schedule these same semantic operations.

Requests change live playback. Saving a scene captures that playback, including
its transition and per-clip completion/loop state. They do not author a state
machine. No extra animation asset format is needed for named clip crossfades.

## Timing and pose contract

A fade uses unscaled elapsed seconds; source and target clip times use playback
speed. Pausing freezes both clip clocks and the fade clock. Completed non-looping
clips hold their final sample while the fade continues. Target looping policy
is independent of the source and becomes current policy when the fade finishes.
A constant clip (duration zero) holds time zero: a loop never computes modulo
zero or emits synthetic loop events; a non-loop emits completion once.
Completion events are emitted once per active clip, including the target if it
finishes before the fade. Target loop count carries into current playback.
Invalid or negative frame deltas do not advance playback.

GPU parameters retain the source weight convention: 1 at the beginning, 0 at
the end. The shader interpolates local translation/scale and slerps local
rotation, then composes hierarchy matrices and inverse bind matrices. This is
consistent with the [ozz-animation local-space pipeline](https://guillaumeblanc.github.io/ozz-animation/documentation/animation_runtime/).
Fade completion promotes the sampled target at its current time, preserving the
endpoint pose. Hierarchy preparation and asset revision remain separate tasks
in TODO.md.

## Deterministic CI contract

CPU tests drive the real typed `AnimationUpdateSystem` with fixed delta times,
not wall-clock sleeps. They exercise the shared semantic operation, defaults,
invalid requests without mutation, pause, constant clips, completed sources,
target looping and transition completion. MCP and assistant tools use the same
request type and execution function.

Native fixtures additionally drive a semantic agent request through the typed
system and animation shader using normal graph bindings, then read joint matrices
back. They verify fade endpoints, child bone length and signed/zero scales. These checks run against Vulkan on Linux and
Metal on capable Apple Silicon with API validation; unavailable Metal 4 hardware
is reported as blocked, as described in [CI policy](ci.md). CPU state assertions
and actual GPU output assertions establish different parts of the contract.

```bash
cargo test -p katla_app --lib animation --all-features --locked
cargo test -p katla_agent --lib --all-features --locked
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_app --lib native_transition_tests --all-features --locked -- --ignored --test-threads=1
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_gfx --lib render_graph::native_compute_tests::animation --locked -- --test-threads=1
```
