# Metal 4 frame ownership

There are exactly three frame slots. Each submission has a slot and monotonically increasing generation. A completion for one generation never releases another generation's resources. Slot reuse waits only when that slot remains busy; there is no unconditional previous-frame wait.

| Mutable owner | Storage and lifetime |
| --- | --- |
| Command allocator | One per slot; reset after every consuming command buffer completes |
| Native command buffer | Slot retains the complete wrapper through terminal feedback |
| Frame uniforms and object arrays | Independent buffers per slot, written after acquisition |
| Skeleton matrices | Independent slot buffers; replacement buffers retained by consuming submissions |
| Animation parameters, static animation arrays, world and output matrices | Three slot allocations; static publication replaces buffers; parameters update the acquired slot |
| Light data, tile indices, tile counts and frame data | Three slot allocations; declared graph transfer and compute commands order writes and reads |
| Particle counters, indirect commands, frame data and emitter configurations | Three slot allocations; graph rollover copies previous counters into the current slot |
| Particle pool and index lists | Shared GPU state; declared byte slices and explicit queue visibility order compute and draw consumers |
| Shadow cascade and encode data | Three slot buffers |
| UI vertices and indices | Three renderer owners; each upload publishes fresh buffers retained by its submission |
| Dynamic meshes | Immutable replacement publication; previous buffers remain owned by consuming submissions |
| Transient images and buffers | Three allocation sets; alias members use explicit stage and alias visibility handoffs |
| Bindless resources | Immutable resource-ID buffer and native residency snapshot per published frame variant |
| Persistent buffers | Stable immutable owner residency snapshots, retained by submissions |
| Upload staging | Bounded service batches keyed by exact submission generation |
| Timestamp queries | One native Metal 4 counter heap per slot; opted-in frame spans resolve only after their exact completion, before reuse |
| Picking readback | Request owns the last committed graph attachment, its slot/generation, destination and command buffer until exact completion |
| Argument tables, inline constants, samplers and pipeline states | Retained by the command buffer's encoding-resource payload |

Acquisition retires the reused slot's terminal completion, then resets its allocator and permits CPU writes. The command buffer begins with that allocator and attaches its residency sets. Each encoder retains its native resources, bindings, and pipeline state. Reflected binding failures reject the whole encoding before commit.

The commit callback copies only feedback timing and native error data into a synchronized completion object. It does not capture thread-affine resources. Successful completion retires upload staging; terminal GPU failure also retires staging and returns the native error. The slot owner releases all submission resources on the encoding thread after feedback.

Unsubmitted errors and aborts drop encoded resources before replaying their prepared upload batches. A frame token can record once. Rendering retains an unsubmitted command buffer; abort drops it and publishes no GPU state. Presentation commits that recorded buffer, publishes its output and buffer-history state, and advances the slot without waiting. Explicit screenshots wait for the latest exact submission; shutdown drains all slots before destruction.

Resize replaces size-dependent resources without invalidating retained submissions. A newly allocated output has undefined contents: a declared LOAD requires previously defined contents, while CLEAR establishes them. Output state advances only after a successful commit, and discarded contents remain undefined.

Physical buffer history records byte ranges by native allocation identity across graphs and frames. Allocation retirement waits for every consuming slot completion before removing that allocation's history. A full drain clears history only when all submissions completed successfully.

Profiling is opt-in: a label opened with `begin_timestamp` around `render` measures that native command buffer's GPU work, and `end_timestamp` closes the label for subsequent renders. A frame with no open labels emits no counter commands. Results use the native timestamp heap and Mach timebase, are polled without a wait, and remain cached after the completed slot is reused. Each slot's heap stays owned through feedback.

Object-ID draws bind the complete object array and use the draw's global object slot as native base instance. The GPU ID is the global slot plus one, including every instance in an instanced draw. Picking never reads a standalone texture: rendering stages the graph attachment, presentation publishes it, and aborted frames leave the previous submitted source intact.
