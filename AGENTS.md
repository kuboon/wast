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
| file-manager | `crates/file-manager/` | **Done** (JSON) | 16 | SQLite migration |
| pattern-analyzer | `crates/syntax-plugin/internal/pattern-analyzer/` | **Done** | 17 | — |
| ruby-like syntax | `crates/syntax-plugin/ruby-like/` | **Partial** | 9 | `from_text` body parsing, body roundtrip tests |
| ts-like syntax | `crates/syntax-plugin/ts-like/` | **Partial** | 9 | `from_text` body parsing, body roundtrip tests |
| rust-like syntax | `crates/syntax-plugin/rust-like/` | **Partial** | 9 | `from_text` body parsing, body roundtrip tests |
| CLI | `packages/cli/` | **Partial** | 0 | bindgen world.wit parsing, WASM runtime integration |
| VS Code extension | `packages/vscode-extension/` | **Partial** | 0 | Body rendering, save flow, LSP, session conflicts |

## Detailed TODO

### partial-manager (`crates/partial-manager/src/lib.rs`)
- [x] **extract**: Walk function bodies to find call references and include called funcs
- [x] **extract**: `include_caller` — scan all func bodies for calls to target, include callers
- [x] **merge**: Validate that all func references in partial's internal funcs exist in full (missing_dependency check)

### file-manager (`crates/file-manager/src/lib.rs`)
- [x] **bindgen**: Parse `world.wit` and populate exported/imported funcs and types into initial wast.db
- [x] **write/merge**: Deeper world.wit validation (wit_path existence + param count matching for exported/imported funcs)
- [ ] Migrate storage from JSON to SQLite (spec requirement — currently serializes as JSON despite `.db` extension)

### syntax plugins (ruby-like, ts-like, rust-like)
- [x] **to_text**: Render actual body instructions (all 3 plugins deserialize via pattern-analyzer and render real instructions with language-specific syntax)
- [ ] **from_text**: Parse body expressions back to instructions (currently signature-only: parses func declarations, import/export markers, and param types, but **skips all body lines** — existing binary body is preserved unchanged from `existing` component)
- [ ] Add unit tests for body instruction roundtrips (current "roundtrip" tests only validate signature/metadata preservation — bodies pass through as opaque blobs, never going through parse→serialize cycle)

### CLI (`packages/cli/`)

> **Note**: All CLI commands currently use standalone JS implementations that read/write wast.db JSON directly. They do NOT load WASM components. This means the Rust crate logic (validation, body deserialization, etc.) is not used at runtime.

- [ ] Load wasm components at runtime (wasmtime/jco integration)
- [ ] `bindgen` — currently writes empty scaffold `{funcs:[], types:[]}` only. Does NOT call file-manager's bindgen or parse world.wit. Needs to invoke file-manager WASM component (or reimplement WIT parsing in JS)
- [x] `extract` — reads wast.db JSON + syms files, resolves UIDs, formats text output. Has `--include-caller` with heuristic body scanning (JS regex, not full instruction deserialization)
- [x] `merge` — parses func text blocks from stdin via regex, merges into wast.db JSON. Preserves existing bodies. Has `--dry-run` mode
- [x] `fmt` — reads stdin, validates func/type definitions exist, normalizes trailing whitespace. No actual syntax formatting (passthrough)
- [x] `diff` — compares two wast.db directories (funcs, types, syms). Detects added/removed/changed with detailed output
- [x] `syms` — reads/writes syms YAML files, classifies UIDs (wit/internal/local), updates display names
- [x] `setup-git` — configures git diff driver and .gitattributes

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
