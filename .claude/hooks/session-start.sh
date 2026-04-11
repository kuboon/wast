#!/bin/bash
set -euo pipefail

# Only run in remote (web) environments
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# Ensure Rust toolchain is up to date (installs wasm32-wasip1 target + rustfmt via rust-toolchain.toml)
rustup update

# Install cargo-component for building WASM components
cargo install cargo-component

# Build WASM component crates (exclude cli-rust which is a native binary using wasmtime)
cargo component build --workspace --exclude wast-cli-rust

# Build the native CLI binary
cargo build -p wast-cli-rust

# Install Node.js dependencies via pnpm
pnpm install
