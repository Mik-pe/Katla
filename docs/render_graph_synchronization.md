# Render graph synchronization

The execution plan orders native resource operations at each pass boundary.
The compiler consumes the same typed image and buffer accesses as the dependency
DAG. Image ranges retain aspects, mips and layers; buffers retain byte ranges,
usage and native execution stage. A resource-indexed frontier retains the current
writer and every outstanding reader until the next write replaces those ranges.
Visibility already established for an identical consumer scope is coalesced;
readers at other stages and later writers keep their required ordering. Buffer
frontiers also seed the next frame. A renderer-wide history resolves physical
native buffer identity and slice offsets, retaining source scopes even when roles
alternate or a different graph removes its former writer. History conservatively
accumulates writer and reader scopes; aborted encodings and zero dispatches cannot
erase real in-flight dependencies. Identical overlapping scopes merge. Completed
native allocation destruction retires its entry, and a successful full GPU drain
clears history. Frame-slot completion never clears another slot's source scopes.

Each live pass declares its operation. Graphics, compute and transfer operations
produce render, compute and blit encoder requirements, respectively. Queue identity
is explicitly the graphics queue, including transfer producers. The plan retains
canonical predecessor indexes. Pass names have no synchronization meaning.
Asynchronous graph queues are not implemented, so graph work never silently moves
to another queue.

Imported images use generational TextureHandle identities and explicit initial and
required-final contracts. Queued uploads seed exact mip/layer transfer-write scopes
before imported image consumers. Shader encoders also carry an explicit upload
batch dependency for material textures consumed through the bindless table. Frame-end contract operations also execute when
an imported image has no live pass. An undefined initial contract still means
undefined contents; a final layout transition does not initialize those contents.
The canonical compiler rejects color, depth and stencil Load operations before
an authored producer defines the corresponding imported range. Clear followed by
Store defines that aspect; StoreDontCare discards it. A declared non-Undefined
initial contract explicitly supplies externally initialized contents. Vulkan binds the acquired output's final
consumer to presentation in windowed mode and transfer readback in headless mode.
The acquire semaphore waits at all commands, covering graphics, compute and
transfer first uses. Each acquired output retains its actual layout and whether
its contents were stored. Submission commits that state; abort does not. New
and resized outputs reject Load before encoding until an authored attachment
write initializes them. Empty final transitions do not initialize output pixels.

Vulkan lowers typed scopes through synchronization2 image and buffer barriers.
A same-layout hazard always reaches the driver. Layout tracking partitions image
subresource ranges instead of applying one mip's state to its siblings. Encoded
transient layout changes are journaled: submission commits them, and abort
restores their actual pre-encoding range states. Buffer
barriers retain declared ranges plus the resolved physical slice offset. Compiled
allocation handoffs include frame-cycle reuse and emit a memory dependency before
a physical range is reused by another aliased image.

Metal 4 treats resources as untracked regardless of descriptor hazard mode. Its
adapter consumes the same image/buffer producer and consumer scopes and emits
native stage dependencies at encoder boundaries. Physical-range reuse additionally
requires ResourceAlias visibility. Queue submissions and upload producer batches
must use this explicit ordering rather than relying on retained native objects or
legacy driver tracking.

Host reads require completion of the owning submission before mapping. Native
resource residency and frame-slot retirement are allocation/lifecycle contracts;
a stage barrier alone does not make a resource resident or safe for CPU reuse.

Focused tests cover attachment sampling, storage read-modify-write, multiple shader
reader stages, WAR/WAW ordering, buffer slices, transfer upload scopes, untouched
imported final contracts, operation-derived encoder boundaries, and independent
mip layouts. Native validation must additionally exercise the backend's actual
encoded scopes and presentation/upload paths.

## Access declarations and liveness

Built-in pass templates declare typed NamedImageAccess explicitly. Coarse access
refinement is only for low-level PassDesc/SimplePass. Generic sampled reads cover
all aspects; narrow ranges only with knowledge of the actual image format. A
color-only declaration must not silently remove a depth sampling dependency.

Exported resources and explicit side effects are liveness roots. Preserve all
surviving final writers for disjoint byte ranges, mips and layers; fully
overwritten writers may be culled. Loads and blending declare read-before-write.
Reject submissions to culled passes, and never encode hidden work for an absent
pass. Named depth_target declarations identify graph-owned depth attachments.
Native encoders translate the compiled order and authored color/depth/stencil
operations without singleton scene targets or semantic pass-name heuristics.
