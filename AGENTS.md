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
| file-manager | `crates/file-manager/` | **Done** | 16 | SQLite migration (future) |
| pattern-analyzer | `crates/syntax-plugin/internal/pattern-analyzer/` | **Done** | 17 | — |
| ruby-like syntax | `crates/syntax-plugin/ruby-like/` | **Done** | 9 | Body text→instructions parsing (future) |
| ts-like syntax | `crates/syntax-plugin/ts-like/` | **Done** | 9 | Body text→instructions parsing (future) |
| rust-like syntax | `crates/syntax-plugin/rust-like/` | **Done** | 9 | Body text→instructions parsing (future) |
| CLI | `packages/cli/` | **Done** | 0 | Wasm runtime integration (future) |
| VS Code extension | `packages/vscode-extension/` | **Done** | 0 | LSP, save flow, session conflicts (future) |

## Detailed TODO

### partial-manager (`crates/partial-manager/src/lib.rs`)
- [x] **extract**: Walk function bodies to find call references and include called funcs
- [x] **extract**: `include_caller` — scan all func bodies for calls to target, include callers
- [x] **merge**: Validate that all func references in partial's internal funcs exist in full (missing_dependency check)

### file-manager (`crates/file-manager/src/lib.rs`)
- [x] **bindgen**: Parse `world.wit` and populate exported/imported funcs and types into initial wast.db
- [x] **write/merge**: Deeper world.wit validation (currently only checks file exists)
- [ ] Migrate storage from JSON to SQLite (spec requirement)

### syntax plugins (ruby-like, ts-like, rust-like)
- [x] **to_text**: Render actual body instructions (all 3 plugins now deserialize and render instructions)
- [ ] **from_text**: Parse body expressions back to instructions
- [x] Add unit tests for to_text/from_text roundtrips

### CLI (`packages/cli/`)
- [ ] Load wasm components at runtime (wasmtime/jco integration)
- [x] `bindgen` — creates empty wast.db scaffold (TODO: parse world.wit via file-manager)
- [x] `extract` — reads wast.db + syms directly, formats func dump text
- [x] `merge` — parses func text from stdin, merges into wast.db JSON
- [x] `fmt` — validates and normalizes wast text from stdin (passthrough)
- [x] `diff` — compares two wast.db files (funcs, types, syms)
- [x] `syms` — write display name to syms file
- [x] `setup-git` — configure git diff driver

### VS Code extension (`packages/vscode-extension/`)
- [x] TreeView panel (list wast.db components and functions)
- [x] Virtual document provider (`wast://` scheme)
- [ ] Save flow (from_text → merge → write)
- [ ] LSP diagnostics (real-time from_text validation)
- [x] fs.watch for external wast.db changes
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
cargo test --workspace               # Run all Rust tests (76 tests)
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
