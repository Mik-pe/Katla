# Capturing and Comparing Render-Graph Diagnostics

Render-graph diagnostics are a deterministic, backend-neutral snapshot of the
compiled graph: passes, dependencies, typed accesses, synchronization
transitions, culled passes, resource lifetimes, and physical transient
allocation slots. They are the artifact to attach when a rendering change
needs explaining, and the artifact CI uploads when a render-graph test fails.

The export contains no pointers, driver IDs, or file-system paths, so a capture
taken on one machine compares byte-for-byte with a capture on another.

## Formats

`RenderGraphDiagnostics` (`katla_gfx::render_graph`) exposes three exports:

| Export | Method | Use |
|--------|--------|-----|
| JSON | `to_json_pretty()` | Machine comparison; carries every field |
| Text | `Display` | Human review; passes, accesses, transitions, allocation slots |
| DOT | `to_dot()` | Graph view; logical resources, physical allocations, frame boundaries |

The `schema_version` field is bumped whenever a format changes. A capture and a
golden from different schema versions are not comparable.

## Capturing a frame locally

The application can dump the compiled graph once, right after a rendered frame —
the graph at that point reflects the live resource set:

```bash
# Text export to stdout
cargo run -p game --release -- --headless -s --dump-render-graph

# Text export to a file (keeps the log stream clean for diffing)
cargo run -p game --release -- --headless -s \
    --dump-render-graph-file target/render-graph-diagnostics/local.txt
```

In windowed mode the flags also cap the run at three frames, so the capture
finishes on its own; headless mode decides its own frame count (`-s` runs the
100-frame validation pass) and dumps after the loop. `--headless` only changes
where the frame is rendered, not the graph.

The dump is backend-neutral text, so the same command records the same graph on
Vulkan (Linux) and Metal (macOS). Backend-only fields such as Metal residency or
argument-table identity are not part of the export yet — see issue
[#37](https://github.com/Mik-pe/Katla/issues/37).

## Comparing two captures

Compare the captures directly; the order is deterministic and independent of
hash-map iteration.

```bash
cargo run -p game --release -- --headless -s --dump-render-graph-file /tmp/before.txt
# ... make the rendering change ...
cargo run -p game --release -- --headless -s --dump-render-graph-file /tmp/after.txt
diff /tmp/before.txt /tmp/after.txt
```

Read a diff as a claim about the compiler: a changed pass order, a new
synchronization transition, a changed allocation slot, or a resource moving
between live and culled. A diff that touches none of those but still changes
pixels points at the backend encoder path, not the graph.

## Golden snapshots

`katla_gfx/tests/goldens/` pins the canonical exports
(`render_graph_diagnostics.{json,text,dot}`) of a representative
shadow → geometry → lighting → present chain. The golden test runs in the
`cargo test -p katla_gfx --lib` CI step.

When the export format or the compiler changes intentionally, regenerate and
review the diff like code:

```bash
KATLA_BLESS_GOLDENS=1 cargo test -p katla_gfx --lib render_graph::diagnostics
```

A drifting golden *without* `KATLA_BLESS_GOLDENS=1` writes the actual export to
`target/render-graph-diagnostics/` and fails the test. CI uploads that directory
as an artifact when a job fails, so a failing run does not require re-running
locally to see what changed.

## Known gaps

Tracked on [#37](https://github.com/Mik-pe/Katla/issues/37):

- Backend encoder traces and a compiled-vs-emitted comparison report.
- Metal argument-table layout, residency-set membership, and commit-feedback
  identity.
- Frame-slot ownership on transient allocations.

These fields need the backend stages that produce them; the export deliberately
does not invent them.
