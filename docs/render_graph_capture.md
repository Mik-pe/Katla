# Capturing and comparing Odin render graphs

The passive recording joins the compiled graph with facts observed at actual native
allocation, encoding, synchronization and binding sites. Enabling capture does not
introduce a GPU wait or change scheduling. Enable it before the submission to inspect.

```odin
metal.capture_enable(&renderer, true)
// Submit the ordinary application graph.
capture, found := metal.capture_snapshot(&renderer, submission.id)
if found {
    defer gfx.capture_snapshot_destroy(&capture)
    findings := gfx.capture_compare(&capture)
    defer delete(findings)
    text := gfx.capture_text(&capture)
    defer delete(text)
    fmt.print(text)
}
```

Vulkan exposes the same capture API. A snapshot owns its data independently of the
graph, renderer, resource handles and reusable frame slot. Its submission ID, slot
and acquisition generation identify exact accepted work. Actual completion owners
update Pending/Completed/Failed feedback; obtaining a snapshot never waits for GPU
completion. Rejected recordings preserve previous accepted captures. Each renderer
retains the latest 16 submissions, while caller-owned clones have independent lives.

## Editor exports

The canonical editor enables capture for its dump flags and exports the first
accepted aggregate submission. The graph includes actual scene, lighting, particles,
postprocessing, picking and UI composition.

```sh
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
python3 scripts/run_katla_odin.py -- --headless --frames 2 \
  --dump-render-graph-file /tmp/metal-capture.json

python3 scripts/run_katla_odin.py --backend vulkan -- --headless --frames 2 \
  --dump-render-graph-file /tmp/vulkan-capture.dot
```

`.json` exports the complete `{capture, comparison}` bundle. `.dot` exports graph
and physical allocation/native encoder nodes. Other extensions select text;
`--dump-render-graph` prints text. Backend library/ICD arguments are described in
[build and offline validation](odin_build.md).

## Owned schema

Schema 1 is the Odin contract. Passes retain their declaration names, kinds, access
ranges, actual live order and liveness reasons: side-effect root, exported producer,
required predecessor or not required. Resources retain descriptors, import/export
contracts and actual first/last live positions. Dependencies identify exact producer
and consumer pass/access indices and their byte or mip/layer/aspect ranges. These
indices describe the actual compiler, rather than inventing a second scheduler.

Native events retain physical allocation size, heap placement and actual native
memory choices, encoded command ownership and logical pass spans, pipeline/binding
and argument table/layout identities, real residency membership, attachment
load/store/clear operations and exact emitted synchronization scopes. Vulkan records
one actual command buffer and distinct logical pass spans. Metal records actual
native encoders and its global queue-stage visibility barriers; those barriers are
not fabricated into per-range native calls. Alias events join compiler handoffs
with the real native ownership ordering. Auxiliary allocations have explicit
resource kind and negative logical indices.

Stable encounter ordinals identify native objects only within a recording. Exports
contain no native handles, pointers, GPU addresses or private filesystem paths.
Physical facts can differ across backends; allocation byte counts are observations,
not bandwidth measurements or inferred savings.

`gfx.capture_graph_snapshot` provides an owned compiler projection before any GPU
execution. It reports `native_captured=false` and backend None. Accepted native
captures report `native_captured=true`; pure compiler snapshots cannot claim native
completion or trigger missing-native-execution comparisons.

## Comparison and validation

`gfx.capture_compare` returns typed findings with concrete pass, resource, expected
and observed event indices. It checks live pass order and scope, encoder ownership,
resource declaration/range coverage, pipeline/table/residency scopes and independently
translated expected synchronization. Missing, duplicate, unexpected and changed
native scopes/ranges remain distinct findings. Direct vertex/index/indirect bindings
have explicit paths and do not pretend to use descriptor tables.

Pure CPU tests inject changed scope/range, missing/duplicate observations, unknown
resources, undeclared bindings and stale table/residency identities. Ownership tests
mutate declarations, reject candidates, reuse slots and evict recordings while a
cloned snapshot remains valid. Native backend tests exercise the actual instrumentation
with API validation and allocation tracking. Rendering validation follows
[the gfx contracts](gfx_odin.md). The former Rust schema and its delivery records
remain [historical evidence](archive/rust-render_graph_capture.md).
