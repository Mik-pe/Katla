# Odin GPU particles

The application owns particle policy and authored queues. `odin/app/particles.odin`
registers the durable emitter attachment, validates its descriptor and owns each
ordered burst queue. `odin/app/render/particles*.odin` composes ordinary generic
GPU buffers, transfers, compute dispatches and an indirect graphics phase with
one accepted scene submission. Both production Metal4 and Vulkan1.3 adapters
execute the same graph and canonical WGSL through `gfx/shader_adapter`.

## Storage and submission ownership

`Particle_Data` has a 64-byte stride, `Particle_Config` 160 bytes, `Particle_Frame`
32 bytes and `Particle_Camera` 112 bytes. Padding is initialized. Persistent GPU
private data and dead-index pools hold the simulation. Each acquired native slot
owns separate alive/working lists, counters, draw and dispatch commands, uniform
uploads, emitter/index uploads and completed-counter readback. Optional state
capture copies the command, survivor indices and complete storage into that
slot's readback buffer.

The graph copies the previous accepted survivor list into the current working
list, rolls over live/dead counts and resets the survivor counter. Emission
appends exact per-emitter requests. A GPU kernel generates the simulation's
indirect dispatch from its actual work count; simulation returns dead indices and
writes the new survivor list. A final kernel produces the graphics command with
six vertices per live particle. Billboards load the existing scene attachments,
apply the authored lifetime/scale/color behavior, test depth and blend alpha.
The scene submits geometry, models and particles with one acquired token.

Preparation can update acquired-slot uploads and graph packets, but does not
consume authored queues, publish rollover or advance emission accumulators.
Only the accepted callback commits that state. Rejection frees the staged CPU
snapshot and leaves its queue and previous accepted GPU frame intact. Completed
counter readbacks determine actual occupancy; later accepted spawns retain their
reservation until those exact submissions are observed.

## Admission and emitter retirement

A frame admits a prefix of complete burst entries that fits the reserved free
capacity. It never splits an individual burst. The accepted callback acknowledges
only the immutable prepared queue prefix; later appended entries remain ordered.
When the next entry cannot fit, simulation continues with the remaining queued
emissions retained. Continuous emission and timed-emission progress are deferred
while this bounded burst batch is blocked. A burst larger than the configured
pool returns `Particle_Capacity` and keeps its queue; the default pool supports
every individual burst accepted by the application attachment.

Particles carry an emitter index. Removing or disabling an emitter retains its
last accepted configuration, including its kill policy, while GPU particles can
still reference it. A replacement entity, including reuse of the same ECS index
with a new generation, receives a separate configuration index. Retired indices
are reclaimed only after completed counters and outstanding reservations prove
an empty particle population. An exhausted configuration pool produces a typed
failure rather than overwriting a surviving generation.

`particle_scene_validate` performs read-only candidate-scene validation before
native scene staging publishes resources. It checks descriptors, world positions,
individual burst limits and active plus retained emitter capacity. It does not
warm up the simulation or consume any requests.

## Executed evidence

`odin/examples/particles_render` executes both production adapters with native
API validation. Its first journey is actual Rapier trigger entry, the resource
script `scripts/prefab-effect.luau`, the resulting 32 queued emissions, native
compute simulation, an indirect command for 192 vertices and exact green image
pixels. GPU readbacks verify unique bounded indices, lifetime, scale and color.

The same executable proves failed-frame queue retention, deferred admission while
existing particles age, death returning all indices, acquired-slot reuse,
attachment resize and kill-on-disable. A second journey queues three concurrent
frames with 300 + 150 + 62 exact emissions, multiple GPU workgroups and a full
512-particle pool. Independent per-slot copies verify previous-frame state,
unique survivor indices, the authored gravity and both emitter cohorts. Native
delete/recreate coverage keeps the removed emitter's red particles unchanged
until GPU death, then verifies a blue replacement after safe index reclamation.

Both journeys passed on Metal4 and Vulkan1.3 with native validation and Address
Sanitizer. The tracking allocator reports no owned allocations or invalid frees.
Portable render tests cover immutable prefix publication, capacity rejection,
closed compiler behavior, retired-index staging and empty-population reclamation.

Run the native evidence on Apple Silicon with the real Naga compiler and scene
runtime dependency libraries:

```sh
ASAN_OPTIONS=detect_leaks=0 \
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
VK_ICD_FILENAMES=/usr/local/share/vulkan/icd.d/MoltenVK_icd.json \
odin run odin/examples/particles_render -vet -strict-style -sanitize:address -- \
  /path/to/katla-shader-compiler \
  /path/to/libvulkan.dylib \
  /path/to/libkatla_odin_scene_runtime.dylib \
  /absolute/project/root
```

Address Sanitizer's framework-exit leak detector is disabled for the native
Apple dependency process; the application tracking allocator remains enabled.
These automated journeys prove the particle GPU consumer. Interactive editor
acceptance and the full Odin migration remain separate application-level checks.
