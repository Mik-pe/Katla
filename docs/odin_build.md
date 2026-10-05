# Build and run Katla in Odin

The canonical source build is `tools/build`. It compiles the
Odin editor, the MCP stdio/proxy programs and their native dependencies.
Linux and Windows also build the pinned SDL3 window/input dependency. It does
not build or load any Rust engine crate. Cargo builds only the locked,
standalone offline WGSL compiler in `tools/naga_bridge`; that executable is a
build tool invoked by the shader cache, not a runtime FFI library.

```sh
odin run tools/build -- --tests
odin run tools/build -- run --no-build
```

Normal builds use Odin's speed optimization and retain debug symbols. Sanitized
builds remain unoptimized by default; add `--optimize` to instrument an optimized
editor as well.

Odin, Git, curl, a native C/C++ compiler, an archiver and Cargo are required.
Windows needs Clang/LLVM and a configured Windows SDK. Native dependencies are
compiled directly by the Odin build tool. Python, CMake, Ninja and make are not
build or launch dependencies. Extended QA drivers run through the same Odin command.
Git keeps source files in LF form on every host so pinned checksums remain
identical even with Windows `core.autocrlf`. Source-pinned upstream checkouts\nalso explicitly disable automatic CRLF conversion.

Font sources and licensed fallback fonts are retrieved from pinned revisions;
all source checkouts must retain their exact clean revision.

Install the verified `dev-2026-09` compiler before invoking the build tool:

```sh
scripts/install_odin.sh target/odin-toolchain
# Add the printed executable's parent directory to PATH.
```

Windows uses `./scripts/install_odin.ps1 target/odin-toolchain`. Both bootstrap
scripts verify the host archive against `scripts/odin-releases.sha256`, preserve
Odin's core/vendor collections and need no Python. Alternatively install Odin
normally and select its executable with `--odin FILE` or `ODIN`. Updating the
compiler release requires updating every corresponding archive digest.

The output is `target/katla-odin/<host>-<arch>/<normal|asan>`. Each directory owns
its native libraries, fonts, compiler executable, binaries, canonical WGSL
sources in `shaders/` and `build.json`. Shader sources come exclusively from
`odin/app/render/shaders`, including relative include files; their hashes enter
the same manifest as native artifacts. The launcher supplies `--shader-root`
with that shipped directory. The old Rust shader asset directory is not used.
The manifest is published only after every requested build/test succeeds; a
failed rebuild removes the previous successful manifest. `tools/build run`
checks the host, architecture, checkout, sanitizer mode and every recorded
artifact hash before launching. `--no-build` explicitly reuses compiled source;
run the build again after source changes. The launcher normally rebuilds first.
A directory cannot be reused for another host or sanitizer mode.

A bounded validated Metal run with actual retained viewport GPU readback is:

```sh
mkdir -p target/odin-editor-proof
odin run tools/build -- run --no-build -- \
  --frames 100 --preferences target/odin-editor-proof/preferences \
  --screenshot target/odin-editor-proof/editor.png
```

The launcher supplies explicit shader/compiler/font/Luau/Box/window library paths and
project/resource roots. Metal launches always set `MTL_DEBUG_LAYER=1` and
`METAL_DEVICE_WRAPPER_TYPE=1`. Select another project with `--project DIR` and
its resource directory with `--resources DIR`; scene paths are passed to the
application's confined project/file resolver. Pass application arguments after
`--`. Native `--help` lists current application options.

```sh
odin run tools/build -- run --no-build --backend vulkan \
  --vulkan-loader /absolute/path/to/libvulkan.dylib -- --frames 100
```

The loader can also be selected through `KATLA_VULKAN_LIBRARY`, or discovered
from the operating system. Backend capability failures remain errors. A
successful compile does not establish device availability or native acceptance.

ASan compiles every C/C++ dependency with the non-Apple Clang major matching
Odin's reported LLVM major, and links the Odin program with
`-sanitize:address`. The build verifies explicit CC/CXX overrides too. On macOS
install the corresponding Homebrew `llvm@<major>`; on Linux put matching Clang
and Clang++ on PATH or set CC/CXX explicitly. Normal and ASan libraries and
object directories are isolated.

```sh
odin run tools/build -- --sanitize --tests
odin run tools/build -- run --no-build --sanitize -- --frames 100
```

CPU suites run serially with bad-memory failures enabled, including real
Odin decoder, physics, Luau, shader-process and font consumers. Explicit
package runs cover app, app/render, app/editor, gfx, shader tests, script,
audio, UI, precise image codecs, resources, agent/host and native fonts. Their ASan
processes always enable leak detection, even if the invoking environment sets
`detect_leaks=0`. On Darwin the only suppressions are the observed Apple
`CFPrefsPlistSource` and `CFPrefsSearchListSource` process-cache stacks;
application and dependency ownership remains
checked. This CPU policy does not change native GPU or audio-driver validation
environments. Native GPU and audio-device validation remain explicit:

```sh
odin run tools/build -- validate gpu --native-metal --sanitize
odin run tools/build -- validate audio --native --switch-default
```

Native GPU ASan launches check addresses and Odin ownership while excluding
process-exit leaks from retained Apple CF/ObjC/driver objects. CI applies that
boundary only to native launches; CPU processes retain leak detection. Audio
device validation has its own explicit boundary documented in
[audio_odin.md](audio_odin.md).

Linux CPU consumers also link the native curl and Mbed TLS libraries. Install
`libcurl4-openssl-dev libmbedtls-dev` alongside the SDL/native build dependencies
on Ubuntu; macOS uses its platform transport.

On Linux, use `odin run tools/build -- validate gpu --native-vulkan --vulkan-library
/usr/lib/x86_64-linux-gnu/libvulkan.so.1 --vulkan-icd /path/to/lvp_icd.json`,
substituting the host's actual loader file and ICD manifest. Vulkan validation must be
available. Resource-array fixtures require actual descriptor-indexing features;
an unsupported device does not establish array acceptance.

| Dependency | Canonical source/owner |
|---|---|
| WGSL compiler | Locked `tools/naga_bridge/Cargo.toml` and Cargo.lock; Naga 29.0.1 |
| Physics | `tools/build/native.odin`; Box3D v0.1.0 commit `8441b4a06d6d09dcfb0b0f704df4d847d1437b92`; bounded adaptations in `box_adaptations.odin` |
| Script VM | `tools/build/native.odin`; Luau 0.709 commit `b968ef742741bb2b703afc3b3c53f06608c87481` |
| Audio | `tools/build/native.odin`; miniaudio 0.11.25 and hashed repository stb_vorbis |
| glTF | `tools/build/native.odin`; repository-pinned cgltf C source |
| Image codecs | `odin/image`; Odin PNG/BMP, sequential/progressive JPEG, Classic/BigTIFF and fixed-output Deflate; no native image library |
| Typography | `tools/build/fonts.odin`; exact FreeType/HarfBuzz/SheenBidi/Unibreak commits and hashed fallback fonts |
| Window/input | `tools/build/window_desktop.odin`; SDL3 3.2.28 commit `7f3ae3d57459e59943a4ecfefc8f6277ec6bf540`, direct compilation and explicit host configuration |
| Preferences | `tools/build/native.odin`; repository-pinned tomlc17 C source |

Box3D and Luau source manifests are explicit Odin constants tied to their pinned revisions; no build generator is executed. SDL has a checked-in host configuration for the exact
pinned revision. Linux generates native Wayland protocol code with
`wayland-scanner`, links both X11 and Wayland and retains IBus/Fcitx IME.
Disabled SDL subsystems are owned by Katla's audio and graphics packages.

The canonical `.github/workflows/odin.yml` cold-builds dependencies and CPU
consumers on macOS, Linux and Windows. macOS uses exactly `macos-26` on Apple Silicon,
probes Metal 4 before native GPU tests and records **BLOCKED** on unsupported
hosted hardware. Linux runs Vulkan contracts with Khronos validation.
Vulkan baseline contracts run before the resource-array capability probe;
missing array features or limits mark only that breadth **BLOCKED**, while
loader, validation or baseline failures fail the lane. Windows
selects the SDK, builds the desktop executable and native dependencies, runs
CPU consumers, and executes the actual confined-resource suite, including
Windows junction rejection and retained-directory-handle operations.
Diagnostics/manifests are retained for seven days. The workflow definition is
not a receipt of a completed CI run.

The desktop entrypoint uses Cocoa on macOS arm64 and the pinned SDL3 bridge
with Vulkan on Linux/Windows. Linux source builds require X11 and Wayland
development packages, xkbcommon, EGL, D-Bus, IBus, wayland-scanner and pkg-config;
both native drivers and actual SDL IBus/Fcitx IME support are mandatory. On
Windows configure the SDK developer environment before building. The launcher
retains the SDL DLL dependency directory on PATH.
`--objects-only --tests` is an explicit alternative that compiles the real
editor entrypoint and reachable app/gfx/render code into host objects, without
linking a desktop executable. It also builds portable MCP programs; its
manifest cannot launch an editor. Linux/Windows source build and object checks
must be distinguished from native GPU/window acceptance, which requires the
corresponding actual device.

The desktop frontend supports `--headless` with a real offscreen target and no
OS window or presentation surface. `-s`/`--single-frame` bounds a run to 100
frames unless `--frames` overrides it. `--camera YAW,PITCH,DIST` initializes the
viewport camera in degrees. `--check-black-frames` checks the accepted GPU
frame's center pixel; it is a targeted black-frame check, not an image quality
metric. `--dump-layout[-file]` and `--dump-render-graph[-file]` export the actual
retained UI tree and compiled frame plan as JSON.

```sh
odin run tools/build -- run --no-build -- \
  --headless --frames 100 --camera 25,-15,8 \
  --dump-layout-file target/odin-editor-proof/layout.json \
  --dump-render-graph-file target/odin-editor-proof/graph.json \
  --screenshot target/odin-editor-proof/headless.png
odin run tools/build -- run --no-build -- \
  --interaction-test target/odin-editor-proof/interaction
```

`--ui-test DIR` and `--interaction-test DIR` run retained UI/input journeys
against actual offscreen editor frames and write PNGs and check receipts. They
select headless mode; native window, IME and OS input acceptance requires a
separate windowed run. Omitting `--scene` discovers the selected project's
`assets/scenes/default.katla`; an explicit scene uses the confined resolver.
Conversation resume belongs to persisted host/preferences state rather than a
CLI resume flag. Local source builds, portable checks and workflow definitions
must be distinguished from completed CI and native device receipts.
