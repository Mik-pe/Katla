# Metal texture uploads

Metal sampled, storage, and attachment textures use private GPU storage. Shared
textures are limited to explicit CPU readback fixtures and captures. Creation and
updates queue a copy through the same pooled staging service; they return errors
without creating default textures. Asset loaders own any placeholder decision.

`TextureDescriptor` allocates depth slices, array layers, and mip levels. Nonempty
creation data fills the complete base mip of the first array layer. Remaining
layers and explicit mip chains use `GpuRenderer::update_texture_region` with a
`TextureUploadRegion`. Setting `generate_mips` generates all lower mips after a
complete base upload; this requires one array layer and an uncompressed,
filterable color format. 32-bit float/integer, depth, compressed, and array mip generation is
rejected. Vulkan's asset upload API rejects the extended descriptor and region
capabilities explicitly. Array and 3D textures use explicit graph texture bindings;
they do not occupy slots in the built-in 2D bindless table.

The region contract validates mip extents, layers, 3D bounds, format block sizes,
row and image pitches, source length, and checked arithmetic. Zero pitches mean
tightly packed data. Padding after the last uploaded row/image is unnecessary.
BC1 and BC3 use 4×4 blocks; origins align to blocks, and a partial block is accepted
only where the region reaches a mip edge. Devices without BC support return a
typed capability error. Depth/stencil and automatic-format pixel uploads are
rejected before staging. Metal repacks source rows to a 256-byte aligned staging
pitch; source pitches remain independent from native alignment.

One batch encodes before frame consumers. Its transfer writes are exposed to
imported graph resources as exact mip/layer producers. Queue barriers cover
previous-frame readers before an in-place update; encoder barriers preserve
ordering between overlapping queued uploads. Native transfer resources enter the
submission's residency set. An encoded batch retains its staging buffers and
private destinations until the exact submission completes. Aborting before
submission requeues the original bytes in order. Submission IDs from other frame
slots cannot retire those allocations.

Admission is atomic. Default limits are 256 MiB queued and per submission, 512 MiB
resident staging, and 4,096 uploads per submission. Exhaustion returns an upload
error for retry after submission/completion. Encoded but unsubmitted bytes count
against admission, so abort replay cannot exceed the batch bound. Completed
buffers return to a best-fit pool; free buffers are released when new allocation
would exceed its budget. `GpuRenderer::texture_upload_metrics` reports queued
bytes, cumulative submitted bytes, resident staging/high-water bytes, completed
batches, observed completion latency, and validation/allocation/submission failures.
Byte gauges include the aligned staging layout.

The focused tests exercise byte-exact RGBA/BGRA, R8/RG8, integer and float uploads, padded
partial regions, mip/array/3D subresources, generated mip chains, compressed
blocks or capability rejection, staging lifetime/budgets, and abort replay. GPU
sampling probes compare shared and private storage through direct compute and
the production bindless snapshot. Run them under native validation:

```sh
METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_gfx --lib texture_upload -- --test-threads=1
cargo test -p katla_gfx --lib texture::upload
```

The explicitly ignored physical-device benchmark compares the old synchronous
shared-texture `replaceRegion` fixture with pooled staged private uploads for 512
32×32 textures and four 2048×2048 streaming textures. It reports medians after one
warmup, CPU creation/upload time, time until the upload path is ready, payload
bytes, and staging high-water bytes. Every trial then verifies the last texture
through a GPU readback. Shared readiness is its synchronous CPU upload return;
private readiness includes the exact blit submission completion. These timings
measure upload readiness, not texture sampling speed or whole-frame latency.

```sh
METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_gfx --lib test_texture_upload_benchmark -- --ignored --nocapture --test-threads=1
```

Measured on 2026-10-02 with Metal API validation enabled, Apple M5 (10 cores),
24 GiB memory, macOS 27.0 (26A428). Five measured trials follow one warmup; CPU
and ready columns are independent medians. Payload is 2 MiB for the small case
and 64 MiB for streaming.

| Workload | Upload path | CPU upload (ms) | Ready (ms) | Staging high-water (MiB) |
| --- | --- | ---: | ---: | ---: |
| 512 small textures | Synchronous shared | 2.778 | 2.778 | 0 |
| 512 small textures | Staged private | 2.347 | 5.611 | 4 |
| 4 large textures | Synchronous shared | 5.791 | 5.791 | 0 |
| 4 large textures | Staged private | 1.740 | 5.293 | 64 |

Staging reduces measured CPU upload time in both cases. Waiting for the small
batch's GPU completion costs more than its synchronous shared upload; large
streaming readiness improves modestly. These measurements include validation
overhead and do not establish application-frame performance.

Whole-scene acceptance compares the default editor rendered by the historical
shared-texture backend and the final private-texture path. Checked static material,
ground, sky and shadow regions match exactly. UI comparison uses identical saved
preferences; animated particles and poses vary with wall-clock time. Native direct
and bindless sampling/readback probes also pass. See [Metal 4 validation](metal4_validation.md)
for artifact hashes, region bounds and measurement limits.
