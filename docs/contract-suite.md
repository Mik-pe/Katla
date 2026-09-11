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
cargo test -p katla_gfx --test contract -- --ignored
```

The scenarios need a graphics device, so they are `#[ignore]`d like the other
device suites. On macOS, enable Metal API validation the way CI does:

```bash
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
    cargo test -p katla_gfx --test contract -- --ignored
```

Select scenarios by name substring: `-- --ignored test_contract_instanced`.

## Adding a scenario

1. Open a renderer with `ContractRenderer::open` (API-validation capture) or
   `ContractRenderer::open_without_api_validation`.
   PBR-material scenarios **must** use the latter: compiling a PBR pipeline
   under the Khronos validation layer segfaults the Intel driver on the
   canonical Linux machine. UI-material scenarios capture validation errors
   and `finish()` asserts the log stays empty.
2. Write the scenario against `renderer.gfx()` — the backend-neutral
   `AnyRenderer` and the `GpuRenderer` trait — plus `harness::build_graph`,
   `render_frame`, and `pass_id`. Never name `VulkanRenderer` or
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
