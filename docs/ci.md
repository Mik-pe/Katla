# Continuous Integration

Katla uses explicit runner labels so operating-system and Metal SDK changes are intentional and reviewable.

## macOS policy

Katla supports exactly one macOS generation in CI:

| Role | GitHub Actions label | Architecture | Purpose |
|---|---|---|---|
| Current | `macos-26` | Apple Silicon (`arm64`) | Current macOS, Xcode, Metal SDK, tests, checks, and Clippy |

There is no backwards-compatible macOS job. Do not add `macos-15`, `macos-14`, or another older macOS runner as a compatibility matrix entry.

Do not use `macos-latest`. It is a mutable alias and can change independently of Katla's explicit platform decision.

## Runner upgrade policy

When Katla adopts a newer generally available macOS runner:

1. Replace the current explicit runner label with the new explicit label.
2. Update this document and `AGENTS.md` in the same change.
3. Validate the complete Metal, graphics-library, and application checks on the new runner.
4. Do not retain the previous macOS generation as a compatibility job.

Katla intentionally follows the current macOS and Metal platform rather than maintaining an operating-system compatibility matrix.

## Metal validation limits

GitHub-hosted macOS runners may expose a virtualized Metal device with fewer capabilities than physical Apple Silicon hardware. CI must still verify that Katla:

- compiles against the selected current macOS and Xcode environment;
- runs all device-independent library tests and shader/reflection checks;
- runs the full native library and contract suites when the exposed device supports `MTLGPUFamilyMetal4`;
- detects unsupported GPU capabilities before issuing invalid Objective-C or Metal calls;
- returns typed errors instead of aborting across the Objective-C/Rust boundary.

The job probes the default device before testing. A virtual GPU without Metal 4 receives an explicit **BLOCKED** native-acceptance notice in the job summary; the typed capability-rejection regression still runs. The native GPU exclusion manifest is `katla_gfx/tests/metal4-required-tests.txt`. It excludes actual device-dependent fixtures, including capture, range preflight, core-only construction and retained readback. Reflection and binding layouts, cache metadata, synchronization, submission feedback, graph-only attachment checks and typed capability rejection remain runnable. Add new native fixtures to that manifest; do not disable an entire mixed module or pin documentation to inventory counts. This is a hardware limitation, not native GPU acceptance. Physical Apple Silicon validation is recorded separately in `metal4_validation.md`. No legacy command path or older macOS runner is introduced.

## Cross-backend contract suite

Linux runs the shared contract suite (`katla_gfx/tests/contract/`) against Vulkan
on lavapipe with active Khronos validation. The macOS job runs it against Metal
only when the default GPU supports Metal 4, with `MTL_DEBUG_LAYER=1` and
`METAL_DEVICE_WRAPPER_TYPE=1`:

```bash
cargo test -p katla_gfx --test contract --locked -- --ignored --test-threads=1
```

The suite asserts the contracts Katla promises above the backend boundary
(render results, resource lifetime, error semantics) through one
backend-neutral harness. See `docs/contract-suite.md` for the full guide and
the harness module docs before adding a scenario.

## Failed render-graph artifacts

Both jobs upload `target/render-graph-diagnostics/` as a plan/execution artifact
when the job fails (`actions/upload-artifact`, 7-day retention, no `if-no-files-found`
error). A drifting golden snapshot writes the actual export there before
asserting, so a failing run ships the export that disagreed with the checked-in
snapshot. Native capture fixtures also write the joined capture, compiled plan,
actual execution, text and DOT when comparison fails. Workflow permissions remain
read-only. Capture the same artifact locally with `--dump-render-graph-file`; see
`docs/render_graph_capture.md`.

## Local equivalents

```bash
cargo fmt --all -- --check
cargo check -p katla_gfx -p katla_app --all-targets --locked
cargo test -p katla_gfx --lib --locked -- --test-threads=1
cargo clippy -p katla_gfx -p katla_app --all-targets --locked -- -D warnings
cargo test -p katla_gfx --test contract --locked -- --ignored --test-threads=1
```

Checks and strict Clippy include graphics examples, tests and benchmarks through
`--all-targets`. The application also compiles all targets without default features, so editor-only helpers cannot leak into GraphOnly builds. Linux separately validates the graphics library and Vulkan path
on Ubuntu 24.04. On lavapipe, use the contract command with
`--skip graphics::pbr` as in CI; the full contract suite remains a physical-device
acceptance command.

## Linux apt source hygiene

GitHub's `ubuntu-24.04` runners preinstall Google's Chrome apt source. Its
upstream metadata intermittently arrives hash-mismatched, which fails
`apt-get update` before any build step runs. The Linux graphics job removes
that source before updating — no Katla job uses Chrome.

## Animation transitions

The shared MCP/editor `animation` request is covered by application and agent
library tests with all features enabled. Fixed delta times exercise the real
typed playback system, completion events, pause, target looping and scene
snapshot restoration. The Linux graphics job also runs the ignored
`animation::native_transition_tests` fixture on Vulkan with active validation:
an agent request advances the player and its ordinary graph parameters, then
GPU joint-matrix readback checks the start, midpoint and completed target.
The `macos-26` job runs the same fixture with Metal API validation after its
Metal 4 capability probe. A skipped native Metal fixture means hardware
acceptance remains blocked; Linux output and CPU tests still run.

```bash
cargo test -p katla_app -p katla_agent --lib --all-features --locked
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_app --lib native_transition_tests --all-features --locked -- --ignored --test-threads=1
```

See the [transition contract](animation-transitions.md) for timing and agent
request semantics.

## ECS ownership checks

The existing Linux and explicit `macos-26` jobs check, test and lint the ECS with
all features, including doctests and its CPU-only benchmarks. The macOS job also
runs application and script library tests for the typed/exclusive migration.
The separate Ubuntu Miri job uses pinned `nightly-2026-08-04` for query and
parameter pointer boundaries, filters, allocator generations, stale sparse keys
and public entity lifecycle tests. No extra macOS generation or runner is introduced.

```bash
cargo test -p katla_ecs --all-features --locked
cargo clippy -p katla_ecs --all-targets --all-features --locked -- -D warnings
cargo +nightly-2026-08-04 miri test -p katla_ecs --lib typed_query::tests --locked
cargo +nightly-2026-08-04 miri test -p katla_ecs --lib params::tests::test_param --locked
```

Miri covers CPU reference provenance and lifetimes; native parallel tests cover
actual worker overlap and ordering. Native GPU/window checks remain part of
application acceptance and are distinct from these CPU checks.

## Native graph and frame ownership acceptance

Both backends execute graph-owned generic compute, explicit resource bindings,
constants, transfer commands and declared color/depth attachments. Shader
reflection and interface checks happen during preparation. Headless constructor
fixtures prove that core construction installs no animation, particle, lighting,
shadow, picking or built-in compute service; application services build ordinary
graph passes for those workloads.

The native capture regressions run the same compute, transfer and graphics
workload with capture disabled and enabled. They assert identical buffer and
pixel outputs and actual encoder, synchronization, binding and submission work,
then compare the captured native scopes with the compiled contract. Capture
feedback snapshots do not wait for or retire pending submissions.

Linux requires active Khronos validation with synchronization validation enabled.
Native library fixtures run serially to avoid lavapipe instance creation races.
Separate steps cover transient aliasing and resize cycles, and the ignored
Vulkan capture/retained-readback fixtures:

```bash
cargo test -p katla_gfx --test transient_aliasing --locked -- --ignored --test-threads=1
cargo test -p katla_gfx --lib renderer::capture_tests --locked -- --ignored --test-threads=1
```

When Metal 4 is supported, the `macos-26` library step uses `MTL_DEBUG_LAYER=1`
and `METAL_DEVICE_WRAPPER_TYPE=1`. It covers capture equivalence, three-slot
ownership, streamed resource replacement, retained graph readback, UI isolation,
private textures, placement heaps, timestamp readback and frame aborts. The
native capture regression can also run directly:

```bash
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_gfx --lib metal::capture_tests --locked -- --test-threads=1
```

Compilation and portable tests establish the API contract; native output and
lifetime assertions establish GPU acceptance on a capable device.

## Trigger and script event checks

The Linux and `macos-26` jobs run actual Rapier overlap/enter/exit regressions,
agent rule validation and Luau callback command/lifetime/next-tick tests. Native
application construction additionally verifies rule persistence and generational
reference remapping; the Metal fixture follows the existing Metal 4 capability
gate and API-validation environment. See [scene events](scene-events.md).

## Scene format and document lifecycle

Linux runs the portable scene format tests and native document regressions with
Khronos validation. Native tests cover staged load failures, resource cleanup,
stable keys, custom component codecs, particle and joint restoration, Save As
asset origins and Play/Stop snapshots. The `macos-26` job runs the shared native
document suite when the existing Metal 4 capability gate passes, with API
validation enabled. Unsupported Metal hardware remains blocked rather than
counting as native acceptance.

```bash
cargo test -p katla_app --lib --all-features --locked scene
RUST_LOG=info cargo test -p katla_app --lib document_tests --all-features --locked -- --ignored --test-threads=1
```

## Mesh and prefab authoring

Linux runs portable mesh/prefab tests and `prefab::native_tests` serially with
Khronos validation. The capability-gated `macos-26` document step runs the same
native prefab fixture with Metal API validation. It covers GPU geometry sharing,
AI authoring and atomic persistence, capture/reference remapping, failed staging
rollback, resource retirement and scaled mesh collider reconstruction. See
[the prefab contract](prefabs.md#verification) for commands and rendering evidence.

The extended native interaction walkthrough checks prefab asset double-click and
toolbar Play/Stop in addition to material/component edits. Linux and capability-gated `macos-26` also build the editor and run the disposable
`scripts/validate_prefabs.py` acceptance flow uses real MCP and committed viewport
readbacks to verify script attachment, particles, trigger visitor remapping,
preview controls and capture/save/reload; commands are in [prefabs](prefabs.md#verification).

## Material imports and neutral textures

The graphics library suite includes strict native sampler probes with small
shaders. They verify min/mag filtering, mip selection/interpolation, repeat/clamp/
mirrored coordinates, anisotropy, full and selected imported mip views, and all
eight depth comparisons with below/equal/above references. Multiple viewports
within one pass verify phase sampler overrides and restoration of the base policy.
They run in Linux's graphics library tier and Metal's capability-gated native tier;
they do not compile the full scene PBR shader. Physical Vulkan acceptance passes;
physical Metal evidence remains outstanding.
The same tier checks independent UV0/UV1 values in static and skinned vertex
readbacks, including joint/weight locations and the submitted mesh buffer order.

Linux runs the portable `gltf_` and `util::modelcache` application tests; macOS's portable application
suite includes them too. The existing Metal 4 capability-gated document step
also runs the native `material_tests` fixtures, including static/skinned
per-primitive surfaces, textureless/textured emission, scaled normals, occlusion
strength, alpha coverage/compositing across color/depth/shadow/picking, mirrored
faces, integer/HDR image fidelity, filtered mip regeneration, weak image sharing,
independent per-role UV transforms and imported samplers, generated/authored/
changed normal-coordinate bases, transformed alpha coverage,
second-skin selection, scene round-trip/rollback (including unavailable UV
selection in persisted sampling overrides) and
resource retirement. The complete PBR lighting fixture executes both actual
scene shaders and checks 120 HDR pixels against a double-precision reference:
identity/nonuniform/mirrored frames, CPU baking/live model/skin/combined transforms,
roughness, metallicity, multiple lights, occlusion, shadow visibility, scaled normals
and HDR emission. Its flat-normal regression requires bit-identical pixels for
normal scale zero and one when no normal map is assigned. The primitive probes use
API validation. Full PBR compilation/execution fixtures disable Vulkan validation on the affected Intel driver; Metal
requires its debug environment.
Physical Vulkan acceptance runs the same fixture locally. It compiles the scene
PBR shader, so it is not added to the lavapipe tier whose PBR compiler limitation
is documented in the [contract suite](contract-suite.md).

```bash
cargo test -p katla_app --lib gltf_ --all-features --locked
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo test -p katla_app --lib material_tests --all-features --locked -- --ignored --test-threads=1
```

The fixture checks real imported drawable factors and samples uploaded neutral
normal/MR textures and shared color/data images through a native shader, then
asserts readback values. See the [material contract](../katla_gfx/src/material/API.md)
for current import and surface-feature limits. Its Vulkan PBR compilation disables
Khronos validation because of the known Intel compiler crash; passing this fixture
does not establish validation-layer acceptance or native Metal acceptance on a
machine without a Metal 4 device.

Linux CI runs the dedicated image-fidelity and submission/early-retirement
fixtures with native Vulkan validation. They use small shaders and are independent
of the full PBR compiler. Full-asset timing evidence is retained in the
[import study](material-import-study/README.md); its opt-in benchmark does not
assert machine-dependent performance thresholds.

Local submission/retirement acceptance:

```bash
cargo test -p katla_gfx --lib test_native_image_upload_failures --locked -- --ignored --nocapture
```
