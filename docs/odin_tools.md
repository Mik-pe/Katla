# Odin development tools

Katla builds, launches, validates, exports shaders and authors scenes through
Odin programs. Python, CMake, Ninja and make are not dependencies of these
commands. The small shell/PowerShell compiler bootstrap runs before Odin is
installed; Clang/Clang++, the archiver, Git, curl and the isolated Cargo shader
compiler remain explicit external tools.

## Build and acceptance

```sh
odin run tools/build -- --tests
odin run tools/build -- --sanitize --tests
odin run tools/build -- run --no-build -- --frames 100
odin run tools/build -- validate processes --sanitize \
  --build-manifest target/katla-odin/darwin-arm64/asan/build.json
odin run tools/build -- validate gpu --native-metal --sanitize \
  --build-manifest target/katla-odin/darwin-arm64/asan/build.json
odin run tools/build -- validate render --native-metal --sanitize \
  --build-manifest target/katla-odin/darwin-arm64/asan/build.json
odin run tools/build -- validate ui --native-metal --sanitize \
  --build-manifest target/katla-odin/darwin-arm64/asan/build.json
```

Without `--build-manifest`, validation first runs the canonical build. An
explicit manifest must belong to this checkout, host, architecture and sanitizer
mode, contain the required native artifacts and match every artifact hash.
`--output-dir DIR` retains validator executables, logs and captures independently
of the reused build. No native library overrides bypass manifest verification.

| Suite | Actual acceptance |
| --- | --- |
| `processes` | HTTP/TLS/SSE, MCP stdio, two-proxy shared world, private host connection and proxy lifecycle |
| `http` | Responses/chat, exact tool IDs, scene save/load, TLS rejection, cancellation, rate limits, malformed/truncated/oversized streams and parallel jobs |
| `mcp` | Discovery/tool schemas, Unicode scene/material mutations, exact integer IDs, fragmented/pipelined frames, recovery, EOF drain and broken output |
| `socket` | Two real proxy processes share one scene owner and resource writes across disconnect |
| `host` | Existing-thread initialize/list/resume/read/start/steer/interrupt plus disconnect and invalid responses |
| `proxy` | More than 3 MiB byte identity, half-close drain, peer EOF, output failure and private path admission; `--slow` adds stalled-output deadline |
| `gpu` | CPU graph/SPIR-V/shader contracts and explicitly selected native backends |
| `render` | CPU renderer and real material/model/precision consumers; `--particles` requires both backends |
| `ui` | CPU graph/fonts, actual typography/picking and visible thumbnails; `--cpu-only` omits native launches |
| `shader` | Locked compiler fmt/test/clippy and reflection; `--native --native-metal --native-vulkan` adds replacement rendering |
| `physics`, `luau` | Native Box3D, script and application consumers from the source-pinned build |
| `audio` | Codec/mixer/application consumers; `--native` exercises the actual output device and `--switch-default` tests macOS route replacement |

Native Vulkan accepts `--vulkan-library FILE` and `--vulkan-icd FILE`.
`--vulkan-baseline` runs non-array contracts. A separate
`--vulkan-probe-array-capabilities` invocation exits 77 only for unsupported
array capability; other failures retain failing exit codes. Baseline acceptance
must run before the probe. `--glslc FILE` selects the independent GLSL compiler.
Native Metal requires actual macOS arm64 hardware and always enables API
validation. Native application/UI harnesses currently require Darwin arm64.
Unix host/proxy fixtures explicitly reject unsupported Windows transport.

CPU ASan retains leak checks, including native C/C++ consumers. Native GPU
launches disable external framework/driver process-exit leaks while retaining
address and Odin ownership checks. `--native-leaks` explicitly restores the
external GPU audit. Native audio retains leaks by default; the explicit
`--native-asan-leaks external-driver` mode applies only with `--native`.
Deterministic transport fixtures never establish paid-model acceptance or
attachment to a human conversation.

## Shader export

```sh
odin run tools/build -- shader-export \
  --source odin/gfx_shader_native/shaders/array.wgsl \
  --compiler target/katla-odin/darwin-arm64/normal/shader-compiler/debug/katla-shader-compiler \
  --entry main --stage Fragment --output target/shader-export/array
```

The existing bounded compiler protocol validates the selected entry. Export
writes ABI-1 JSON/reflection, MSL and little-endian SPIR-V beside the basename.

## Authoring and fixtures

```sh
odin run tools/author -- room --dry-run --name Study --size 6 3 8 --ceiling
odin run tools/author -- room --socket /private/editor.sock \
  --proxy /absolute/build/bin/katla-mcp-proxy --name Study --size 6 3 8
odin run tools/author -- validate --socket /private/editor.sock \
  --proxy /absolute/build/bin/katla-mcp-proxy --project /absolute/disposable-project \
  --output /absolute/disposable-project/proof
```

The same transport supports `view`, `shared-view`, `furnish` and `prefabs`.
`--binary FILE` explicitly selects a standalone stdio scene owner instead of a
private socket. Only native editors supply paired GPU captures. Room operations
unwind only their own accepted chronological edits on a tool rejection. Do not
interleave concurrent edits during a recipe. `--save FILE` explicitly publishes
a scene; room construction otherwise leaves it unsaved. The validation journeys
replace scenes and belong in disposable editor/project copies. The material
journey waits for retained transient GPU state to settle before exact RGB
comparison and verifies zero changed pixels after Undo.

```sh
odin run tools/provider -- --config /private/llm.toml --scenario editor_scene \
  --stop-file /private/stop --receipt /private/provider.json
odin run tools/native_host -- --socket /private/host.sock \
  --receipt /private/host.json --hold-second --stop-file /private/stop
odin run tools/fixtures -- models target/generated-models
odin run tools/fixtures -- audio target/generated-audio
```

Providers bind loopback only. Configs, existing-host receipts and screenshots
are owner-private; Unix socket parents must already be owner-only. Both fixture
servers have bounded `--seconds` lifetimes. Creating the stop file drains
connections and cleans up the owned endpoint. The existing-host fixture keeps
one selected thread across reconnect/start/steer/interrupt and validates actual
PNG dimensions and hashes. HTTP fixtures record scenario request counts.
Generated models retain pinned normal textures/material policy from the
checked-in fixtures; geometry/accessors are generated in Odin. Optional audio
fixture regeneration requires FFmpeg's native Vorbis, MP3 and FLAC encoders.
