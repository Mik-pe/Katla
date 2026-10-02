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
- runs the complete `katla_gfx` library test suite;
- detects unsupported GPU capabilities before issuing invalid Objective-C or Metal calls;
- returns typed errors instead of aborting across the Objective-C/Rust boundary.

Pixel-accurate rendering and performance validation should use a physical, self-hosted Apple Silicon runner when one is available. The hosted `macos-26` job remains the required current-SDK validation environment.

## Cross-backend contract suite

Both jobs run the shared contract suite (`katla_gfx/tests/contract/`) against
the platform's native backend — Vulkan on lavapipe with the Khronos validation
layers installed, Metal on Apple Silicon with `MTL_DEBUG_LAYER=1` and
`METAL_DEVICE_WRAPPER_TYPE=1`:

```bash
cargo test -p katla_gfx --test contract --locked -- --ignored
```

The suite asserts the contracts Katla promises above the backend boundary
(render results, resource lifetime, error semantics) through one
backend-neutral harness. See `docs/contract-suite.md` for the full guide and
the harness module docs before adding a scenario.

## Failed render-graph artifacts

Both jobs upload `target/render-graph-diagnostics/` as an artifact when the job
fails (`actions/upload-artifact`, 7-day retention, no `if-no-files-found`
error). A drifting golden snapshot writes the actual export there before
asserting, so a failing run ships the export that disagreed with the checked-in
snapshot — no local re-run needed to see the difference. Capture the same
artifact locally with `--dump-render-graph-file`; see
`docs/render_graph_capture.md`.

## Local equivalents

```bash
cargo fmt --all -- --check
cargo check -p katla_gfx -p katla_app --locked
cargo test -p katla_gfx --lib --locked
cargo clippy -p katla_gfx -p katla_app --locked -- -D warnings
cargo test -p katla_gfx --test contract --locked -- --ignored
```

Linux separately validates the graphics library and Vulkan path on Ubuntu 24.04.
## Linux apt source hygiene

GitHub's `ubuntu-24.04` runners preinstall Google's Chrome apt source. Its
upstream metadata intermittently arrives hash-mismatched, which fails
`apt-get update` before any build step runs. The Linux graphics job removes
that source before updating — no Katla job uses Chrome.

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

Graphics library tests execute real buffer readback for neutral direct/indirect
compute, same-pass command chains, animation interpolation/rest poses and a small
particle pool on the platform's backend. Linux requires an active Khronos
validation messenger with synchronization validation enabled; CI runs native
library fixtures serially to avoid lavapipe instance creation races. A separate
Vulkan transient-alias integration step queues both frame slots across eight
resize/rebuild cycles and checks validation messages and pixel contents.

The macos-26 graphics library step runs with both `MTL_DEBUG_LAYER=1` and
`METAL_DEVICE_WRAPPER_TYPE=1`. Native tests cover three-slot ownership, stream
replacement, UI isolation, private texture sampling, placement heaps, timestamp
readback and frame aborts. Compilation or a screenshot alone is insufficient
for these output/lifetime assertions.
