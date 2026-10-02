# Cross-Backend Graphics Contract Suite

One suite asserts the contracts Katla promises **above** the backend boundary.
It lives in `katla_gfx/tests/contract/` and runs the same scenarios against
whichever backend the platform provides: Vulkan on Linux, Metal on macOS. CI
runs it in both jobs; failures name the divergent contract, not a backend
struct.

## Why

Backend-specific suites (`dynamic_mesh_updates.rs`, `attachment_semantics.rs`,
…) prove each backend against its own expectations. The contract suite proves
the promise app code actually builds on: the same scenario must render the
same pixels, enforce the same resource-lifetime rules, and fail with the same
typed errors everywhere. Backend-specific suites stay; this is the layer
above them, not a replacement.

## Running

```bash
cargo test -p katla_gfx --test contract --locked -- --ignored --test-threads=1
```

The scenarios need a graphics device, so they are `#[ignore]`d like the other
device suites. On macOS, enable Metal API validation the way CI does:

```bash
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
    cargo test -p katla_gfx --test contract --locked -- --ignored --test-threads=1
```

Select a scenario with a Cargo filter, for example:

```bash
cargo test -p katla_gfx --test contract test_contract_instanced --locked -- --ignored --test-threads=1
```

CI runs Linux scenarios on lavapipe with `--skip graphics::pbr`, because that
software driver's PBR compilation path crashes. Physical Vulkan acceptance runs
the full suite. The macOS job runs native scenarios only when its default GPU
supports Metal 4; unsupported hosted GPUs still run portable library tests and
typed capability rejection, and report native acceptance as blocked.

## Adding a scenario

1. Open a renderer with `ContractRenderer::open` (API-validation capture) or
   `ContractRenderer::open_without_api_validation`.
   PBR-material scenarios **must** use the latter: compiling a PBR pipeline
   under the Khronos validation layer segfaults the Intel driver on the
   canonical Linux machine. UI-material scenarios capture validation errors
   and `finish()` asserts the log stays empty.
2. Write the scenario against `renderer.gfx()` — the backend-neutral
   `AnyRenderer` and the `GpuRenderer` trait — plus `harness::build_graph`,
   `render_frame`, and `pass_id`. Pass resources, constants, samplers, draw or
   dispatch phases and explicit color/depth targets belong to graph declarations
   and binding packets. Renderer construction supplies device/frame/resource
   primitives; the harness supplies scenario data. Never name `VulkanRenderer` or
   `MetalRenderer` in a scenario.
3. Assert observable results only: readback pixels (BGRA, row 0 = top on both
   backends), typed `RendererError` variants, or public query methods.
   Prefer probes that survive a vertical flip — center pixels, left/right
   splits, whole-frame scans — so one expectation holds on both backends.
4. If the backends legitimately differ, extend `harness::Capabilities` and
   branch on the capability. Never branch on `cfg!(target_os)` inside a
   scenario; platform `#[cfg]` belongs to the harness alone.
5. Tear down with `harness::cleanup_graph(graph)` and then
   `renderer.finish()`.

## Conventions

- Test names start with `test_contract_`, prefixed by the scenario module
  (`graphics`, `resources`, `errors`).
- Scenario failures must be actionable: the assertion message names the
  contract that broke ("the recycled slot's new owner must be resolvable"),
  not the pixel that mismatched.
- The suite stays cheap enough for PR CI (small targets, few frames, one
  test binary); heavy stress lives in the backend-specific validation tier.
- When extending the public gfx API, ask: which of these promises does the
  new API touch, and does the suite already pin it? If not, add the scenario
  in the same PR.

## Covered contracts

Graphics scenarios assert indexed widths, object transforms, direct/instanced
draw equivalence, dynamic mesh lifecycle, material rendering across graph
configurations, emitted attachment contracts and load/clear behavior. Resource
scenarios exercise stale texture generations, idempotent destruction, retirement
after frame drain and independent frame slots; error
scenarios assert typed rejection without disturbing live resources or producing
validation errors.

The harness acquires a frame token, installs explicit graphics data, renders the
graph and consumes the token at present. It reads the committed graph export
through a retained texture source and readback ticket, rather than a renderer
scene attachment. Capabilities describe legitimate backend differences; optional
API details stay in the harness.

Native backend capture regressions complement this suite with actual encoder,
synchronization scope, binding, residency and submission observations. They
compare capture disabled/enabled GPU outputs and write plan/execution artifacts
on divergence. Portable diagnostic fixtures cover deterministic JSON, text and
DOT, culling/allocation identities and deliberate mismatch detection. See
[render-graph capture](render_graph_capture.md) and [CI](ci.md) for those commands
and the native-device boundary.
