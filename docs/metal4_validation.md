# Odin native GPU validation

Validation runs canonical Odin graph and application consumers. Superseded Rust renderer measurements do not establish this implementation's behavior or performance. Current contracts are [Metal backend](metal_backend.md), [graphics ownership](graphics_core.md), [UI/picking](odin-ui-rendering.md) and [shader reload](odin-shader-reload.md).

## Hardware boundaries

Metal requires macOS Apple Silicon with actual Metal 4 support; texture arrays also require argument-buffer Tier 2. The required CI runner is `macos-26`. A hosted GPU lacking Metal 4 produces **BLOCKED** native acceptance while SDK builds and CPU tests still run. Those results do not become GPU passes.

Local Odin acceptance used an Apple M5 with Metal 4 and Vulkan 1.3 through MoltenVK, with native validation enabled on both. Local Vulkan paths were `/usr/local/lib/libvulkan.dylib` and `/usr/local/share/vulkan/icd.d/MoltenVK_icd.json`; verify these files for a new run. Linux/Windows native desktop input requires execution on those operating systems. Cross compilation does not prove it. Current application/UI paired GPU harnesses require macOS Apple Silicon; the generic Vulkan harness also builds outside macOS.

## Build and CPU acceptance

From the repository root:

```sh
python3 scripts/build_katla_odin.py --tests
python3 scripts/build_katla_odin.py --sanitize --tests
```

The builder verifies Odin, builds pinned dependencies from source, packages shader sources and records artifact hashes in `build.json`. Normal/ASan outputs are separate. Native C/C++ instrumentation must match Odin's LLVM major/runtime; the builder selects it. CPU/font/decoder checks retain LeakSanitizer. A documented external CFPreferences suppression, when used by the broader app validator, is limited to its observed initialization stacks and reported by that run.

## Native core contracts

Metal alone:

```sh
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
  python3 scripts/validate_odin_gpu.py --native-metal --sanitize
```

Both backends on the local MoltenVK host:

```sh
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
  python3 scripts/validate_odin_gpu.py \
  --native-metal --native-vulkan --sanitize \
  --vulkan-library /usr/local/lib/libvulkan.dylib \
  --vulkan-icd /usr/local/share/vulkan/icd.d/MoltenVK_icd.json \
  --glslc /usr/local/bin/glslc
```

`--native-surface` adds actual Cocoa Vulkan acquisition, resize and presentation. Use a desktop session with the native window slot available. Explicit dependency paths must name existing files. `--skip-shader-checks` requires an already verified compiler target and omits those checks; it is not a cold-build receipt.

Core fixtures read actual compute/color/depth/stencil results. They verify direct/indirect work, mesh commands, stencil/blend/scissor/wireframe, formats, pitched D2/D3 copies, mip generation, GPU-produced indirect commands, mixed physical allocation aliases, queued slot ownership, discard, retained source-bound readback, fixed sampled/storage arrays and stale resource rejection. Reverse-Z pairs clear 1/Less with clear 0/Greater, draws farther geometry after nearer geometry, and checks actual nearest color/depth and untouched-clear pixels. Vulkan validation must report zero errors. Typed feature rejection is a capability boundary, not rendered-output acceptance.

On a Vulkan device with limited descriptor indexing, first run the same validator
with `--vulkan-baseline`. This still executes every non-array GPU contract;
only sampled-array4096 and storage-array2 proofs are excluded. The separate
`--vulkan-probe-array-capabilities` invocation builds the native fixture and
prints actual feature/descriptor limits as JSON. Exit 0 permits the complete
array suite; exit 77 means only that array breadth is **BLOCKED**. Missing
devices, loader failure, initialization errors or validation failure return
other failure codes and must fail the job. The probe is capability evidence,
not rendered-output acceptance. CI must run the actual baseline even when the
array probe reports Unsupported.

## Application, UI and picking

The scene/model validator builds dependencies and exercises real consumer graphs:

```sh
python3 scripts/validate_odin_render.py \
  --native-metal --native-vulkan --native-surface --sanitize \
  --vulkan-library /usr/local/lib/libvulkan.dylib \
  --vulkan-icd /usr/local/share/vulkan/icd.d/MoltenVK_icd.json
```

The optional particle scenario requires source-built Luau/Box3D libraries; consult `--help` for `--particles`, `--luau-library` and `--box3d-library`. Selected consumer fixtures are distinct from complete owner-loop acceptance.

After the canonical ASan build, reuse its exact dependencies for UI/font/picking:

```sh
python3 scripts/validate_odin_ui_gpu.py --sanitize --backend both \
  --shader-compiler target/katla-odin/darwin-arm64/asan/shader-compiler/debug/katla-shader-compiler \
  --font-library target/katla-odin/darwin-arm64/asan/deps/fonts/libkatla_font_native.dylib \
  --image-library target/katla-odin/darwin-arm64/asan/deps/image/libkatla_image.a \
  --vulkan-loader /usr/local/lib/libvulkan.dylib \
  --vulkan-icd /usr/local/share/vulkan/icd.d/MoltenVK_icd.json
```

Its retained-context/provider path verifies Swedish preedit/commit, Unicode shaping/fallback, grapheme deletion, visual bidi navigation, actual glyph coverage, clipping, popup order and image batching. Readbacks verify linear alpha, sRGB passthrough, decoded BMP/TIFF sampling, foremost mapped R32Uint IDs and alpha masking. Paired color/ID snapshots keep their submission/entity map across pending work, resize and teardown. This does not prove OS-generated IME or live agent transport.

Launch the canonical editor through its verified builder/launcher:

```sh
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
  python3 scripts/run_katla_odin.py --sanitize --backend metal -- \
  --frames 100 --preferences target/native-editor/preferences \
  --screenshot target/native-editor/editor.png
```

For Vulkan choose `--backend vulkan --vulkan-loader <existing-loader>` before `--`, and set `VK_ICD_FILENAMES`. `--headless` after `--` selects the actual window-free owner loop. A retained PNG alone does not prove desktop input, clean-close recovery, agent attachment or editing operations; each requires the actual editor/endpoint journey.

## Reproducible receipts

Record exact source/index tree, build manifest, compiler/dependency identity, device/driver/OS, commands, validation flags, exit codes and selected scenarios. Preserve readback hashes/comparisons and lifetime assertions. Report **PASS**, **FAIL** or **BLOCKED** per requested platform/path, including unexecuted Windows/Linux desktop acceptance.

Native GPU processes retain ASan address checks and Odin allocation tracking but use the explicit external CF/ObjC/driver process-exit leak boundary `ASAN_OPTIONS=detect_leaks=0`. CPU-only runs retain `detect_leaks=1`; do not apply the GPU boundary to font, decoder or shader-worker ownership tests. The focused source-reload harness verifies old pending pipelines, failed WGSL/native rejection and repair through exact GPU pixels. Production aggregate acceptance must additionally cover four consumers and future scene/model rebuilding.

Current CI runs Metal core contracts and limited editor frames when its capability probe succeeds, plus generic Vulkan contracts on Linux. Separate UI/picking and complete editor interaction receipts remain distinct from those jobs. Correctness fixtures and the superseded renderer's CSVs establish no performance or 60-FPS guarantee.
