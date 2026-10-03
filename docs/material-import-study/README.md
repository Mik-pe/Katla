# Full material asset import measurements

Five alternating baseline/current pairs on native Vulkan imported the complete
DamagedHelmet asset eight times in each process. The baseline is `a162c360`;
the current implementation is the image upload/sharing change committed with
this report. The same benchmark fixture was copied into the detached baseline
without engineering changes. Raw [samples and metadata](measurements.json), ten
process logs and the reproduction script are retained.

| Median | Baseline | Current |
| --- | ---: | ---: |
| Decode full glTF geometry and five images | 221.105 ms | 220.451 ms |
| First import CPU return | 410.241 ms | 404.992 ms |
| First import through device idle | 410.647 ms | 410.479 ms |
| Seven additional imports CPU return | 1227.087 ms | 15.283 ms |
| Seven additional imports through device idle | 1227.522 ms | 16.086 ms |
| Unique image handles for eight models | 40 | 5 |

Warm import completion is about 76× faster in this workload, with five image
uploads shared across eight independently registered materials. First import
and decoding show no meaningful improvement. Current imports additionally
allocate/generate complete mip chains; the baseline stores only mip zero.
These are complete asset preparation timings, not frame-rate measurements or
an isolated image-transfer benchmark. Native allocation bytes and Rust
allocation counts are not measured here.

The asset is 3,773,916 compressed bytes, with 46,356 imported vertices/indices
and five 2048×2048 images totaling 60 MiB of decoded source pixels. Each process
uses test/dev optimization level 1 on an Intel Core i5-1035G7 and Iris Plus ICL GT2
under Linux. Rust/toolchain, CPU information, fixture/asset hashes and tree
provenance appear in the measurements. OS file pages and driver compilation
were warmed by excluded trials. Process order alternates; the data retain
variation rather than applying unit-test performance thresholds.

The ready timings include `wait_for_device`, so asynchronous upload submission
cannot move GPU work outside the measured interval. Compilation of the Rust test
binary, renderer initialization, the separate decoder probe, native acceptance
readback and cleanup lie outside import intervals. Imported PBR shader pipelines
are compiled, but this benchmark does not draw a full PBR scene. A native shader
loads the first and eighth imported albedo images into RGBA16F; both must match
CPU sRGB decoding and each other. The resulting texel bytes are identical across
all ten runs. Scene cleanup verifies that tracked image owners are released.

Vulkan validation is disabled in this full PBR compile benchmark for the
[documented Intel compiler failure](../../katla_gfx/src/material/API.md).
Separate strict native fixtures validate upload/filtering, failed submission,
image lifetime and scene sharing. The optional headless MCP service reports EOF
when no client initializes its stdio transport; this is outside timing intervals
and is not a GPU validation result. Physical Metal measurements remain unavailable.

To reproduce, make a detached baseline and copy only the benchmark stimulus:

```bash
git worktree add --detach /tmp/katla-material-import-baseline a162c360
cp katla_app/src/application/spawning/import_benchmark.rs /tmp/katla-material-import-baseline/katla_app/src/application/spawning/
```

Add `#[path = "import_benchmark.rs"] mod import_benchmark;` to the baseline's
`material_tests.rs`, then run from the current checkout:

```bash
python3 scripts/measure_material_imports.py --baseline /tmp/katla-material-import-baseline --iterations 5 --output /tmp/katla-material-import-results
```

The script builds both fixtures, runs their native test binaries serially,
checks matching output texels, and writes metadata, logs and median values.
