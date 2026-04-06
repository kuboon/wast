# WAST Project — Agent Guide

## Architecture Overview

WAST is a system centered on the `wast.db` file format (SQLite), providing an intermediate layer between human-readable text files and WASM Components.

```
text <──syntax plugin──> partial WastComponent <──partial manager──> full WastComponent <──file manager──> [wast.db, world.wit, syms]
```

## Module Map

| Module | Path | Language | Type | Status |
|---|---|---|---|---|
| WIT contract | `wit/wast-core.wit` | WIT | Shared types | Skeleton |
| file-manager | `crates/file-manager/` | Rust | wasm component (WASI) | Not started |
| partial-manager | `crates/partial-manager/` | Rust | wasm component | Not started |
| pattern-analyzer | `crates/syntax-plugin/internal/pattern-analyzer/` | Rust | library crate | Not started |
| ruby-like syntax | `crates/syntax-plugin/ruby-like/` | Rust | wasm component | Not started |
| ts-like syntax | `crates/syntax-plugin/ts-like/` | Rust | wasm component | Not started |
| rust-like syntax | `crates/syntax-plugin/rust-like/` | Rust | wasm component | Not started |
| CLI | `packages/cli/` | TypeScript | pnpm package | Not started |
| VS Code extension | `packages/vscode-extension/` | TypeScript | pnpm package | Not started |

## Responsibility Boundaries

| Layer | Responsibility |
|---|---|
| **wast** | UID, types, body. Zero name information |
| **wit** | Interface boundary and type definitions (integrated into WastComponent) |
| **syms** | Human display names only (not needed for wasm generation). Per-language files |
| **file-manager** | WastComponent <-> wast.db (SQLite). world.wit consistency validation. WASI-based |
| **partial-manager** | extract / merge (stage 2 validation) |
| **syntax-plugin** | wast <-> text bidirectional conversion (stage 1 validation). New UID generation |
| **CLI / Editor** | User operations and workflow control |

## Implementation Order

1. `wit/wast-core.wit` — contract for all crates (done)
2. `crates/partial-manager` and `crates/file-manager`
3. `crates/syntax-plugin/internal/pattern-analyzer` → each syntax variant
4. `packages/cli`
5. `packages/vscode-extension`

## Development Commands

```bash
# Rust
cargo check                    # Type check all crates
cargo test --workspace         # Run all Rust tests
cargo component build          # Build wasm components (needs cargo-component)

# TypeScript
pnpm install                   # Install dependencies
pnpm build                     # Build all packages
pnpm test                      # Run all tests
```

## Key Design Principles

- **Names are not code essence** — all identifiers are meaningless UIDs
- **wasm generation requires only wast + wit** — syms are never needed
- **Minimize identifier change cost** — UIDs are stable, display names are in syms
- **WastComponent is the central type** — partial and full share the same type definition
- **Syntax plugins are stateless** — called fresh each time
