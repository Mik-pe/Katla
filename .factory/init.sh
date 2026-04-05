#!/bin/bash
# Environment setup script for Katla engine missions
# This script is idempotent - safe to run multiple times

echo "Setting up Katla environment..."

# Check Rust toolchain
if command -v rustc &> /dev/null; then
    echo "Rust version: $(rustc --version)"
else
    echo "ERROR: Rust not installed. Please install from https://rustup.rs"
    exit 1
fi

if command -v cargo &> /dev/null; then
    echo "Cargo version: $(cargo --version)"
else
    echo "ERROR: Cargo not found"
    exit 1
fi

# Build workspace to ensure all dependencies are downloaded
echo "Building workspace..."
cargo build --workspace --lib 2>&1 || echo "WARNING: Build had issues, continuing..."

echo "Environment setup complete!"
