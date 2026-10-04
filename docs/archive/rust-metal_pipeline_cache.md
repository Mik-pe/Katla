> Historical Rust contract at [8b76a167](https://github.com/Mik-pe/Katla/blob/8b76a1670bf961db7afd2151545b5c845c413835/docs/metal_pipeline_cache.md). See the current document in docs.

# Metal pipeline cache

Metal uses one Metal 4 compiler/archive service for libraries, graphics pipelines,
and compute pipelines. Pipeline registration submits native compilation to dedicated
compiler workers and waits for the result. Renderer construction and material
registration prepare the required pipelines before frame acquisition. Material
registration prepares every supported floating color format, including the UI
instanced entry points; integer targets use their declared format. Draw collection
and encoding only consume ready pipeline states. Requesting an unprepared variant
returns an error.

Shader reload resolves a single source snapshot on a background worker and builds
all of a material's existing variants. Frame acquisition polls completed jobs. A
successful job replaces the entire variant map; failure keeps the previous map.
Submitted frame slots retain their pipeline objects through completion, including
states replaced by a reload. Include changes conservatively schedule all registered
shader-backed materials, so a shared include cannot leave dependent materials stale.

## Identity and persistence

SHA-256 keys cover resolved WGSL and generated MSL, shader binding profile and ABI,
MSL language version and fixed compiler options, entry points, vertex attributes and
strides, every color attachment's format and blend/write state, sample count,
rasterization, alpha state, topology, vertex amplification, indirect-command state,
depth/stencil attachment formats and depth/stencil/cull/front-face state. Compute
keys additionally cover the workgroup size and current fixed compute options.
Specialization constants are rejected until the Metal implementation supports them.

The schema-2 manifest records the macOS version, GPU registry identity and supported
GPU families, compiler/Naga identity, complete Cargo.lock digest, and binding ABI.
Incompatible metadata starts an empty index. Native `MTL4PipelineDataSetSerializer`
artifacts use capture-binaries mode; each content-addressed `.mtl4` file has a
manifest checksum. Reads verify the checksum and native container header/slice
bounds before invoking Apple's loader. Missing, truncated, mismatched, or rejected
artifacts are invalidated and rebuilt. Writes serialize to a temporary file, sync
it, rename it, then atomically publish and sync the manifest. Compiler and descriptor
ownership spans native compilation and serialization. Cache write failures keep the
compiled state usable and produce diagnostics.

The default directory is
`~/Library/Caches/dev.ravboet.katla/pipelines-metal4`.
`KATLA_PIPELINE_CACHE_DIR` selects an isolated directory for measurement.
`pipeline_cache` logs expose open, invalidation, library compile, pipeline miss/hit,
compile/save durations, save failures, reload completion, failure, and swap events.
A hit is counted only when the native archive creates the requested pipeline.

## Native validation and measurements

Measured on Apple M5, macOS 27.0 build 26A428, with
`METAL_DEVICE_WRAPPER_TYPE=1` set before launch, on 2026-10-02:

| Native measurement | Duration |
| --- | ---: |
| Minimal graphics pipeline, source to cold archive | 49.990 ms |
| Same pipeline, existing archive, actual native hit | 0.918 ms |
| Changed source, replacement pipeline | 23.050 ms |
| Registered material reload, seven prepared variants | 163.326 ms |

The pipeline measurement includes source/library preparation, service opening, and
pipeline lookup/compile. Its warm run uses the process library cache and a disk
archive, so it measures pipeline-service readiness rather than a fresh application
process. The material measurement includes the background job and acquisition-side
publication. These are single Cargo test-profile observations, not statistical averages.

The native regression tests exercise graphics and compute archive hits, changed
workgroup/source/layout/state keys, incompatible/corrupt metadata, truncated native
archive recovery, and failed/successful material replacement. The material test also
asserts that requesting a prepared variant produces no compiler miss. Run them with:

```sh
METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_gfx --lib metal::pipeline_archive::tests -- --nocapture --test-threads=1
METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_gfx --lib test_async_reload_retains_previous_pipeline_on_failure_and_swaps_success -- --nocapture --test-threads=1
METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_gfx --lib test_metal4_pipeline_latency_benchmark -- --ignored --nocapture --test-threads=1
```

Fresh-process full-editor observations use an isolated archive directory and the
same development build under both Metal validation flags. Empty-cache launch to
frame-loop entry took **2.755 s** with 62 native pipeline misses; the next
process took **0.360 s** with **69 native hits and zero misses**. All cold
compilation finished before frame-loop entry; neither process compiled during
frame rendering. These are single process observations on a shared host.
See [scene validation and raw provenance](metal4_validation.md) for binary/source
hashes, timing boundaries and the separate frame-pacing results.
