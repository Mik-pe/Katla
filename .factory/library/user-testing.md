# User Testing

Testing surface, required testing skills/tools, and resource cost classification.

## Validation Surface

This mission has NO user testing surface. It is a pure codebase cleanup with no UI, no services, and no runtime behavior to validate.

All validation is through automated tooling:
- `cargo check --workspace` — compilation
- `cargo test --workspace` — test suite
- `cargo clippy --workspace -- -D warnings` — linting
- `cargo fmt --check` — formatting
- grep-based checks for specific patterns (dead code removal, API changes)

## Validation Concurrency

Max concurrent validators: 1

All validation is sequential cargo commands. No concurrent processes needed. Each cargo invocation uses ~2-4 GB RAM for compilation.
