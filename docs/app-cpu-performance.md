# App CPU measurements

These comparisons isolate editor hierarchy construction, world-transform
resolution and audio metadata probing. The baseline is `cf15fc23`, which already
contains the math, ECS and keyed-reconciliation changes measured in
[the earlier CPU report](cpu-performance.md). Ratios here measure the additional
app changes; they must not be multiplied into an editor FPS prediction.

## Reproduce

On an otherwise idle Linux host with Rust, Python 3.12+ and the app's native
build dependencies installed:

```sh
python3 scripts/compare_app_cpu.py --baseline-ref cf15fc23 --sample-ms 50
```

The script exports the baseline, copies identical test harnesses and injects
only their test-module includes. It builds both app test executables in separate
Cargo target directories, copies the binaries out, then samples without any
concurrent compilation. Both builds use the optimized bench profile with LTO
disabled, 16 codegen units, debug information disabled and incremental compilation
disabled. These overrides reduce build cost and are recorded in the metadata;
the earlier report uses its own recorded profile configuration.

Two passes run baseline/current, then current/baseline. Each case has three
warm-up operations and seven samples per pass, for 14 observations. Each sample
runs complete operations for at least 50 ms. Results are elapsed time per
operation, including allocations. The runner requires all 33 cases in both
passes, positive timings and iteration counts, and equal observable checksums
per operation across implementations.

[Baseline CSV](benchmarks/app-cpu/baseline.csv),
[current CSV](benchmarks/app-cpu/current.csv) and
[environment metadata](benchmarks/app-cpu/environment.json) retain every sample,
toolchain, host, profile overrides, baseline injections, app source hashes,
harness hashes, manifest hashes, executable hashes and CSV hashes. Dependencies
outside the app are unchanged from the baseline commit. The CSV `matches` column
records input rows, including audio cases; it does not count metadata probes.

## Workloads and boundaries

The transform harness calls the actual `resolve_world_transforms` API on flat,
broad, deep and mixed-depth worlds at 1k, 10k and 100k entities. The mixed case
combines a half-world chain with independent roots. Every operation constructs
the output map and sums world positions; known expected positions are checked
before sampling. The optimized resolver borrows components instead of copying
transforms into its local index, reuses a traversal stack, and tracks visits in
that index instead of a separate hash set. It still reads fresh components on
every call and honors physics world-pose overrides.

The hierarchy harness repeatedly calls `ViewTree::build_from(&HierarchyView)`
at 128, 1,024 and 4,096 rows. It includes snapshot cloning, descriptor construction,
retained-node reconciliation and node counting. Cases cover flat roots, expanded
and collapsed chains, search through a collapsed chain, and a chain visited
deepest-first followed by independent roots. Node counts must remain stable.
Layout, font shaping, painting and GPU submission are outside this measurement.
The optimization resolves ancestry and filtered child membership once per build
and avoids cloning names into counting maps. It retains existing row order,
selection and collapsed-widget behavior. Invalid parent cycles now terminate
instead of leaving the UI in an unbounded ancestry walk.

The audio harness uses two real, generated one-second PCM WAV files, with
44.1 kHz mono and 48 kHz stereo metadata, across 1, 128 and 1,024 sources. Cases
select no source or one source. The baseline explicitly uses the previous
unconditional metadata-reader adapter (`KATLA_EDITOR_AUDIO_REFERENCE=1`); the
current case calls the production `selected_audio_metadata` helper. Both consume
the same selected inspector values and assert reader call counts on every
operation. Baseline performs N metadata probes, reopening the fixture files;
current performs zero or one. Loops black-box
each path, and sampling includes real filesystem reads with a warm page cache.
This isolates metadata probing, rather than the complete entity collector,
audio playback or a whole frame. Selected files are reread on each snapshot so
edits, replacement and error recovery remain fresh.

These are portable CPU measurements on a shared host without CPU pinning.
Native Vulkan/Metal execution and end-to-end editor frame rate were not measured.

## Recorded observations

Captured on 2026-10-15 on an AMD EPYC 9V45 Linux x86_64 virtual machine,
Rust 1.99.0 / LLVM 23.1.1. Values are microseconds per complete operation,
with median and observed min–max across 14 samples. Ratios compare medians.
No custom Rust flags or build target were set.

### World-transform resolution

| Scenario | Rows | Baseline µs (range) | Current µs (range) | Baseline/current |
| --- | ---: | ---: | ---: | ---: |
| `transform_resolve_flat` | 1,000 | 126.425 (125.447–130.483) | 103.295 (101.918–223.192) | 1.22× |
| `transform_resolve_broad` | 1,000 | 216.269 (214.821–230.269) | 180.464 (178.821–293.014) | 1.20× |
| `transform_resolve_deep` | 1,000 | 190.536 (189.821–195.340) | 161.602 (160.066–166.396) | 1.18× |
| `transform_resolve_mixed_depth` | 1,000 | 170.100 (169.257–172.578) | 139.691 (138.635–142.625) | 1.22× |
| `transform_resolve_flat` | 10,000 | 1239.608 (1229.090–1301.666) | 1072.192 (1059.996–1139.092) | 1.16× |
| `transform_resolve_broad` | 10,000 | 2171.818 (2156.932–2187.445) | 1865.486 (1848.624–1958.561) | 1.16× |
| `transform_resolve_deep` | 10,000 | 2046.704 (2023.539–2073.482) | 1710.649 (1677.973–1924.578) | 1.20× |
| `transform_resolve_mixed_depth` | 10,000 | 1768.518 (1732.952–1900.417) | 1440.645 (1430.128–1456.611) | 1.23× |
| `transform_resolve_flat` | 100,000 | 26045.001 (23121.010–28023.592) | 16507.841 (15631.928–23539.087) | 1.58× |
| `transform_resolve_broad` | 100,000 | 39645.555 (36622.649–43720.363) | 31635.394 (27377.060–36062.560) | 1.25× |
| `transform_resolve_deep` | 100,000 | 49620.670 (41323.859–69278.149) | 30020.551 (26880.989–38147.927) | 1.65× |
| `transform_resolve_mixed_depth` | 100,000 | 39386.192 (34247.075–44638.226) | 25432.079 (22708.838–38923.223) | 1.55× |

At 100k entities the resolver is 1.25–1.65× faster across these shapes.
The smaller 1k/10k cases improve by 1.16–1.23×. This includes rebuilding
the local/parent indexes and producing the complete pose map.

### Hierarchy view construction

| Scenario | Rows | Baseline µs (range) | Current µs (range) | Baseline/current |
| --- | ---: | ---: | ---: | ---: |
| `hierarchy_flat` | 128 | 92.117 (90.278–95.145) | 77.557 (76.553–82.866) | 1.19× |
| `hierarchy_chain_expanded` | 128 | 546.478 (535.733–571.303) | 91.171 (90.440–92.953) | 5.99× |
| `hierarchy_chain_collapsed` | 128 | 181.502 (178.673–202.277) | 90.599 (88.911–95.576) | 2.00× |
| `hierarchy_chain_search` | 128 | 179.773 (178.062–188.982) | 93.243 (92.155–104.789) | 1.93× |
| `hierarchy_deep_then_roots` | 128 | 205.415 (202.836–216.812) | 91.664 (90.654–97.161) | 2.24× |
| `hierarchy_flat` | 1,024 | 1152.137 (1131.360–1208.821) | 628.158 (616.492–687.074) | 1.83× |
| `hierarchy_chain_expanded` | 1,024 | 31246.588 (30985.965–31748.388) | 734.702 (726.547–961.545) | 42.53× |
| `hierarchy_chain_collapsed` | 1,024 | 7438.158 (7381.526–7859.447) | 719.531 (712.077–843.146) | 10.34× |
| `hierarchy_chain_search` | 1,024 | 7453.640 (7409.427–7621.062) | 737.210 (726.125–774.440) | 10.11× |
| `hierarchy_deep_then_roots` | 1,024 | 8389.233 (8304.521–8504.211) | 724.692 (714.692–752.060) | 11.58× |
| `hierarchy_flat` | 4,096 | 10433.977 (9994.837–11002.385) | 2635.295 (2516.086–2822.619) | 3.96× |
| `hierarchy_chain_expanded` | 4,096 | 511772.954 (505030.819–563629.454) | 3130.347 (3002.356–3266.302) | 163.49× |
| `hierarchy_chain_collapsed` | 4,096 | 118823.876 (118131.286–119504.863) | 3048.157 (2873.386–3285.114) | 38.98× |
| `hierarchy_chain_search` | 4,096 | 119104.575 (117846.018–129115.681) | 3078.308 (2975.789–3656.684) | 38.69× |
| `hierarchy_deep_then_roots` | 4,096 | 134647.549 (133679.594–141501.072) | 3039.773 (2919.855–3184.905) | 44.30× |

At 4,096 rows, flat view construction falls from 10.434 to 2.635 ms.
An expanded chain falls from 511.773 to 3.130 ms, a 163× ratio for that
particularly expensive ancestry shape. Collapsed/search chains and mixed
deep/root lists also improve; these figures include view construction and
reconciliation, but exclude layout and drawing.

### Audio metadata probing

| Scenario | Rows | Baseline µs (range) | Current µs (range) | Baseline/current |
| --- | ---: | ---: | ---: | ---: |
| `editor_audio_metadata_none` | 1 | 0.812 (0.805–1.883) | 0.024 (0.024–0.025) | 33.21× |
| `editor_audio_metadata_selected` | 1 | 0.814 (0.803–0.853) | 0.822 (0.814–0.842) | 0.99× |
| `editor_audio_metadata_none` | 128 | 102.341 (101.570–201.187) | 0.207 (0.199–0.217) | 493.30× |
| `editor_audio_metadata_selected` | 128 | 101.856 (101.203–110.975) | 1.039 (1.015–1.094) | 98.02× |
| `editor_audio_metadata_none` | 1,024 | 809.419 (800.828–823.272) | 1.478 (1.445–1.515) | 547.71× |
| `editor_audio_metadata_selected` | 1,024 | 808.865 (803.997–857.138) | 2.252 (2.237–2.268) | 359.13× |

With 1,024 sources and one selected entity, probing falls from 808.865
to 2.252 µs, with file reads reduced from 1,024 to one per snapshot. With
no selection there are zero reads. The one-source selected control still
performs one read in both versions and is about 8 ns slower in this capture;
no improvement is claimed for it. These are warm-file metadata adapter
timings, not complete collector or audio-playback timings.

## Correctness checks

```sh
cargo fmt --all -- --check
cargo test -p katla_app -p katla_agent --lib --all-features --locked
cargo check -p katla_app --all-targets --all-features --locked
cargo check -p katla_app --no-default-features --all-targets --locked
cargo clippy -p katla_app --all-targets --all-features --locked -- -D warnings
```

Regression tests cover current transforms after unmarked edits, physics pose
overrides, independent roots and cycles, deep and filtered hierarchy rows,
selection scrolling, duplicate-name numbering, entity generations, metadata
reader counts, and actual WAV deletion/replacement/recovery. Native GPU fixtures
remain outside this CPU evidence.
