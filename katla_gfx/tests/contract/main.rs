//! Cross-backend graphics contract suite (#97).
//!
//! One suite runs the same scenarios against the platform's native backend:
//! Vulkan on Linux (CI runs it on lavapipe), Metal on macOS (CI runs it on
//! Apple Silicon with Metal validation enabled). Scenarios assert the contract
//! Katla promises above the backend boundary — rendering results, resource
//! lifetime and error semantics — never private backend state, so a failure
//! names the divergent contract, not a backend struct.
//!
//! ## Adding a contract test
//!
//! 1. Open a renderer with [`harness::ContractRenderer::open`] (API validation
//!    captured) or `open_without_api_validation` (required for PBR pipelines —
//!    see the harness docs for the driver caveat).
//! 2. Write the scenario against [`harness::ContractRenderer::gfx`] (the
//!    backend-neutral `AnyRenderer` + `GpuRenderer` trait) and
//!    [`harness::ContractRenderer::render_frame`] + [`harness::build_graph`].
//!    Never name `VulkanRenderer`/`MetalRenderer` in a scenario.
//! 3. Assert observable results: readback pixels (`B8G8R8A8`, row 0 = top on
//!    both backends), typed `RendererError` variants, or public query methods.
//!    Prefer probes that are robust to a vertical flip (center pixels,
//!    left/right halves, whole-frame scans) so the same expectation holds on
//!    both backends.
//! 4. If the backends legitimately differ, extend [`harness::Capabilities`]
//!    and branch on the capability — never on `cfg!(target_os)` — then end the
//!    test with [`harness::ContractRenderer::finish`], which cleans up and
//!    asserts the captured API validation log is empty.
//!
//! Run: `cargo test -p katla_gfx --test contract -- --ignored`

mod errors;
mod graphics;
mod harness;
mod resources;
