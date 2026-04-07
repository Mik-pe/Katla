#!/bin/bash
# Environment setup for Katla workspace
# Idempotent — safe to run multiple times

set -e

# Verify Rust toolchain
cargo --version
rustc --version

# Build workspace to verify compilation
cargo check --workspace 2>&1 | tail -5

echo "Katla workspace environment ready."
