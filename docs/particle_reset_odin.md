# Global Odin particle reset

`render.particle_reset_all(&consumer)` queues a global ResetSystem operation on
the particle consumer shared by all view compositions. Calls before preparation
coalesce. A prepared frame returns Busy and leaves the queued request unchanged.
The editor invokes this operation through its render callback; it is separate
from restarting an authored emitter's clock.

Preparation creates a fully initialized replacement GPU particle pool and free
list. The reset frame reads initial counters and generates zero indirect vertices
through the normal compute/transfer graph. All follower views use that same pool
and indirect command. Public pool handles swap only after the common native
submission is accepted. Older accepted recordings retain their old owners;
allocation failure or an aborted frame releases only the candidate parents and
keeps the request queued for a later attempt.

Reset preserves authored emitter descriptors, activation, timed duration and
scene history. It does not advance committed emitter clocks or consume pending
bursts. Pending bursts and continuous emission resume on the next accepted
normal frame. Old simulation reservations and observed counts are replaced by
the accepted zero state, so later stale slot readbacks cannot resurrect particles.
Survivor indices and counters live in explicit persistent GPU rollover buffers.
Every accepted frame copies the completed result into these buffers; the next
frame reads that snapshot independently of the renderer's acquired slot number.
Unrelated texture/upload submissions can therefore advance the renderer ring or
return the same slot without changing particle continuity.

The native `render_features` executable proves 16 live particles → 0 → a new
8-particle burst on Metal and Vulkan. Four independent camera images return
exactly to their particle-free baselines during reset. Busy and aborted attempts
retain the old 16-particle GPU counters and accepted pool handles; retry succeeds.
The fixture deliberately submits unrelated transfer work until the next particle
frame reacquires its previous slot, then verifies two such frames. It verifies
zero particle lifetimes, indirect commands, emission clocks, authored
configuration, preserved queued bursts and one shared commit. CPU ASan/leak tests
also inject failure during the second native allocation and verify rollback.
