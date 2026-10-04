# Native material pipeline reuse

This is historical Rust acceptance at `8b76a167`. It records that source and
its validation scope; current application contracts and commands are in
[the Odin build guide](../odin_build.md) and [editor contract](../odin_editor.md).

The baseline is commit `4d3a6f8f`, before native pipeline sharing. Both implementations
ran the same `katla_gfx/benches/material_instances.rs` on Intel Iris Plus Graphics
(ICL GT2), Vulkan/Mesa, Rust's release profile. Each run creates a fresh renderer,
registers 32 independent materials, warms six frames, draws all 32 through explicit
phases for 100 frames, edits WGSL and waits for green native readback pixels.

Five alternating sequential runs per implementation produced these medians.
Builds and other native acceptance runs had completed before this sample. Raw
results and allocation counts are in [measurements.json](measurements.json).

| Operation | Previous time | Shared time | Previous Rust allocations | Shared Rust allocations |
| --- | ---: | ---: | ---: | ---: |
| First registration | 395 µs | 398 µs | 729 | 736 |
| Following 31 registrations | 2,367 µs | 294 µs | 6,897 | 1,222 |
| 100 warmed frames | 44,637 µs | 46,047 µs | 95,800 | 95,800 |
| Reload 32 materials to green pixels | 4,225 µs | 2,444 µs | 13,902 | 8,235 |

Warm registration and reload allocate less and complete faster in this fixture.
First registration changes little. Frame allocations are unchanged at 958 per
frame; the timings do not establish a frame-rate improvement. The separate native
regression observes 32 native Vulkan pipelines before sharing and one afterward.

These are material/pipeline costs for a small custom shader, not full model import
or full-scene performance. First registration starts with an empty renderer cache;
it does not erase the driver shader cache. Allocation counts include process-wide
Rust allocation/reallocation calls, including background workers, and exclude
allocations inside the driver and Objective-C runtime. Timing disables API
validation. Native correctness runs separately with Vulkan validation enabled.

```bash
cargo bench -p katla_gfx --bench material_instances
cargo test -p katla_gfx --all-features --lib render_graph::native_compute_tests::material_reloads -- --nocapture
```

For a baseline comparison, make a detached worktree at `4d3a6f8f`, copy the benchmark
and its Cargo benchmark declaration into it, build both implementations, then run
the two executables sequentially. Preserve each implementation's shader sources.

The native regressions cover exact sharing, distinct render state and changed
source, independent material texture sets, submitted frames across reload and
material removal, weak cache retirement, and shared plain/instanced UI preparation.
Real static/skinned glTF surface probes still verify independent texture/factor
pixels and resource retirement after adopting sharing.

Metal uses the same exact source/state key, retains shared pipelines on material
variants and synchronizes cache lookup/build between its reload workers. The
benchmark selects Metal on macOS and installs a headless drawable. macOS cross
checks pass, but physical Metal 4 hardware was unavailable for this study. Run
with `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` for native correctness acceptance.
