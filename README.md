# Katla

![Katla](assets/katla-logo.svg)

Katla is a game engine and scene editor written in Odin, with native Metal 4
and Vulkan rendering. Its application owns one ECS world, scene documents,
shared undo history, gameplay and editor composition.

The editor includes dockable and floating panels, component and material
inspection, transform gizmos, asset and prefab authoring, skeletal animation,
GPU particles, Luau scripting, Box3D physics and native audio. Agents use the
same application owner through MCP and receive frame-bound viewport context.

## Build and run

Install Odin, Cargo, Git, curl and a C/C++ compiler with an archiver. Cargo builds the
isolated offline Naga shader compiler; the editor and engine runtime use Odin
and explicitly pinned C/C++ dependencies. Platform prerequisites and verified
Odin installation are documented in [Build and launch](docs/odin_build.md).

```sh
odin run tools/build -- --tests
odin run tools/build -- run --no-build
```

The launcher selects Metal on macOS and Vulkan elsewhere, sets Metal API
validation, and loads the native dependencies from the verified build manifest.
Use `--backend vulkan --vulkan-loader /path/to/libvulkan` to select Vulkan
explicitly. Application arguments follow `--`:

```sh
odin run tools/build -- run --no-build -- --frames 100
odin run tools/build -- run --no-build -- --headless --frames 3 --screenshot /tmp/katla.png
odin run tools/build -- run --no-build -- --scene assets/scenes/default.katla
```

Shader sources ship with the build. Runtime freshness checks request compilation
only when source, options or compiler identity change; failed compilation retains
the last accepted pipelines. UI layout runs in Odin. The engine has no Rust
runtime or layout bridge.

## Validation and documentation

[Documentation](docs/README.md) is organized by task. Start with
[architecture](docs/architecture.md), [editor behavior](docs/odin_editor.md) and
[CI policy](docs/ci.md). Git and GitHub record delivery; [TODO](TODO.md) records
unresolved work.

Katla tooling is native Odin; all build, launch, validation and authoring commands run in Odin.

```sh
odin run tools/build -- validate processes --sanitize
odin run tools/build -- validate gpu --native-metal
odin run tools/build -- validate gpu --native-vulkan --vulkan-library /path/to/libvulkan
```

Native rendering requires the actual device and driver. CPU tests and screenshots
have distinct scopes; [CI policy](docs/ci.md) explains hardware boundaries and
sanitizer evidence. Live scene construction and material editing use the
[agent authoring guide](docs/agent-authoring.md).
