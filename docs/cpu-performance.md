# CPU hot-path measurements

The CPU changes target column-major matrix products and TRS construction,
point transforms, transformed bounds, sparse ECS index pages, typed-query
membership rebuilding and keyed UI reconciliation.
These are portable CPU workloads; they do not measure editor FPS, GPU execution,
audio device latency, or native Metal acceptance.

## Reproduce

Run on an otherwise idle host with Rust and Python 3.12+ installed:

```sh
python3 scripts/compare_cpu_hotpaths.py --baseline-ref 8173c1c2 --sample-ms 50
```

The script exports the baseline into a temporary directory, copies the current
benchmark harnesses into it, and builds optimized baseline and working-tree
executables in **separate Cargo target directories**. It copies the executables
out before running, so subsequent builds cannot overwrite a sampled binary.
All compilation finishes before sampling. Two passes run in baseline/current,
then current/baseline order; each collects seven samples per scenario after
three warm-up iterations. The baseline must support the harness's public APIs.

Results live in [baseline.csv](benchmarks/cpu-hotpaths/baseline.csv),
[current.csv](benchmarks/cpu-hotpaths/current.csv) and
[environment.json](benchmarks/cpu-hotpaths/environment.json). Metadata records
the baseline commit, current source hashes, harness hashes, executable hashes,
toolchain, host and CSV hashes. Build outputs are reusable under
`target/cpu-comparison/{baseline,current}`.

Math samples process batches of 4,096 matrices, points, transforms or boxes. Whole
results pass through `black_box`, with a scalar checksum accumulated outside
each operation. Setup and allocation are outside sampling.
ECS uses the existing storage/churn harness, including
its reference assertions; see [workload definitions](ecs_benchmarks.md).

The `ecs_query_prepare` harness adds fresh public query construction, a complete
typed-system frame after an unrelated component replacement invalidates its
membership cache, and warmed no-op frames as a control. It covers two and four
components, dense and one-percent matching, at 1k/10k/100k entities. It checks
component sums and membership before sampling; the no-op system black-boxes its
prepared query length. Rebuild timings include ordinary lifecycle bookkeeping.

The `cpu_reconciliation` UI harness constructs descriptors and calls the public
`ViewTree::set_root` API at 8/128/1024/4096 siblings. Scenarios cover stable keys,
reversing and rotating order, append/truncate, mixed keyed/unkeyed rows, stable
duplicate keys and an unkeyed control. Node counts are checked every iteration.
It does not invoke layout, text shaping, painting or a GPU; these results isolate
descriptor construction and retained-node matching, not a complete editor frame.

## Recorded observations

Captured on 2026-10-15 against main commit `8173c1c2` on an AMD EPYC 9V45
Linux x86_64 virtual machine, Rust 1.99.0 / LLVM 23.1.1, using the workspace's
optimized bench profile. This is a shared host without CPU pinning. No custom
Rust flags or build target were set, and no Cargo configuration file was found
in the project, its parents or the activated Cargo home. Values are microseconds
per complete batch/frame, with median and observed min–max across 14 samples.
Ratios compare medians. The CSVs retain all 96 scenarios, including the unchanged
archetype prototype, warmed ECS frames and unkeyed UI lists as controls.

### Math and sparse storage

| Scenario | Rows/batch | Baseline µs (range) | Current µs (range) | Baseline/current |
| --- | ---: | ---: | ---: | ---: |
| `mat4_mul` | 4,096 | 119.013 (118.649–125.761) | 12.609 (12.488–14.917) | 9.44× |
| `mat4_vec4` | 4,096 | 31.530 (31.335–34.015) | 4.779 (4.433–5.463) | 6.60× |
| `mat4_point` | 4,096 | 6.955 (6.797–8.477) | 4.420 (4.395–4.728) | 1.57× |
| `transform_matrix` | 4,096 | 247.139 (246.432–249.883) | 13.802 (13.749–20.408) | 17.91× |
| `aabb_transform` | 4,096 | 39.088 (38.960–41.996) | 8.108 (7.988–10.768) | 4.82× |
| `sparse_query2_dense` | 100,000 | 196.441 (193.198–197.853) | 179.935 (176.006–194.599) | 1.09× |
| `sparse_query4_dense` | 100,000 | 365.084 (337.301–391.860) | 321.106 (317.518–339.528) | 1.14× |
| `sparse_query2_one_percent` | 100,000 | 197.460 (194.791–215.636) | 179.362 (176.564–183.862) | 1.10× |
| `sparse_query4_one_percent` | 100,000 | 6.300 (6.189–6.350) | 4.921 (4.663–5.187) | 1.28× |
| `sparse_churn_tag_roundtrip` | 100,000 | 5005.202 (4785.841–5259.514) | 4398.251 (4333.487–4485.007) | 1.14× |
| `sparse_churn_wide_roundtrip` | 100,000 | 5089.704 (4871.629–9368.733) | 4562.372 (4467.193–4735.096) | 1.12× |

Point transforms take 36% less time in this capture. The matrix/TRS, transformed
bounds and compact sparse-page improvements remain in the cumulative change;
these observations do not predict an engine-wide frame-rate ratio.

### Typed-query preparation

| Scenario | Rows/batch | Baseline µs (range) | Current µs (range) | Baseline/current |
| --- | ---: | ---: | ---: | ---: |
| `typed_fresh_query2_dense` | 100,000 | 3059.584 (2926.847–3210.769) | 1158.700 (1131.879–1175.284) | 2.64× |
| `typed_rebuild_frame_query2_dense` | 100,000 | 2614.306 (2539.535–2735.886) | 736.768 (703.733–771.200) | 3.55× |
| `typed_steady_frame_query2_dense` | 100,000 | 0.089 (0.089–0.096) | 0.083 (0.082–0.087) | 1.07× |
| `typed_fresh_query4_dense` | 100,000 | 4608.950 (4213.725–6573.831) | 1082.154 (945.140–1143.074) | 4.26× |
| `typed_rebuild_frame_query4_dense` | 100,000 | 4604.841 (4362.813–4770.030) | 1105.322 (967.222–1169.060) | 4.17× |
| `typed_steady_frame_query4_dense` | 100,000 | 0.114 (0.114–0.115) | 0.104 (0.103–0.104) | 1.10× |
| `typed_fresh_query4_one_percent` | 100,000 | 62.475 (61.454–65.454) | 11.128 (10.796–11.784) | 5.61× |
| `typed_rebuild_frame_query4_one_percent` | 100,000 | 64.570 (63.231–65.814) | 11.422 (11.171–12.458) | 5.65× |
| `typed_steady_frame_query4_one_percent` | 100,000 | 0.115 (0.114–0.120) | 0.104 (0.103–0.110) | 1.10× |

At 100k entities, cache-rebuilding frames are 3.5–5.7× faster. Column-wise
membership filtering removes repeated registry lookups and the driver's redundant
membership check. Warmed no-op controls remain around 0.1 µs; their nanosecond
variation does not establish a steady-frame speedup.

### Keyed UI lists

| Scenario | Rows/batch | Baseline µs (range) | Current µs (range) | Baseline/current |
| --- | ---: | ---: | ---: | ---: |
| `ui_keyed_stable` | 4,096 | 2341.928 (2296.612–2415.907) | 161.798 (158.727–164.612) | 14.47× |
| `ui_keyed_reverse` | 4,096 | 4136.462 (4027.176–4556.093) | 245.873 (243.843–251.761) | 16.82× |
| `ui_keyed_rotate` | 4,096 | 2316.548 (2301.860–2494.224) | 249.661 (246.007–264.833) | 9.28× |
| `ui_keyed_insert_remove` | 4,096 | 2332.630 (2325.629–2388.472) | 162.531 (161.915–164.145) | 14.35× |
| `ui_keyed_mixed` | 4,096 | 2332.168 (2316.285–4250.188) | 164.248 (161.740–165.610) | 14.20× |
| `ui_keyed_duplicates` | 4,096 | 2307.536 (2297.448–2341.442) | 164.352 (161.257–170.314) | 14.04× |
| `ui_unkeyed_stable` | 4,096 | 154.274 (152.963–155.855) | 158.095 (157.182–162.067) | 0.98× |

Matching 4,096 keyed siblings is 9–17× faster, including descriptor construction.
Unchanged key order skips match-index allocation. Reordering uses one key index,
duplicate links and a forward positional fallback instead of nested scans.
Unkeyed lists retain their original index path; no speedup is claimed for them.

Small-list controls use the same public API:

| Scenario | Rows/batch | Baseline µs (range) | Current µs (range) | Baseline/current |
| --- | ---: | ---: | ---: | ---: |
| `ui_keyed_stable` | 8 | 0.285 (0.283–0.290) | 0.240 (0.234–0.353) | 1.19× |
| `ui_keyed_reverse` | 8 | 0.296 (0.291–0.449) | 0.303 (0.301–0.316) | 0.97× |
| `ui_keyed_rotate` | 8 | 0.290 (0.287–0.297) | 0.311 (0.305–0.334) | 0.93× |
| `ui_keyed_insert_remove` | 8 | 0.336 (0.332–0.355) | 0.255 (0.252–0.264) | 1.32× |
| `ui_keyed_mixed` | 8 | 0.288 (0.286–0.293) | 0.235 (0.232–0.238) | 1.23× |
| `ui_keyed_duplicates` | 8 | 0.291 (0.285–0.336) | 0.236 (0.233–0.252) | 1.23× |
| `ui_unkeyed_stable` | 8 | 0.230 (0.228–0.243) | 0.244 (0.227–0.247) | 0.94× |

The bounded inline index avoids allocating and hashing a map for up to 16 old
siblings. Eight-row reverse/rotate cases are still 7–21 ns slower in this capture,
while stable keyed order and append/truncate are faster. Keep this small absolute
tradeoff and shared-host spread visible rather than extrapolating the large-list
ratio to every widget update.

## Correctness and limits

Run the affected crates' complete portable suites and strict Clippy:

```sh
cargo check -p katla_math -p katla_ecs -p katla_ui --all-targets --all-features --locked
cargo test -p katla_math -p katla_ecs -p katla_ui --all-features --locked
cargo clippy -p katla_math -p katla_ecs -p katla_ui --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
```

Regressions compare general matrix/vector products and TRS against independent
scalar composition, including reflections and zero scales. Transformed bounds
are checked against all eight corners with shear and collapsed axes. Existing
generational sparse-set tests cover stale IDs, page boundaries and swap removal;
an added size assertion verifies one word per sparse entry.
The typed-query regression reuses a multi-column cache across changes to its
driver, join and exclusion filter, checking order, values and dirty IDs. UI
regressions check node identity and state through duplicate-key reorder,
key-prioritized positional fallback, append/truncate and removal.
Both the inline key index and hash index are checked at their 16/17-sibling
boundary, including reordered duplicates and node state.

Sparse pages use 8 KiB instead of 16 KiB per allocated 1,024-entry page on
64-bit targets. This halves **sparse entry storage**, not total ECS memory:
the page directory, components, dirty tracking and query caches are additional.

Small floating-point rounding differences are expected when algebraically
equivalent operations are regrouped. CPU timing depends on compiler, target,
frequency and shared-host scheduling; no affinity or frequency pinning is used.
Validate application frame times and physical Apple Silicon separately before
making engine-wide or Metal performance claims.
