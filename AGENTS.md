# WAST Project — Agent Guide

## Architecture Overview

WAST provides an intermediate layer between human-readable text files and WASM Components. On-disk storage is `wast.json` (current) with future migration to `wast.db` (SQLite).

```
text <──syntax plugin──> partial/full WastComponent
partial WastComponent <──partial manager──> full WastComponent
WastComponent <──file manager──> [wast.json, world.wit, syms]
[wast.json, world.wit] --compiler--> wasm component
```

**Top priority**: `compiler` (wast → wasm Component). See [crates/compiler/PLAN.md](crates/compiler/PLAN.md) for the v0 plan. Design decisions for IR / body format / storage schema must be driven by compiler requirements, not storage convenience.

## Storage format

- **`wast.json`** — current format, row-oriented JSON designed for mechanical migration to SQLite rows
- **`wast.db`** — future SQLite format (same logical schema, indexed for caller/callee traversal)
- Both hold identical `WastComponent` content; format choice is pure serialization

See [crates/file-manager/PLAN.md](crates/file-manager/PLAN.md) for the SQLite migration roadmap.

## Module Status

| Module | Path | Status | Remaining |
|---|---|---|---|
| WIT contract | `wit/wast-core.wit` | **Done** | — |
| partial-manager | `crates/partial-manager/` | **Done** | — |
| file-manager | `crates/file-manager/` | **Done** (JSON, row-oriented) | SQLite migration |
| file-manager-hosted | `crates/file-manager-hosted/` | **Done** (JSON, row-oriented) | — |
| wast-types (shared serde types) | `crates/wast-types/` | **Done** | — |
| compiler | `crates/compiler/` | **v0.11 done** (core-only emit + `wit-component` wrap) | string/list/record/variant |
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

### compiler (`crates/compiler/`) — top priority
- [x] Extract shared serde types into new `crates/wast-types/` crate (prerequisite; both file-manager crates and compiler depend on it)
- [x] Scaffold `crates/compiler/` as plain rlib (no `-hosted` suffix; future wasm-component migration is mechanical)
- [x] v0: emit fixed Component WAT for WASI CLI empty run (`wasi:cli/run@0.2.0`), verify via `wasmtime::component::Command` → `Ok(())`
- [x] v0.1: emit `u32 -> u32` identity function, verify via Rust `wasmtime::component` harness
- [x] v0.2: numeric primitives — `Const`, `Arithmetic` (add/sub/mul/div with signedness), `Compare` (eq/ne/lt/le/gt/ge with signedness) across i32/i64/u32/u64/f32/f64/bool; type-inferred `Const`; `s32`/`s64` mapping for lifted signatures
- [x] v0.3: `Call` to internal funcs — single core module houses all internal + exported funcs, callers push args in the callee's declared param order (callers may pass args by name in any order)
- [x] v0.4: control flow (`If`/`Else` with optional typed result, `Block`/`Loop`, `Br`/`BrIf`) + `LocalSet` with first-assignment local declaration (locals collected from body, emitted as `(local …)` after params)
- [x] v0.5: imported `Call` — component-level `(import …)` → `canon lower` core func → core-module `(import "imports" "name" …)` wired via `(with "imports" (instance …))` at instantiation. Primitive-only imports (no memory/realloc yet)
- [x] v0.6: `option<prim>` / `result<prim, prim>` **in param position** + `IsErr` on result locals. Canonical-ABI flat layout: each compound param expands to `(i32 disc, join<payload> payload)` core slots. `LocalGet` pushes all slots; `IsErr` reads only the disc slot. Return position requires `cabi_realloc` — deferred.
- [x] v0.7: memory + `cabi_realloc` infrastructure. Every non-empty core module now exports `memory` (1 page) + a bump-allocator `cabi_realloc` backed by a mutable `$heap_end` global (starts at 1024). Bulk `memory.copy` handles realloc grows. `canon lift` threads `(memory $m "memory") (realloc (func $m "cabi_realloc"))`. `canon lower` still lacks options due to circular ref (core instance $m not yet created) — compound imports deferred until the allocator module is split out.
- [x] v0.8: compound returns — `Some`/`None`/`Ok`/`Err` with primitive payload. Core funcs whose flat-result slot count > `MAX_FLAT_RESULTS=1` switch to indirect return (single `i32` pointer). `body_needs_ret_ptr` scans the body for variant ctors and reserves an extra `i32` local. `size_align`/`store_op`/`align_up` helpers implement the Canonical-ABI variant layout (u8 disc + padded payload).
- [x] v0.9: `MatchOption` / `MatchResult` destructuring. Bindings (`some_binding`, `ok_binding`, `err_binding`) are collected as function-scope locals with the payload WIT type. MatchOption `local.set`s payload into the binding then branches on disc. MatchResult uses `local.tee`+`local.set` to seed both ok/err bindings in one go, then branches. MatchResult currently requires ok/err to share a core type (heterogeneous join + truncation deferred).
- [x] v0.10 spike: validated `wit-component` + `wit-parser` (v0.219, matching wasmtime 27's wasmparser) can wrap our core module output into a Component. Spike covers both `identity(u32)->u32` and indirect-return `mk-some(u32)->option<u32>`.
- [x] v0.11: rewrote emit.rs. `compile_component` now emits **core-only WAT** (single `(module …)`), synthesizes a WIT world from `db`'s exports/imports (inline type refs for option/result), embeds the `component-type` custom section via `embed_component_metadata`, and wraps via `ComponentEncoder`. Consequences: hand-rolled `canon lift`/`canon lower` + outer `(component …)` + memory-option threading are all gone. Imports use `"$root"` namespace convention. `canon lower` circular-reference problem is solved (wit-component handles it). All 25 tests pass end-to-end unchanged. Core body emit (IR → core WAT instructions) is unchanged — only the shell changed.
- [ ] Roadmap: `string` (ptr+len, mostly automatic via wit-component once IR gains `StringLiteral`/`StringLen`/etc.) → `list<T>` → `record/variant/tuple/resource`
- See [crates/compiler/PLAN.md](crates/compiler/PLAN.md) for full context

### file-manager (`crates/file-manager/src/lib.rs`)
- [x] **bindgen**: Parse `world.wit` and populate exported/imported funcs and types into initial wast.json
- [x] **write/merge**: Deeper world.wit validation (wit_path existence + param count matching for exported/imported funcs)
- [x] Row-oriented JSON schema (each func/type is an object with inline `uid`, ready for SQLite row mapping)
- [ ] Migrate to SQLite (`wast.db`) once JSON compiler path stabilizes
- [ ] Populate `calls: Vec<String>` on each func via `pattern-analyzer::deserialize_body` at write time (caller→callee edge index for future SQLite indexing)

### file-manager-hosted (`crates/file-manager-hosted/src/lib.rs`)
- [x] Content-based API: accept `world.wit` / `wast.json` / `syms.en.yaml` bytes and return serialized outputs, so web and desktop hosts can use the same component without WASI or sync fs
- [x] `read` from serialized `wast.json` + optional `syms.en.yaml` and return `wast-component`
- [x] `write` / `merge` parity with `crates/file-manager/`
- [ ] Same `calls` index population as file-manager

### syntax plugins (ruby-like, ts-like, rust-like, raw)
- [x] **to_text**: Render actual body instructions (all plugins deserialize via pattern-analyzer and render real instructions with language-specific syntax)
- [x] **from_text (ts-like)**: Full body expression parser — recursive descent parser handles all instruction types (if/else, while, block, switch/match, calls, arithmetic, comparisons, WIT types). Parses TS-like text back to `Vec<Instruction>` and serializes via pattern-analyzer
- [ ] **from_text (ruby-like, rust-like)**: Still signature-only — skips body lines, preserves existing binary body unchanged
- [x] **Body roundtrip tests (ts-like)**: simple instructions, calls, arithmetic, comparisons, if/else, loops, blocks, WIT types (some/ok/err/isErr), match-option, match-result, nested constructs
- [ ] Body roundtrip tests (ruby-like, rust-like)

### VS Code extension (`packages/vscode-extension/`)
- [x] TreeView panel — scans workspace recursively for wast.json files, lists components and functions with display names from syms. Properly filters .git/node_modules, supports depth limit
- [x] Virtual document provider (`wast://` scheme) — opens function metadata and signatures. **BUT**: function bodies show placeholder `"# [body not available — requires syntax plugin]"` because wast.json body is opaque `number[]` not decodable in JS without syntax plugin WASM
- [ ] Virtual document body rendering — requires loading syntax-plugin WASM component in extension to call `to_text` for body display
- [ ] Save flow (`from_text` → merge → write) — requires syntax-plugin + file-manager WASM integration
- [ ] LSP diagnostics (real-time `from_text` validation)
- [x] fs.watch for external wast.json changes — detects changes, refreshes tree, notifies open virtual documents
- [ ] Session conflict handling

## Responsibility Boundaries

| Layer | Responsibility |
|---|---|
| **wast** | UID, types, body. Zero name information |
| **wit** | Interface boundary and type definitions (integrated into WastComponent) |
| **syms** | Human display names only (not needed for wasm generation). Per-language files |
| **file-manager** | WastComponent <-> wast.json (future wast.db SQLite). world.wit consistency validation. WASI-based |
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
