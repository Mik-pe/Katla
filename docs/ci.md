# Continuous Integration

Katla uses explicit runner labels so operating-system and Metal SDK changes are intentional and reviewable.

## macOS matrix

| Role | GitHub Actions label | Architecture | Purpose |
|---|---|---|---|
| Primary | `macos-26` | Apple Silicon (`arm64`) | Current macOS, Xcode, Metal SDK, tests, checks, and Clippy |
| Compatibility | `macos-15` | Apple Silicon (`arm64`) | Verify that supported code still builds and the graphics test suite runs on the previous macOS generation |

Do not use `macos-latest`. It is a mutable alias and can move independently of Katla's support policy. It currently does not mean macOS 26.

Do not add `macos-14` back to required CI. Sonoma is in GitHub's runner-image deprecation cycle and is not part of Katla's maintained CI baseline.

## Runner upgrade policy

When GitHub makes a new macOS image generally available:

1. Add the new explicit label as the primary job.
2. Keep the previous primary image as the compatibility job.
3. Validate the full Metal and application checks on both images.
4. Remove the oldest image only after the supported macOS baseline has been updated deliberately.

This keeps the matrix at two supported macOS generations and avoids accidental upgrades through aliases.

## Metal validation limits

GitHub-hosted macOS runners may expose a virtualized Metal device with fewer capabilities than physical Apple Silicon hardware. CI must still verify that Katla:

- compiles against the selected macOS and Xcode environment;
- runs the complete `katla_gfx` library test suite;
- detects unsupported GPU capabilities before issuing invalid Objective-C or Metal calls;
- returns typed errors instead of aborting across the Objective-C/Rust boundary.

Pixel-accurate rendering and performance validation should use a physical, self-hosted Apple Silicon runner when one is available. The hosted matrix remains required for current SDK and compatibility coverage.

## Local equivalents

```bash
cargo fmt --all -- --check
cargo check -p katla_gfx -p katla_app --locked
cargo test -p katla_gfx --lib --locked
cargo clippy -p katla_gfx -p katla_app --locked -- -D warnings
```

Linux additionally runs workspace-wide check, test, and Clippy jobs.