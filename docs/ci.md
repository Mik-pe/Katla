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
