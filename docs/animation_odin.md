# Odin animation timeline ownership

Animation data, playback clocks, interpolation, skin/morph sampling and editor
commands belong to `odin/app`. `Animation_Model` and `Animation_Player` own their
clip/channel/string/event storage through the canonical ECS value operations.
Loaded SceneModel data receives the same model/player components during scene
staging; rendered skin/morph geometry samples the player's current source and
pending target using original glTF node indices.

The editor and agent call `animation_execute` with the same typed
`scene.Animation_Op`. The agent decoder rejects unknown fields, nonfinite values
and numeric entity IDs; decimal-string IDs preserve all 64 bits. Playback
inspection returns sorted clips and an owned response containing source duration,
time, speed, loop policy/count, completion state and any pending fade.

| Action | Required fields beyond entity | Behavior |
| --- | --- | --- |
| inspect | none | Current clips/player/transition; does not create a player |
| play | clip | Start a named clip with resolved asset duration; optional fade_seconds (0.25), looping (true), speed (1) |
| fade | clip | Same canonical named-clip transition; positive duration rejects overlap; zero switches immediately |
| pause | none | Freeze source/target clocks and fade progress |
| resume | none | Resume the selected clip at its retained clock; requires a configured player |
| stop | none | Reset source clock/loop count/completion and clear the pending fade; retain clip, speed, loop policy and queued events |
| seek | time_seconds | Finite values clamp to the source's 0..duration range; reset completion while preserving loop count, playing state and pending fade/target clocks |
| speed | speed | Finite nonnegative speed; zero freezes clip clocks while an active fade still progresses |
| loop | looping | Change the source loop policy; a pending target retains the explicit policy from its play/fade request |

Pause/resume/stop require an existing player. Play/fade create one when needed.
A missing clip, malformed model, stale/protected entity, overlapping positive fade
or invalid timing fails before applying the requested control. Queued completion
and loop events survive pause, stop and seek. Standalone pose clients own their
mailbox and call `animation_take_events`. The application consumes it separately
with `animation_events_dispatch` once per frame, after preview advancement or
before simulation scripts. Undo never drains or routes these packets.

During Playing, the registered native Luau runtime receives `animation_looped`
and `animation_completed` through ordinary `world:on_event` subscriptions. The
immutable payload contains `entity` userdata, lossless decimal `entity_id`,
`clip_name` and the cumulative `loop_count`; existing `trigger`/`other` fields
remain available. Source and fade-target events preserve producer order. Clip
strings belong to the packet rather than the current player, so switching clips
or removing an entity cannot invalidate accepted delivery. Callback errors use
the existing error budget and command rollback; accepted ticks retire their
packets once, while a failed tick retains them for retry.

The console drains independent owned `Animation_Feedback` packets even when its
panel is hidden. Feedback history retains at most 4096 rows and 4 MiB of clip
names, reporting how many older diagnostics were retired. This history limit
does not discard pending Luau events. Before animation advances, simulation
reserves space in the 4096-packet script mailbox. Backpressure retries existing
script feedback at zero simulation delta; failure leaves animation clocks and
undelivered packets unchanged. A single frame exceeding that capacity reports
`Invalid_Operation` rather than advancing and silently dropping events.

`animation_editor_step` advances actual poses only in Editing mode. Native frame
orchestration calls it once each frame. Playing advances through
`simulation_step`; Paused freezes animation. Starting simulation captures the
current authored player, including an editing preview's selected clip/time;
Stopping simulation restores that captured state with fresh runtime identities.
This avoids duplicate clock advancement in Playing mode.

Speed and loop changes made in Editing return the editor's shared undo command.
Undo/redo affect that setting and preserve any clock/pose progress since the
command, rather than rewinding the whole entity or its native resources. Controls
performed during simulation are temporary and are restored by simulation Stop.
Timeline pause/resume/stop/seek and clip preview use their explicit runtime control
contract; changing rendered pose does not create a parallel playback owner.

CPU ASan tests cover selected poses, source/target fades, paused progress,
finite-clamped seek, zero-speed transitions, loop/completion feedback, stale and
protected entities, rejected controls, partial-setting undo/redo and simulation
capture/pause/restore. A real imported Fox glTF fixture proves seek changes the
canonical skinned Model_Batch geometry, pause preserves its revision, stop restores
its initial geometry, and speed controls subsequent deformation. Native timeline
UI acceptance additionally requires the complete editor frame consumer; CPU pose
and batch tests alone do not establish OS input or rendered GPU acceptance.
