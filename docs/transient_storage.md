# Transient texture storage

The compiler computes first and last scheduled access after pass culling. Live
sampled, storage, transfer/readback and exported accesses are recorded as
persistence requirements. A resource spanning two native render passes requires
persistent backing even when both accesses are attachment accesses.

The texture allocation plan separates color, sampled depth, unsampled depth and
sampled-image classes. Dimensions and exact format are part of the compatibility
key. Textures currently have one sample, mip and layer; compressed, stencil and
depth formats remain separate because their exact native formats differ. Only
strictly disjoint execution intervals can share a range. Imported images and
backbuffers do not enter the transient allocator. Exported resources are pinned.

Metal creates one placement heap per compatible physical range and frame slot.
Every aliased member occupies offset zero of that range. The heap remains retained
by the textures, and the command buffer residency set includes each texture's
owning heap. Metal 4 treats resources as untracked: the graph emits a native
resource-alias barrier at the start of each aliased member's interval, including
the first interval when a frame slot is reused. Heap descriptor hazard tracking
is not a substitute for these Metal 4 barriers.

On supported Apple GPUs, a resource can use memoryless storage only when its
whole-resource accesses are attachments inside one render pass, it has a writer,
it is not exported, and its actual color/depth/stencil operations neither load
previous contents nor store contents. Sampling, storage, transfers, partial
coverage and render-pass crossings reject this strategy. The allocator respects
the declared attachment operations; it never changes an explicit Store to
DontCare merely to make an allocation eligible.

Vulkan binds compatible alias images to the same device-memory range. Native
memory requirements must have a common memory type and no mandatory dedicated
allocation. An incompatible native requirement falls back to separate allocations.
Transient attachment usage and lazy memory selection require the same compiled
tile-local and discard-store facts used by Metal. An unsampled depth declaration
alone does not permit lazy storage.

`FrameGraph::set_transient_aliasing(false)` disables both physical aliasing and
memoryless selection before initialization. It creates independent allocations
without changing attachment operations or observable resource contents.

Diagnostics schema 13 labels pre-allocation slot totals as `compiler_projection`.
After native allocation, `native_frame_allocations` contains deterministic IDs,
frame-slot ownership, physical ranges, strategy and logical members. Runtime
memory totals use native allocation sizes across all allocated frame slots. Alias
savings are counted only for resources backed by the same native range. Memoryless
storage savings are reported separately from alias savings; these are storage
capacity estimates, not measured GPU bandwidth or performance improvements.

Graph mutations revalidate the native allocation contract before encoding.
Rebuilt groups prove that each existing shared range still has disjoint live
intervals, and native capability and tile-storage policies must match. A change
returns `AllocationContractChanged` without destroying resources. Complete the
GPU work, call `cleanup`, then initialize the changed graph again.
