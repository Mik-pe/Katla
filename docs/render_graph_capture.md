# Capturing and Comparing Render-Graph Diagnostics

The passive capture joins a compiled graph, physical allocations, pass execution,
and native encoder/synchronization/binding observations. It neither waits for GPU
completion nor changes scheduling, synchronization, allocation, or encoding.
Enable tracing before rendering the frame you want to inspect.

```rust
use katla_gfx::GpuRenderer;

graph.set_execution_trace(true);
renderer.render(&token, &mut graph, |frame| { /* submissions */ })?;
renderer.present(token)?;
let capture = graph.capture()?;
println!("{capture}");
for divergence in &capture.comparison {
    eprintln!("{divergence}");
}
```

The stored submission snapshot describes the state observed during encoding. Its
feedback is normally `pending` before `present`. A caller may replace
`capture.backend_execution.frame` with the renderer's passive
`capture_submission_snapshot()` after presentation to observe newer feedback.
`pending`, `completed`, and `failed` are explicit states; capture never forces one
by waiting. Feedback identity combines the reusable frame slot and its acquisition
generation. The command allocator ordinal identifies its bounded frame owner.

## Local Vulkan and Metal capture

The application dump flags enable tracing and export the joined frame. Vulkan
runs on Linux; Metal runs on macOS with a Metal 4 capable physical GPU. Both use
the same application graph. Headless mode replaces the surface with offscreen
outputs and retains the selected application graph/runtime.

```bash
# Vulkan: Linux with an available Vulkan driver.
cargo run -p game --release -- --headless -s \
    --dump-render-graph-file /tmp/vulkan-capture.json

# Metal: enable native API validation before process launch.
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
cargo run -p game --release -- --headless -s \
    --dump-render-graph-file /tmp/metal-capture.json
```

A `.json` destination selects the machine-readable bundle; `.dot` selects Graphviz,
and other extensions select text. `--dump-render-graph` prints text. Each native
encoder is recorded at the actual encoder opening, and each synchronization
observation at the native translation/emission site. An operation that needs no
native barrier carries `emitted: false` and its explicit backend reason. Extra
backend ownership barriers are labelled separately from compiler operations.

The comparison checks live/unknown passes, encoded pass order, color/depth targets,
attachment operations, native encoder/pass order, undeclared bound resources and
exact range-specific synchronization coverage and native stage/access/layout scopes against the canonical backend translator. Missing, duplicate, changed or
unexpected synchronization produces a concrete failure. `SyncScopeMismatch` names required and observed native scopes; every emitted subresource barrier retains its actual range. A skipped empty pass is
recorded, and does not count as an emitted encoder. Auxiliary native encoders can
have no graph pass; their stable labels and positions remain visible.

## Schema and formats

Schema 13 adds the joined capture and explicit liveness reasons. `graph` retains
the standalone `RenderGraphDiagnostics` snapshot, including typed image accesses
(aspects, mip/layer ranges, access mode, usage and stage) and typed buffer accesses
(byte ranges, usage and stage). Every compiled image/buffer transition includes
producer/consumer access, source/destination state, resource version, hazard reason
and encoder/queue boundary. A version identifies the preceding range-specific
access frontier (`rN.access.P`) or imported/undefined initial state (`rN.initial`);
it is diagnostic identity, not an additional scheduling mechanism.

Resources report live/cull state and deterministic cull reasons. Passes identify
side-effect roots, exported producers, required predecessors or unreachable work.
Logical transient lifetimes and compatibility classes join physical allocation
ordinals, frame slots, alias predecessor/successor order and saved bytes. Native
records distinguish actual storage choices from compiler estimates. Tile-local
storage requires whole-resource attachment use and discard stores throughout the
live lifetime; exporting, sampling, storage or transfers disqualify it. Bandwidth
savings remain unknown without hardware counters.

Metal bindings include stable argument-table identity, reflected layout identity,
immutable snapshot generation and residency membership. IDs follow stable
encounter order. No native object pointers, driver handles, GPU addresses or user
paths are exported. Native counts, storage decisions and observed feedback may
legitimately differ across backends or capture times; the compiler contract is the
portable comparison target. Deterministic ordering does not imply identical
hardware facts.

| Export | Method | Meaning |
|--------|--------|---------|
| JSON | `capture.to_json_pretty()` | Complete joined machine-readable bundle |
| Text | `Display` | Logical/physical plan plus native execution and divergences |
| DOT | `capture.to_dot()` | Logical resource ellipses, physical allocation nodes, native encoder hexagons |

`graph.diagnostics()` remains useful before any GPU allocation or frame execution.
It exports the pure compiler projection; it does not claim native execution.
Capture with tracing disabled explicitly reports that native execution was not
captured.

## Comparison and failure artifacts

```bash
diff -u /tmp/before.json /tmp/after.json
dot -Tsvg /tmp/metal-capture.dot -o /tmp/metal-capture.svg
```

Read compiler changes separately from native changes. A changed access, lifetime,
order, synchronization requirement or physical slot belongs to the graph/compiler
contract. A changed native observation with an unchanged contract belongs to its
backend translation. The `comparison` list records those contract violations.
Different schema versions require an intentional format migration.

`katla_gfx/tests/goldens/` covers both standalone graph exports and a representative
joined capture with render/compute/blit encoder kinds, frame ownership, feedback,
argument-table layout, residency, and buffer synchronization. The fixture is a
serialization/contract test; native tests separately exercise actual GPU encoding.
Focused tests inject order/resource/synchronization divergence, duplicate barriers,
missing coverage and unexplained no-ops.

```bash
cargo test -p katla_gfx --lib render_graph::diagnostics
KATLA_BLESS_GOLDENS=1 cargo test -p katla_gfx --lib render_graph::diagnostics
```

Review blessed JSON/text/DOT diffs like code. Failed golden checks write actual
exports under `$CARGO_TARGET_DIR/render-graph-diagnostics` (the workspace `target`
directory when unset). CI uploads graph/plan/execution artifacts on failures with
read-only repository permissions. Native validation failures can write their
joined capture to the same directory, preserving both the compiled contract and
actual observations for review.
