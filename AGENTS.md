# WAST Project — Agent Guide

## Architecture Overview

WAST is a system centered on the `wast.db` file format (SQLite), providing an intermediate layer between human-readable text files and WASM Components.

```
text <──syntax plugin──> partial WastComponent <──partial manager──> full WastComponent <──file manager──> [wast.db, world.wit, syms]
```

## Module Status

| Module | Path | Status | Tests | Remaining |
|---|---|---|---|---|
| WIT contract | `wit/wast-core.wit` | **Done** | — | — |
| partial-manager | `crates/partial-manager/` | **Done** | 21 | — |
| file-manager | `crates/file-manager/` | **Partial** | 2 | world.wit parsing for bindgen, SQLite migration |
| pattern-analyzer | `crates/syntax-plugin/internal/pattern-analyzer/` | **Done** | 17 | — |
| ruby-like syntax | `crates/syntax-plugin/ruby-like/` | **Partial** | 0 | Body rendering/parsing, tests |
| ts-like syntax | `crates/syntax-plugin/ts-like/` | **Partial** | 0 | Body rendering/parsing, tests |
| rust-like syntax | `crates/syntax-plugin/rust-like/` | **Partial** | 0 | Body rendering/parsing, tests |
| CLI | `packages/cli/` | **Partial** | 0 | 4 commands need wasm runtime (extract, merge, fmt, diff) |
| VS Code extension | `packages/vscode-extension/` | **Stub** | 0 | Everything |

## Detailed TODO

### partial-manager (`crates/partial-manager/src/lib.rs`)
- [x] **extract**: Walk function bodies to find call references and include called funcs
- [x] **extract**: `include_caller` — scan all func bodies for calls to target, include callers
- [x] **merge**: Validate that all func references in partial's internal funcs exist in full (missing_dependency check)

### file-manager (`crates/file-manager/src/lib.rs`)
- [ ] **bindgen**: Parse `world.wit` and populate exported/imported funcs and types into initial wast.db
- [ ] **write/merge**: Deeper world.wit validation (currently only checks file exists)
- [ ] Migrate storage from JSON to SQLite (spec requirement)

### syntax plugins (ruby-like, ts-like, rust-like)
- [ ] **to_text**: Render actual body instructions (currently placeholder `[body: N bytes]`)
- [ ] **from_text**: Parse body expressions back to instructions
- [ ] Add unit tests for to_text/from_text roundtrips

### CLI (`packages/cli/`)
- [ ] Load wasm components at runtime (wasmtime/jco integration)
- [x] `bindgen` — creates empty wast.db scaffold (TODO: parse world.wit via file-manager)
- [ ] `extract` — call file-manager.read + partial-manager.extract + syntax-plugin.to_text
- [ ] `merge` — call syntax-plugin.from_text + file-manager.merge
- [ ] `fmt` — call syntax-plugin.from_text + syntax-plugin.to_text
- [ ] `diff` — call syntax-plugin.to_text on both + difftastic
- [x] `syms` — write display name to syms file
- [x] `setup-git` — configure git diff driver

### VS Code extension (`packages/vscode-extension/`)
- [ ] TreeView panel (list wast.db components and functions)
- [ ] Virtual document provider (`wast://` scheme)
- [ ] Save flow (from_text → merge → write)
- [ ] LSP diagnostics (real-time from_text validation)
- [ ] fs.watch for external wast.db changes
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
cargo test --workspace               # Run all Rust tests (36 tests)
cargo fmt                            # Format source code

# TypeScript
pnpm install                         # Install dependencies
pnpm build                           # Build all packages
pnpm test                            # Run all tests

# CI check (same as GitHub Actions)
cargo component build --workspace && \
  find . -name bindings.rs -path '*/src/*' | xargs rustfmt && \
  cargo fmt --check && \
  cargo test --workspace && \
  pnpm build
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
2. Update the Module Status table (tests count, remaining column)
3. Commit the AGENTS.md update together with the implementation
