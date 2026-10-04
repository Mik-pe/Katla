# Metal binding and residency ownership in Odin

The offline WGSL compiler emits per-entry reflection and a canonical Metal binding
map. Buffer, texture and sampler namespaces remain distinct. Native preflight
checks each reflected resource and minimum byte range before opening an encoder.
Only reachable entry-point bindings are required. Vertex attribute bindings and
runtime-array size buffers use their reflected native slots.

A submitted frame retains immutable argument-table owners, required shader views,
inline constant uploads, runtime-size uploads and all native resources those
bindings reference. Residency contains real graph allocations and their owning
heaps, including auxiliary native views. Removing a public handle cannot retire a
submitted binding. Pipeline and sampler handles share the same exact completion
rule. A slot becomes reusable only when its own acquisition generation completes.

Image and sampler overrides belong to explicit render phases. Missing overrides
restore packet defaults; changes never mutate a previously accepted descriptor.
Fixed arrays support actual sampled/storage bindings, including 4096 material
slots, with real native output and ownership validation.

Passive [capture diagnostics](render_graph_capture.md) join immutable argument
table/layout identities and observed residency membership with the actual frame.
Object identifiers are recording-local encounter ordinals; exported data contains
no object pointers or GPU addresses. The former Rust snapshot implementation and
its benchmarks remain [historical evidence](archive/rust-metal-binding-residency.md).
