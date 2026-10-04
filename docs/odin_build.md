# Build and run Katla in Odin

The canonical source build is `scripts/build_katla_odin.py`. It compiles the
Odin editor, the MCP stdio/proxy programs and their native dependencies.
Linux and Windows also build the pinned SDL3 window/input dependency. It does
not build or load any Rust engine crate. Cargo builds only the locked,
standalone offline WGSL compiler in `tools/naga_bridge`; that executable is a
build tool invoked by the shader cache, not a runtime FFI library.

```sh
python3 scripts/build_katla_odin.py --tests
python3 scripts/run_katla_odin.py --no-build
```

Normal builds use Odin's speed optimization and retain debug symbols. Sanitized
builds remain unoptimized by default; add `--optimize` to instrument an optimized
editor as well.

Python 3.12+, Odin, Git, a native C/C++ compiler, an archiver and Cargo are
required. Unix TIFF builds also require make and system zlib development
headers; Windows requires Clang/LLVM, a configured Windows SDK, CMake and
Ninja. Font builds download pinned source archives and fallback fonts. No
preinstalled engine library, Cargo engine target or generated shader binary is
required.

For a verified compiler installation:

```sh
python3 scripts/build_katla_odin.py --install-odin target/odin-toolchain
```

This downloads the host's `dev-2026-09` release from the
[Odin release](https://github.com/odin-lang/Odin/releases/tag/dev-2026-09),
checks its repository-recorded SHA-256 and preserves the compiler's core/vendor
collections. Add the printed executable's parent directory to PATH, or supply
`--odin /absolute/path/to/odin`. Changing the compiler pin requires changing
all corresponding archive digests together.

The output is `target/katla-odin/<host>-<arch>/<normal|asan>`. Each directory owns
its native libraries, fonts, compiler executable, binaries, canonical WGSL
sources in `shaders/` and `build.json`. Shader sources come exclusively from
`odin/app/render/shaders`, including relative include files; their hashes enter
the same manifest as native artifacts. The launcher supplies `--shader-root`
with that shipped directory. The old Rust shader asset directory is not used.
The manifest is published only after every requested build/test succeeds; a
failed rebuild removes the previous successful manifest. `run_katla_odin.py`
checks the host, architecture, checkout, sanitizer mode and every recorded
artifact hash before launching. `--no-build` explicitly reuses compiled source;
run the build again after source changes. The launcher normally rebuilds first.
A directory cannot be reused for another host or sanitizer mode.

A bounded validated Metal run with actual retained viewport GPU readback is:

```sh
mkdir -p target/odin-editor-proof
python3 scripts/run_katla_odin.py --no-build -- \
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
python3 scripts/run_katla_odin.py --no-build --backend vulkan \
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
python3 scripts/build_katla_odin.py --sanitize --tests
python3 scripts/run_katla_odin.py --no-build --sanitize -- --frames 100
```

CPU suites run serially with bad-memory failures enabled, including real
native decoder, physics, Luau, shader-process and font consumers. Explicit
package runs cover app, app/render, app/editor, gfx, shader tests, script,
audio, UI, precise image codecs, resources, agent/host and native fonts. Their ASan
processes always enable leak detection, even if the invoking environment sets
`detect_leaks=0`. On Darwin the only suppressions are the observed Apple
`CFPrefsPlistSource` and `CFPrefsSearchListSource` process-cache stacks;
application and dependency ownership remains
checked. This CPU policy does not change native GPU or audio-driver validation
environments. Native GPU and audio-device validation remain explicit:

```sh
python3 scripts/validate_odin_gpu.py --native-metal --sanitize
python3 scripts/validate_odin_audio.py --native --switch-default
```

Native GPU ASan launches check addresses and Odin ownership while excluding
process-exit leaks from retained Apple CF/ObjC/driver objects. CI applies that
boundary only to native launches; CPU processes retain leak detection. Audio
device validation has its own explicit boundary documented in
[audio_odin.md](audio_odin.md).

On Linux, use `validate_odin_gpu.py --native-vulkan --vulkan-library
/usr/lib/x86_64-linux-gnu/libvulkan.so.1 --vulkan-icd /path/to/lvp_icd.json`,
substituting the host's actual loader file and ICD manifest. Vulkan validation must be
available. Resource-array fixtures require actual descriptor-indexing features;
an unsupported device does not establish array acceptance.

| Dependency | Canonical builder/source pin |
|---|---|
| WGSL compiler | Locked `tools/naga_bridge/Cargo.toml` and Cargo.lock; Naga 29.0.1 |
| Physics | `scripts/build_box3d.py`; Box3D v0.1.0 commit `8441b4a06d6d09dcfb0b0f704df4d847d1437b92` |
| Script VM | `scripts/build_odin_luau.py`; Luau 0.709 commit `b968ef742741bb2b703afc3b3c53f06608c87481` |
| Audio | `scripts/build_odin_audio.py`; miniaudio 0.11.25 and verified repository stb_vorbis |
| glTF | `scripts/build_odin_gltf.py`; repository-pinned cgltf C source |
| Image codecs | `scripts/build_odin_image.py`; repository stb_image, SHA-256 pinned IJG 9f/TIFF 4.7.2; Windows also [pinned zlib 1.3.2](https://zlib.net/) |
| Typography | `tools/font_native/build.py`; exact FreeType/HarfBuzz/SheenBidi/Unibreak commits and hashed fallback fonts |
| Window/input | `tools/window_native/build.py`; SDL3 3.2.28 commit `7f3ae3d57459e59943a4ecfefc8f6277ec6bf540` on Linux/Windows |
| Preferences | `scripts/build_odin_toml.py`; repository-pinned tomlc17 C source |

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
development packages, xkbcommon, wayland-protocols, D-Bus, IBus and pkg-config;
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
python3 scripts/run_katla_odin.py --no-build -- \
  --headless --frames 100 --camera 25,-15,8 \
  --dump-layout-file target/odin-editor-proof/layout.json \
  --dump-render-graph-file target/odin-editor-proof/graph.json \
  --screenshot target/odin-editor-proof/headless.png
python3 scripts/run_katla_odin.py --no-build -- \
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
