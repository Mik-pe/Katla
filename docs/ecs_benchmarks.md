# ECS storage and scheduling measurements

The production ECS keeps one sparse-set component store. The benchmark-only
archetype implementation evaluates a storage alternative; it is not a second
runtime implementation. Its contiguous iteration establishes a useful storage
ceiling, while its wide-row migration shows why replacing every sparse set is
not automatically faster.

## Reproduce

Run on an otherwise idle macOS host with Rust and Python 3 installed:

```sh
python3 katla_ecs/benches/run_comparison.py --sample-ms 50
```

The script exports the original commit
`8d3ceb142fe5c2f7eb421bfea3eda2f6be2c0247` to a temporary directory, copies the
same comparison harness into it, builds both optimized binaries, and runs them
sequentially after compilation. The temporary checkout is removed automatically;
build outputs are reusable under `target/ecs-comparison-{baseline,current}`.
Outputs are `docs/benchmarks/ecs-baseline-8d3ceb14.csv`, `ecs-current.csv`, and
`ecs-environment.json`. The latter records toolchain, hardware, OS, seed, source
hashes, parent commit, and the uncommitted production diff hash. After the change
is committed, these source hashes identify the measured source independently of
the parent commit.

For an individual production suite:

```sh
KATLA_BENCH_MODE=storage RUSTFLAGS='--check-cfg=cfg(katla_ecs_baseline)' \
  cargo bench -p katla_ecs --bench ecs_comparison
KATLA_BENCH_MODE=scheduler RUSTFLAGS='--check-cfg=cfg(katla_ecs_baseline)' \
  cargo bench -p katla_ecs --bench ecs_comparison
KATLA_BENCH_MODE=typed RUSTFLAGS='--check-cfg=cfg(katla_ecs_baseline)' \
  cargo bench -p katla_ecs --bench ecs_comparison
```

When changing only the production implementation, an existing baseline can be
reused with `--current-only --sample-ms 30`. The script requires identical Rust
harness hashes, hardware, OS, toolchain, sample duration, and seed; it records
separate baseline/current capture times and both CSV hashes. The complete command
above remains the default reproduction path.

The Criterion `ecs_benchmarks` suite also measures bulk creation with lifecycle
events, through `spawn`, and per-entity dirty insertion, through
`get_component_mut_dirty_marking`. The latter starts each measured iteration
with a populated World whose change tracking has been cleared; setup is excluded.
These timings are reported distributions rather than correctness assertions or
percentage claims against an event-free implementation. The recorded Criterion
estimates and raw samples are in `docs/benchmarks/ecs-dirty-marking.json`, with
matching production and harness hashes. The short capture uses ten samples;
longer runs can improve the estimate.

```sh
cargo bench -p katla_ecs --bench ecs_benchmarks --locked -- \
  get_component_mut_dirty_marking --warm-up-time 0.1 --measurement-time 0.3 \
  --sample-size 10 --noplot
```

## Workloads and interpretation

- Scenes contain 1,000, 10,000, or 100,000 entities. Position and velocity have
  three floats each; health has one float. The four-component query additionally
  requires a zero-sized tag, present on either every entity or every hundredth
  entity. Both implementations compute identical sums; assertions check them
  before timing.
- Archetype tables use separate contiguous columns for complete signatures.
  There are two tables (tagged and untagged), with an entity-to-table-and-row
  location index. Component churn performs actual `swap_remove` on every retained
  component, repairs the displaced entity location, pushes the complete row into
  the destination, and maintains the tag column. It does not time a tag bit flip
  or a prefiltered view of sparse components.
- A churn iteration adds and removes the tag on every initially untagged entity
  in a deterministic shuffled order. The narrow retained row is 28 component
  bytes; the wide scenario additionally retains a 256-byte cold component.
  Neither prototype variant duplicates another component store. Assertions
  validate locations, column lengths, cold component values, and retained data
  after the timed run.
- The fixed-schema archetype prototype omits ECS generations, lifecycle event
  emission, dirty tracking, arbitrary component registration, editor metadata,
  and scheduler integration. Sparse churn includes real lifecycle bookkeeping
  and end-of-frame clearing. Prototype results are optimistic storage-only
  comparisons, not a claim that a complete archetype ECS would achieve those
  timings.
- `sparse_query*` measures direct queries. `typed_query*` measures a complete
  registered typed-system frame, including matching-cache access, fresh
  batch-scoped data preparation, execution, and end-of-frame clearing. Cached
  query membership is warmed, and structural mutation is excluded from these
  iteration scenarios. They distinguish iteration-only cost from dispatch cost.
- Scheduling has eight systems, each processing a different component column
  (independent), or all processing the same column (conflicting). Each row performs
  either 1 or 16 dependent floating-point multiply/add operations. Sizes
  0/1k/10k/100k and
  worker pools of 1/2/4/8 threads show empty dispatch overhead and workload
  scaling. Conflicting systems must remain sequential. Pool entry and first DAG
  construction are outside the steady-state timing. Production dispatch estimates
  work as `entity_count * enabled_systems` and uses a conservative threshold of
  32,768. A group below that threshold runs sequentially; workloads with expensive
  per-row work can lower it with `World::set_parallel_work_threshold`.
- Change tracking is part of the production frame cost. Typed mutable queries
  conservatively mark matching writes; a full-column match uses an O(1) dirty
  flag. Direct mutable column views mark the whole requested mutable column,
  including unmatched rows. The historical direct mutable iterator bypassed
  dirty tracking, so its light-system timing has fewer semantics to maintain.
- Chunk processing uses a three-component mutable query and either 1 or 64
  dependent multiply/add operations per row, with chunks of 1,024 rows and the
  same 1/2/4/8-thread pools. Complete frame preparation is included. Lightweight
  rows reveal when preparation/dispatch dominates; heavier rows reveal useful
  parallel throughput.

Every scenario warms three iterations, then collects seven samples of at least
the requested sample duration each (50 ms by default; 30 ms in the recorded
capture). CSV rows contain elapsed nanoseconds per full iteration, sample
index, actual iteration count, match count, and checksum. Reported medians do not
hide the raw spread. Allocation, clock, OS scheduling, CPU frequency, and thermal
state can affect measurements; there is no affinity or frequency pinning.

The historical baseline scheduler contains the exclusive-World aliasing defects
that this change removes. Its timings are retained as a before reference; they
do not validate its safety. The production tests and Miri run establish safety
separately from benchmark timings.

## Recorded results

Both the baseline and current captures were refreshed on 2026-10-02 after the
World/scheduler cleanup, with the same comparison harness, 30 ms sample duration
and seed. Hardware is Apple M5 (10 cores, 24 GiB), macOS 27.0, Rust 1.99.0 /
LLVM 23.1.1. Source and CSV hashes are recorded in `ecs-environment.json`.
Values below are medians in microseconds per complete operation/frame; setup is
excluded.

Measurements ran in a shared development session with other compilation
activity. Raw samples and their spread remain published. These observations do
not establish a causal cleanup speedup; use an idle target host before tuning
thresholds.

### Query iteration and frame preparation

| Entities | Query | Original direct | Current direct | Current typed frame | Current table prototype |
| --- | --- | --- | --- | --- | --- |
| 1,000 | 2 components, dense | 0.975 | 1.487 | 0.828 | 0.538 |
| 1,000 | 4 components, dense | 1.937 | 2.496 | 0.832 | 0.555 |
| 1,000 | 4 components, 1% matches | 1.381 | 0.162 | 0.109 | 0.019 |
| 10,000 | 2 components, dense | 9.402 | 14.759 | 7.502 | 5.729 |
| 10,000 | 4 components, dense | 21.529 | 25.574 | 9.046 | 5.613 |
| 10,000 | 4 components, 1% matches | 15.340 | 0.386 | 0.178 | 0.054 |
| 100,000 | 2 components, dense | 94.876 | 148.914 | 76.951 | 54.498 |
| 100,000 | 4 components, dense | 215.574 | 268.622 | 106.825 | 55.773 |
| 100,000 | 4 components, 1% matches | 153.884 | 3.897 | 1.944 | 0.555 |

Typed rows include cached matching, freshly prepared borrows, execution and change
tracking. Direct rows do uncached joins. The prototype omits generations, events,
arbitrary registration and scheduling, so it remains an optimistic storage
ceiling.

### Component churn

| Entities | Retained row | Original ECS | Current ECS | Current table prototype |
| --- | --- | --- | --- | --- |
| 1,000 | 28 bytes | 22.220 | 23.060 | 15.141 |
| 1,000 | 284 bytes | 21.826 | 23.338 | 32.821 |
| 10,000 | 28 bytes | 281.113 | 295.228 | 168.306 |
| 10,000 | 284 bytes | 281.590 | 299.563 | 376.079 |
| 100,000 | 28 bytes | 3028.833 | 3091.217 | 1879.141 |
| 100,000 | 284 bytes | 3023.196 | 3135.912 | 4317.190 |

### Scheduling across worker counts

Eight independent systems, 100k rows each, 16 dependent multiply/add operations
per row:

| Threads | Original parallel | Current sequential | Current parallel | Sequential / parallel |
| --- | --- | --- | --- | --- |
| 1 | 8700.979 | 9372.448 | 9287.823 | 1.01× |
| 2 | 4382.536 | 9382.323 | 4762.125 | 1.97× |
| 4 | 2886.322 | 9465.864 | 3446.301 | 2.75× |
| 8 | 1913.854 | 10027.406 | 3540.347 | 2.83× |

At eight threads, dispatch overhead, fallback and conflicting work:

| Entities | Work | Original parallel | Current sequential | Current parallel |
| --- | --- | --- | --- | --- |
| 0 | independent, 1 operations/row | 2.275 | 0.180 | 0.186 |
| 0 | conflicting, 16 operations/row | 0.030 | 0.439 | 0.421 |
| 1,000 | independent, 1 operations/row | 4.075 | 4.901 | 4.936 |
| 1,000 | conflicting, 16 operations/row | 86.370 | 96.271 | 93.449 |
| 10,000 | independent, 1 operations/row | 13.663 | 42.682 | 21.094 |
| 10,000 | conflicting, 16 operations/row | 865.737 | 956.740 | 926.514 |
| 100,000 | independent, 1 operations/row | 130.669 | 418.090 | 310.700 |
| 100,000 | conflicting, 16 operations/row | 8657.281 | 9652.459 | 10485.625 |

Conflicting systems remain sequential. The 32,768 estimated-work threshold avoids
worker dispatch for small/light groups; expensive small systems can lower it
explicitly. Historical scheduler timings do not validate its exclusive-World
aliasing.

### Chunked query processing

100k matching rows, three components and 64 dependent operations per row,
including complete frame preparation:

| Threads | Sequential | Parallel | Parallel min–max | Sequential / parallel |
| --- | --- | --- | --- | --- |
| 1 | 9937.094 | 9086.021 | 8935.416–9430.156 | 1.09× |
| 2 | 9880.240 | 4755.345 | 4713.298–4987.637 | 2.08× |
| 4 | 10015.722 | 3054.754 | 2664.417–3224.492 | 3.28× |
| 8 | 10149.417 | 2110.858 | 2043.869–2392.276 | 4.81× |

Chunking is an explicit application choice: work per row matters as much as chunk
size, and lightweight rows can cost more to dispatch than to process sequentially.
The recorded spread limits conclusions about scaling.

### Storage decision

Keep sparse sets as the sole production store. The fixed-schema prototype has a
contiguous-iteration advantage but pays full-row migration on structural changes.
Cached matching, generations, structural filters, lifecycle events, change
tracking and editor access remain on one production implementation. These
observations support retaining sparse storage; they do not establish a blanket
engine speedup or an idle-host thread-scaling curve.

## Rust and Odin compilation

The separate [Odin port report](ecs_odin.md#validation-and-compilation-measurements)
compares complete ECS consumer build/typecheck times, with and without editor
functionality. It does not time ECS runtime throughput or the full engine build.
Raw compiler samples and source/toolchain receipts are in
[ecs-odin-compile.csv](benchmarks/ecs-odin-compile.csv) and
[ecs-odin-compile.json](benchmarks/ecs-odin-compile.json).
