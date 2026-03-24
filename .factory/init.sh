#!/bin/bash
# Environment setup script for Katla engine cleanup mission
# This script is idempotent - safe to run multiple times

set -e

echo "Setting up Katla cleanup environment..."

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

# Verify project compiles
echo "Checking project compiles..."
cargo check

echo "Environment setup complete!"
