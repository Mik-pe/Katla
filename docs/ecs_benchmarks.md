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

The current capture was refreshed on 2026-10-02 after sealing structural filters and adding the lifecycle cache regression. The baseline is the 2026-10-01 capture with the identical Rust harness, sample duration, host, toolchain, and seed. Hardware is Apple M5 (10 cores, 24 GiB), macOS 27.0, Rust 1.98.1 / LLVM 22.1.8. All values are medians in microseconds per complete operation/frame; setup is excluded.

This refresh ran while the graphics migration was compiling on the host. Several sequential scheduler and chunk samples vary substantially with worker-count scenario order even though those scenarios use the same sequential algorithm. These are host-contention measurements, not a controlled causal speedup comparison. Raw samples and source/CSV hashes remain published; rerun on an idle host before tuning thresholds.

### Query iteration and frame preparation

| Entities | Query | Original direct | Current direct | Current typed frame | Current table prototype |
| --- | --- | --- | --- | --- | --- |
| 1,000 | 2 components, dense | 0.888 | 1.523 | 0.979 | 0.566 |
| 1,000 | 4 components, dense | 1.872 | 2.585 | 0.816 | 0.584 |
| 1,000 | 4 components, 1% matches | 1.392 | 0.174 | 0.365 | 0.020 |
| 10,000 | 2 components, dense | 9.058 | 14.818 | 24.155 | 5.975 |
| 10,000 | 4 components, dense | 20.007 | 26.325 | 21.262 | 5.805 |
| 10,000 | 4 components, 1% matches | 14.994 | 0.406 | 0.288 | 0.057 |
| 100,000 | 2 components, dense | 91.319 | 154.388 | 234.963 | 58.200 |
| 100,000 | 4 components, dense | 201.621 | 298.396 | 280.895 | 59.671 |
| 100,000 | 4 components, 1% matches | 150.289 | 4.363 | 7.964 | 0.587 |

Typed rows include cached matching, freshly prepared borrows, execution, and change tracking. Direct rows do uncached joins. The prototype omits generations, events, arbitrary registration and scheduling, so it remains an optimistic storage ceiling.

### Component churn

| Entities | Retained row | Original ECS | Current ECS | Current table prototype |
| --- | --- | --- | --- | --- |
| 1,000 | 28 bytes | 20.888 | 23.585 | 15.052 |
| 1,000 | 284 bytes | 21.974 | 23.637 | 33.944 |
| 10,000 | 28 bytes | 284.477 | 317.043 | 180.741 |
| 10,000 | 284 bytes | 283.481 | 324.562 | 463.934 |
| 100,000 | 28 bytes | 3106.746 | 4194.328 | 2041.388 |
| 100,000 | 284 bytes | 3143.254 | 3672.819 | 4792.381 |

### Scheduling across worker counts

Eight independent systems, 100k rows each, 16 dependent multiply/add operations per row:

| Threads | Original parallel | Current sequential | Current parallel | Sequential / parallel |
| --- | --- | --- | --- | --- |
| 1 | 8803.500 | 32667.792 | 32318.334 | 1.01× |
| 2 | 4429.435 | 19871.084 | 17804.959 | 1.12× |
| 4 | 2923.667 | 17784.271 | 8580.458 | 2.07× |
| 8 | 1878.750 | 10119.875 | 3156.925 | 3.21× |

At eight threads, dispatch overhead, fallback and conflicting work are visible:

| Entities/system | Work | Original parallel | Current sequential | Current parallel |
| --- | --- | --- | --- | --- |
| 0 | independent, 1 operations/row | 1.115 | 0.272 | 0.375 |
| 0 | conflicting, 16 operations/row | 0.040 | 0.534 | 0.532 |
| 1,000 | independent, 1 operations/row | 4.170 | 6.020 | 5.981 |
| 1,000 | conflicting, 16 operations/row | 86.395 | 117.475 | 140.359 |
| 10,000 | independent, 1 operations/row | 13.859 | 56.043 | 24.205 |
| 10,000 | conflicting, 16 operations/row | 864.971 | 986.403 | 1012.349 |
| 100,000 | independent, 1 operations/row | 128.664 | 464.371 | 309.779 |
| 100,000 | conflicting, 16 operations/row | 8643.167 | 11324.792 | 11239.069 |

Conflicting systems remain sequential. The 32,768 estimated-work threshold avoids worker dispatch for small/light groups; expensive small systems can lower it explicitly. The unsafe historical scheduler timings do not validate its exclusive-World aliasing.

### Chunked query processing

100k matching rows, three components and 64 dependent operations per row, including complete frame preparation:

| Threads | Sequential | Parallel | Parallel min–max | Sequential / parallel |
| --- | --- | --- | --- | --- |
| 1 | 13025.472 | 11309.903 | 9652.177–12687.375 | 1.15× |
| 2 | 12692.847 | 6992.925 | 6449.750–8278.323 | 1.82× |
| 4 | 36642.333 | 11368.958 | 7568.177–40516.416 | 3.22× |
| 8 | 41432.458 | 13146.417 | 8963.281–32293.875 | 3.15× |

The raw spread and varying sequential timings show contention. Chunking is an explicit application choice: work per row matters as much as chunk size, and lightweight rows can cost more to dispatch than to process sequentially.

### Storage decision

Keep sparse sets as the sole production store. The fixed-schema prototype has a contiguous-iteration advantage but pays full-row migration on structural changes; the wide-row churn measurement is slower than the sparse implementation even without full ECS bookkeeping. Cached matching, generations, structural filters, lifecycle events, change detection and editor access remain on one production implementation. These observations justify retaining sparse storage; they do not establish a blanket engine speedup or an idle-host thread-scaling curve.
