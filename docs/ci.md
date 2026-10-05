# Continuous integration and native acceptance

The canonical workflow is [.github/workflows/odin.yml](../.github/workflows/odin.yml).
It builds the actual Odin editor and its source-pinned native dependencies;
CPU suites run serially with strict vet/style and bad-memory checks.
Cargo is used only for the isolated offline Naga compiler. Builds and launches
run `tools/build` in Odin; validation and authoring also use Odin.
Native compilation calls Clang/Clang++ and the archiver directly, without CMake
or make. Image decoding has no C library or platform-specific build.

| Platform | Explicit runner | Acceptance |
| --- | --- | --- |
| macOS | `macos-26`, Apple Silicon | Cold normal and combined C/C++/Odin ASan builds, CPU tests, Metal graph and actual editor frames when Metal 4 is available |
| Linux | `ubuntu-24.04` | Source-pinned SDL X11/Wayland build, canonical editor, CPU tests, validated native Vulkan graph execution |
| Windows | `windows-2025` | Source-pinned native dependencies, actual canonical editor code generation, CPU tests and native retained-handle filesystem contracts |

Use exactly one explicit macOS generation. Do not add older macOS jobs or
`macos-latest`. When adopting another generation, replace the runner and this
policy together and verify its native Metal paths.

## Native boundaries

A macOS runner lacking Metal 4 reports native acceptance **BLOCKED**. Its SDK
build and CPU tests still run. Typed capability rejection must happen before
invalid native calls; a virtualized GPU is not physical Apple Silicon evidence.
Actual Metal launches set `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1`.
Vulkan fixtures require Khronos synchronization validation and inspect its
validation count alongside real pixels, compute, resource lifetime and recovery.

Linux/Windows compilation proves neither native desktop input nor GPU behavior
on untested hardware. Windows retained-directory tests execute in the Windows
job; cross-compilation on macOS only proves their types and code generation.
Fixture provider conversations and simulated transports do not establish
attachment to a human conversation or paid model availability.

## Sanitizers

Address-instrumented C/C++ dependencies must match Odin's LLVM major, including
Clang++ and static archives. The builder rejects mismatches and selects the
same Clang directory for Odin's macOS linker driver. Manifest-based ASan
validation restores that driver selection in each process. CPU tests retain
LeakSanitizer and Odin bad-free/allocation tracking. On macOS the specifically
observed `CFPrefsPlistSource`/`CFPrefsSearchListSource` initialization caches may
use narrow named stack suppressions; do not suppress all CoreFoundation.
Native GPU launches retain ASan and Odin ownership checks but disable external
framework/driver process-exit leak detection explicitly. Native audio defaults
to leak checks; the validator's explicit `--native-asan-leaks external-driver`
mode documents observed Apple HAL/NSXPC process caches. CPU mixer tests still
retain leak checks.

## Local equivalents

```sh
odin run tools/build -- --tests
odin run tools/build -- --sanitize --tests
odin run tools/build -- validate physics
odin run tools/build -- validate gpu --native-metal --sanitize
odin run tools/build -- validate gpu --native-vulkan --vulkan-library /path/to/libvulkan --sanitize
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 odin run tools/build -- run -- --frames 100
```

Application composition adds [render validation](render_features_odin.md),
[overlay/picking contracts](editor_overlays_odin.md),
[audio acceptance](audio_odin.md) and
[live viewport transport](shared-editor-view.md). The editor supports true
headless captures and layout/graph diagnostics. Retain failed logs and images
rather than inferring acceptance from successful screenshots alone.

CI uploads build logs, verified dependency manifests and native receipts/images
for seven days. Workflow permissions remain read-only. Every pushed head is a
new CI target; local tests and older successful runs do not establish current
remote-head acceptance.
