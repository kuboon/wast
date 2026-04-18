# WAST Project — Agent Guide

## Architecture Overview

WAST is a system centered on the `wast.db` file format (SQLite), providing an intermediate layer between human-readable text files and WASM Components.

```
text <──syntax plugin──> partial WastComponent <──partial manager──> full WastComponent <──file manager──> [wast.db, world.wit, syms]
```

## Module Status

| Module | Path | Status | Remaining |
|---|---|---|---|
| WIT contract | `wit/wast-core.wit` | **Done** | — |
| partial-manager | `crates/partial-manager/` | **Done** | — |
| file-manager | `crates/file-manager/` | **Done** (JSON) | SQLite migration |
| file-manager-hosted | `crates/file-manager-hosted/` | **Done** | — |
| pattern-analyzer | `crates/syntax-plugin/internal/pattern-analyzer/` | **Done** | — |
| raw syntax | `crates/syntax-plugin/raw/` | **Done** | — |
| ruby-like syntax | `crates/syntax-plugin/ruby-like/` | **Partial** | `from_text` body parsing, body roundtrip tests |
| ts-like syntax | `crates/syntax-plugin/ts-like/` | **Done** | — |
| rust-like syntax | `crates/syntax-plugin/rust-like/` | **Partial** | `from_text` body parsing, body roundtrip tests |
| VS Code extension | `packages/vscode-extension/` | **Partial** | Body rendering, save flow, LSP, session conflicts |

## Detailed TODO

### partial-manager (`crates/partial-manager/src/lib.rs`)
- [x] **extract**: Walk function bodies to find call references and include called funcs
- [x] **extract**: `include_caller` — scan all func bodies for calls to target, include callers
- [x] **merge**: Validate that all func references in partial's internal funcs exist in full (missing_dependency check)

### file-manager (`crates/file-manager/src/lib.rs`)
- [x] **bindgen**: Parse `world.wit` and populate exported/imported funcs and types into initial wast.db
- [x] **write/merge**: Deeper world.wit validation (wit_path existence + param count matching for exported/imported funcs)
- [ ] Migrate storage from JSON to SQLite (spec requirement — currently serializes as JSON despite `.db` extension)

### file-manager-hosted (`crates/file-manager-hosted/src/lib.rs`)
- [x] Content-based API: accept `world.wit` / `wast.db` / `syms.en.yaml` bytes and return serialized outputs, so web and desktop hosts can use the same component without WASI or sync fs
- [x] `read` from serialized `wast.db` + optional `syms.en.yaml` and return `wast-component`
- [x] `write` / `merge` parity with `crates/file-manager/`

### syntax plugins (ruby-like, ts-like, rust-like, raw)
- [x] **to_text**: Render actual body instructions (all plugins deserialize via pattern-analyzer and render real instructions with language-specific syntax)
- [x] **from_text (ts-like)**: Full body expression parser — recursive descent parser handles all instruction types (if/else, while, block, switch/match, calls, arithmetic, comparisons, WIT types). Parses TS-like text back to `Vec<Instruction>` and serializes via pattern-analyzer
- [ ] **from_text (ruby-like, rust-like)**: Still signature-only — skips body lines, preserves existing binary body unchanged
- [x] **Body roundtrip tests (ts-like)**: simple instructions, calls, arithmetic, comparisons, if/else, loops, blocks, WIT types (some/ok/err/isErr), match-option, match-result, nested constructs
- [ ] Body roundtrip tests (ruby-like, rust-like)

### VS Code extension (`packages/vscode-extension/`)
- [x] TreeView panel — scans workspace recursively for wast.db files, lists components and functions with display names from syms. Properly filters .git/node_modules, supports depth limit
- [x] Virtual document provider (`wast://` scheme) — opens function metadata and signatures. **BUT**: function bodies show placeholder `"# [body not available — requires syntax plugin]"` because wast.db body is opaque `number[]` not decodable in JS without syntax plugin WASM
- [ ] Virtual document body rendering — requires loading syntax-plugin WASM component in extension to call `to_text` for body display
- [ ] Save flow (`from_text` → merge → write) — requires syntax-plugin + file-manager WASM integration
- [ ] LSP diagnostics (real-time `from_text` validation)
- [x] fs.watch for external wast.db changes — detects changes, refreshes tree, notifies open virtual documents
- [ ] Session conflict handling

## Responsibility Boundaries

| Layer | Responsibility |
|---|---|
| **wast** | UID, types, body. Zero name information |
| **wit** | Interface boundary and type definitions (integrated into WastComponent) |
| **syms** | Human display names only (not needed for wasm generation). Per-language files |
| **file-manager** | WastComponent <-> wast.db. world.wit consistency validation. WASI-based |
| **partial-manager** | extract / merge (stage 2 validation) |
| **syntax-plugin** | wast <-> text bidirectional conversion (stage 1 validation). New UID generation |
| **CLI / Editor** | User operations and workflow control |

## Development Commands

```bash
# Rust
cargo component build --workspace   # Build all wasm components
cargo test --workspace               # Run all Rust tests
cargo fmt                            # Format source code

# TypeScript
pnpm install                         # Install dependencies
pnpm build                           # Build all packages
pnpm test                            # Run all tests

# Devcontainer image publish (split architecture)
cd .devcontainer && ./push.sh        # Local arm64 push: arm64-<sha>, arm64-latest
# Then run workflow: Publish Devcontainer Image
# input source_sha=<same sha>         # Builds amd64 in GitHub Actions and publishes multi-arch manifest

# CI check (same as GitHub Actions)
cargo component build --workspace && \
  find . -name bindings.rs -path '*/src/*' | xargs rustfmt && \
  cargo fmt --check && \
  cargo test --workspace && \
  pnpm build && \
  pnpm test
```

## Key Design Principles

- **Names are not code essence** — all identifiers are meaningless UIDs
- **wasm generation requires only wast + wit** — syms are never needed
- **Minimize identifier change cost** — UIDs are stable, display names are in syms
- **WastComponent is the central type** — partial and full share the same type definition
- **Syntax plugins are stateless** — called fresh each time

## Agent Instructions

**When completing a task**, update this file:
1. Move the completed item from the TODO list (change `[ ]` to `[x]`)
2. Update the Module Status table (remaining column)
3. Commit the AGENTS.md update together with the implementation
