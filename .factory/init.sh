#!/bin/bash
# Environment setup script for Katla engine
# This script is idempotent - safe to run multiple times

set -e

echo "Setting up Katla environment..."

# Check Rust version
if command -v rustc &> /dev/null; then
    RUST_VERSION=$(rustc --version)
    echo "Rust version: $RUST_VERSION"
else
    echo "ERROR: Rust not installed. Please install from https://rustup.rs"
    exit 1
fi

# Check cargo
if command -v cargo &> /dev/null; then
    CARGO_VERSION=$(cargo --version)
    echo "Cargo version: $CARGO_VERSION"
else
    echo "ERROR: Cargo not found"
    exit 1
fi

# Build dependencies
echo "Building dependencies..."
cargo build

echo "Environment setup complete!"
