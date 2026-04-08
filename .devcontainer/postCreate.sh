#!/bin/bash
set -e

mise trust
cargo component build --workspace
pnpm install
