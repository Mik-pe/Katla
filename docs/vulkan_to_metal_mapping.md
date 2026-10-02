# Vulkan and Metal 4 mapping

Both backends consume Katla's compiled render-graph contracts. Native API differences stay below the public graph and renderer interfaces.

| Vulkan | Metal 4 |
| --- | --- |
| Queue | `MTL4CommandQueue` |
| Command pool | Slot-owned `MTL4CommandAllocator` |
| Command buffer | `MTL4CommandBuffer` begun with its allocator |
| Dynamic rendering attachments | `MTL4RenderPassDescriptor` attachments with declared load, store and clear operations |
| Graphics commands | `MTL4RenderCommandEncoder` |
| Compute dispatch | `MTL4ComputeCommandEncoder` |
| Copy, fill and texture upload | Transfer operations on the Metal 4 compute command encoder |
| Descriptor binding | Reflected `MTL4ArgumentTable` buffer addresses and resource IDs |
| Push constants | Immutable submission-owned inline buffer bound through the table |
| Bindless descriptor arrays | Immutable resource-ID buffer plus retained native residency snapshot |
| Buffer device address | `MTLBuffer.gpuAddress` |
| Synchronization2 producer and consumer scopes | Explicit queue-stage barriers and device visibility |
| Image layouts | No native Metal layout; compiled access scopes still require visibility ordering |
| Transient memory alias handoff | Placement heap reuse plus `ResourceAlias` visibility |
| Submission fence | Exact native commit-feedback completion object |
| Queue submit | Native commit with feedback handler |
| Presentation acquire and present semaphore chain | Queue wait for drawable, commit, queue signal drawable, drawable present |
| Pipeline cache | Prepared Metal 4 compiler archives keyed by canonical shader and state identity |

Metal 4 treats resources as untracked. Encoder order, unified memory, and tracked heap descriptors do not eliminate graph hazards. Imported resources, external uploads, multiple reader frontiers, and final output contracts all participate in the compiled synchronization plan. Native resources are also unretained by command buffers: Katla keeps their complete ownership payload through exact terminal feedback.

Metal uses three frame slots and Vulkan uses two; both expose their bounded slot count to application resource owners. CPU uploads occur after acquisition. App-owned animation, light culling and particle services use ordinary declared compute and transfer commands. Custom WGSL kernels follow the same reflection and execution path.
