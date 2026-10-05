# Cross-backend graphics acceptance

[odin/gfx_conformance](../odin/gfx_conformance) defines shared observable
contracts and receives actual native operations from Metal and Vulkan. Native
entrypoints and backend tests exercise the same generic graph/resource API.
Application suites separately prove real consumers such as models, particles,
UI, picking and editor captures. See [graphics ownership](graphics_core.md) and
[the detailed GPU contracts](gfx_odin.md).

## Run the current suite

From the repository root, build dependencies and run configured CPU consumers:

```sh
odin run tools/build -- --tests
odin run tools/build -- --sanitize --tests
```

Require the affected native adapter explicitly:

```sh
odin run tools/build -- validate gpu --native-metal --sanitize
odin run tools/build -- validate gpu --native-vulkan --sanitize \
  --vulkan-library /absolute/path/to/libvulkan \
  --vulkan-icd /absolute/path/to/driver_icd.json
```

The Metal lane requires macOS arm64 and real Metal 4/Tier2 support. It sets
`MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` before launch. Vulkan initializes
Khronos synchronization validation and checks its collected errors. Linux
Vulkan execution requires an actual loader/device; descriptor-array acceptance
also requires the advertised native indexing features. Unsupported hardware is
not a successful native run. Windows/Linux typechecks and object builds do not
establish window or GPU execution on those hosts.

Application rasterization and transactional resource behavior use:

```sh
odin run tools/build -- validate render --native-metal --native-vulkan \
  --sanitize --vulkan-library /absolute/path/to/libvulkan \
  --vulkan-icd /absolute/path/to/driver_icd.json
odin run tools/build -- validate ui --sanitize --backend both \
  --vulkan-loader /absolute/path/to/libvulkan \
  --vulkan-icd /absolute/path/to/driver_icd.json
```

These paired application fixtures currently require Darwin arm64. Add
`--native-surface` to the applicable GPU/render driver for real windowed
acquire/resize/present checks. Surface acceptance requires a desktop session.
Script/physics-backed particle acceptance uses the render driver's `--particles`
with the verified canonical `--build-manifest`. See each driver's `--help`
and [build/launch instructions](odin_build.md) for isolated outputs and native
dependency paths.

## Required observations

Shared GPU scenarios verify actual compute values, color/depth pixels, immutable
bindings, busy slot behavior, exact out-of-order retirement, alias handoffs and
retained copies after public owner removal. Backend scenarios additionally
exercise mesh and indirect draws, stencil/blend/raster state, 3D pitched copies,
filtered mip chains, compressed formats and fixed material descriptor arrays.

Application acceptance must check its own useful output and failure behavior.
Examples include linear model composition, native physics contact overlays,
GPU particle state/draws, source-frame picking, UI clips/textures/font shaping,
transactional resize and texture replacement. A build or a successful empty
submission cannot replace these observations.

Readback must come from an exact committed graph export and retained ticket,
with actual row/slice pitches and format taken into account. Rejected candidates
must preserve previous live owners, commands and accepted state. Teardown must
release tracked Odin allocations and native owners. Native ASan address checks
are distinct from process-exit audits of external driver/framework caches;
[shader validation](gfx_shader_odin.md) documents that boundary.

## Add or extend a contract

1. Keep shared scenarios backend-neutral: supply native function inputs and
   author ordinary graph declarations/packets. Backend adapters own capability
   admission and native setup.
2. Assert observable bytes, pixels, typed failures or public ownership behavior.
   Include rejection and retry when publication or lifetime is involved.
3. Preserve command order, exact frame identities and source generations.
   Never manufacture completion, initialized content or a ready native handle.
4. Express legitimate differences as capability rejection. An unsupported path
   must not silently emit no work or omit required validation.
5. Run the affected native path on each supported adapter before delivery.
   Report the exact hardware/backend, sanitizer mode and unavailable coverage.

Use small targets and bounded waits. Performance and stress checks belong to
separate measured fixtures; ordinary contract tests should diagnose which
promise broke. [CI policy](ci.md) distinguishes source checks, configured CPU
acceptance and required native device evidence.
