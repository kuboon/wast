# WAST Project — Agent Guide

This file describes the repository **as it is now**. Future work lives in the
per-crate/per-package `PLAN.md` files (those contain *only* plans, never
current-state docs).

## Architecture Overview

WAST provides an intermediate layer between human-readable text files and WASM
Components. On-disk storage is `wast.json` (current) with a future migration to
`wast.db` (SQLite).

```
text <──syntax plugin──> partial/full WastComponent
partial WastComponent <──partial manager──> full WastComponent
WastComponent <──wast-codec──> bytes(wast.json, world.wit, syms.en.yaml)
[wast.json, world.wit] --compiler--> wasm component
```

The Rust crates are compiled to wasm components (`cargo component`), transpiled
with jco where a JS host needs them, and consumed by two hosts: the VS Code
extension (`packages/vscode-extension/`) and the web demo
(`packages/web-demo/`).

## Workspace layout

| Path | What |
|---|---|
| `wit/wast-core.wit` | `wast:core` package — `syntax-plugin` and `partial-manager` interfaces |
| `wit-types/types.wit` | `wast:types` package — shared type vocabulary (`wast-component`, `wast-error`, …) `use`d by every other WIT package |
| `wit-codec/codec.wit` | `wast:codec` package — `compile-wit` / `read` / `write` / `merge` |
| `wit-compiler/compiler.wit` | `wast:compiler` package — `compile(component, world-wit) -> result<list<u8>, wast-error>` |
| `crates/wast-types/` | Shared serde types (rlib). Defines the `wast.json` schema (`WastDb`) |
| `crates/wast-codec/` | Codec component: `WastComponent` ↔ `wast.json` / `syms.en.yaml` bytes, `world.wit` validation |
| `crates/partial-manager/` | Extract/merge component (see semantics below) |
| `crates/compiler/` | wast → wasm Component compiler (rlib) |
| `crates/compiler-component/` | WIT wrapper component around the `wast-compiler` rlib |
| `crates/syntax-plugin/{raw,ruby-like,ts-like,rust-like}/` | The 4 reference syntax-plugin components |
| `crates/syntax-plugin/internal/pattern-analyzer/` | Rlib: `Instruction` IR, body (de)serialization, control-flow pattern detection (while/for/for-in/try) |
| `crates/syntax-plugin/internal/syntax-core/` | Rlib: Rust scaffolding for plugins — shared `wit_types` bindings, `convert`, `RenderContext`, `TypePrinter`, `scaffold` from_text helpers |
| `crates/demo-gen/` | Legacy generator for web-demo milestone demos (see Tech debt) |
| `packages/vscode-extension/` | VS Code extension: TreeView, editable `wast://` virtual docs, compile command |
| `packages/web-demo/` | Browser playground (jco-transpiled components), deployed to GitHub Pages |
| `packages/sample-wast/` | Canonical hand-authored sample (`wast.json` + `world.wit` + `syms.en.yaml`) |
| `docs/PLUGIN-AUTHORING.md` | How to write a new syntax plugin |

## WIT contract

- All four WIT packages share one type vocabulary: the **`wast:types`** package
  in `wit-types/types.wit`. `wast:core`, `wast:codec`, and `wast:compiler`
  `use wast:types/types@0.1.0.{…}` — one definition, no copies to drift.
- `syntax-plugin.to-text` returns `result<string, list<wast-error>>` (plugins
  must fail rather than render lossy placeholders); `from-text` returns
  `result<wast-component, list<wast-error>>`.
- Rust crates bind the shared types once via `wast_syntax_core::wit_types` and
  remap their generated bindings onto it with
  `[package.metadata.component.bindings] with = { "wast:types/types@0.1.0" = "wast_syntax_core::wit_types" }`.
- The WIT `syntax-plugin` interface is the language-agnostic contract — anyone
  can implement it in any language that targets WASM Components.

## Storage and encoding formats

- **`wast.json`** — row-oriented JSON (each func/type row inlines its `uid`,
  ready for a 1:1 SQLite row mapping). The top-level object has a **required
  integer `version` field** (no serde default — files without it are
  rejected). Current schema version: **1** (`WastDb::CURRENT_VERSION`).
- **Function bodies** — `option<list<u8>>` in WIT. The byte layout is a single
  **format-version byte** (`BODY_FORMAT_VERSION = 1`) followed by the
  `postcard` encoding of `Vec<Instruction>` (see
  `crates/syntax-plugin/internal/pattern-analyzer/`). Decoders reject unknown
  versions.
- **`wast.db`** — future SQLite format (same logical schema). The codec is
  byte-oriented: hosts pass full file contents in/out, because vscode-web's
  workspace fs has no partial r/w.
- **`world.wit` + `wast.json` + `syms.en.yaml` are SOURCE CODE** —
  hand-authored, or edited via the VS Code extension's text-pane round-trip
  (`from_text` → `merge` → `codec.write`). Never design build-time generators
  that emit them. The canonical shared sample lives at `packages/sample-wast/`.

Historical note: the WASI-fs-based `crates/file-manager/` was retired 2026-04 —
jco couldn't transpile its WASI fs deps and vscode-web has no partial-access
fs API, so the byte-oriented codec model fits both web and desktop hosts.

## Responsibility boundaries

| Layer | Responsibility |
|---|---|
| **wast** | UID, types, body. Zero name information |
| **wit** | Interface boundary and type definitions (integrated into WastComponent) |
| **syms** | Human display names only (not needed for wasm generation). Per-language files |
| **wast-codec** | WastComponent ↔ wast.json bytes (future wast.db SQLite). world.wit consistency validation. Byte-oriented, host-driven I/O |
| **partial-manager** | extract / merge (stage 2 validation) |
| **syntax-plugin** | wast ↔ text bidirectional conversion (stage 1 validation). New UID generation |
| **wast-syntax-core** | Optional Rust scaffolding for Rust plugins. Not part of the WIT contract. See [docs/PLUGIN-AUTHORING.md](docs/PLUGIN-AUTHORING.md) |
| **CLI / Editor** | User operations and workflow control |

### partial-manager semantics (condensed)

`extract(full, targets)` builds a partial: every target is included; targets
*without* `include_caller` are forced to `Exported(uid)` (signature locked —
the partial can't prove all callers are visible); targets *with*
`include_caller` keep their original source and pull their direct callers in
(with bodies); callees are added as signature-only `Imported(uid)` stubs.
`merge(partial, full)` verifies `Imported`/`Exported` signatures against
`full`, replaces/adds `Internal` funcs, and errors on `signature_mismatch`,
`missing_dependency`, or `uid_conflict`.

### Compiler pipeline (condensed)

```
WastDb + synthesized WIT world
  → emit_core_module (core-only WAT)        # the hand-written part
  → wat::parse_str → core .wasm
  → wit_component::embed_component_metadata
  → wit_component::ComponentEncoder         # shell: canon lift/lower, wiring
  → Component .wasm
```

The compiler emits only the core module; the component shell is delegated to
`wit-component` (pinned at 0.219 to match wasmtime 27's wasmparser). The IR
stays a high-level semantic representation (not a core-opcode list) so syntax
plugins can round-trip it. Coverage today: numerics, control flow, calls
(internal + imported), option/result/variant/record/tuple/enum/flags,
string/list (params, returns, literals), nested compounds, resources
(exported + imported), heterogeneous narrows/widens. Remaining gaps are listed
in [crates/compiler/PLAN.md](crates/compiler/PLAN.md).

### Syntax plugin from_text status

- **raw**, **ts-like**: full body parsers (S-expression / recursive descent) —
  structural round-trip of signatures *and* bodies.
- **ruby-like**, **rust-like**: signatures parse; bodies are preserved from
  the `existing` component (depth-aware body skip). Real body parsers are
  future work (see their `PLAN.md`s).

### VS Code extension status

Bundles 7 components into `dist/components/` (4 syntax plugins,
partial-manager, codec, compiler). TreeView over workspace `wast.json` files;
editable `wast://` virtual docs via `FileSystemProvider` (save runs
`from_text` → `merge` → `codec.write`); fs.watch refresh; `WAST: Compile
current component` writes `<dir>/dist/<name>.wasm`. Runs on Node (desktop)
hosts; vscode-web support is future work (see
[packages/vscode-extension/PLAN.md](packages/vscode-extension/PLAN.md)).

## Development commands

Toolchain: Rust 1.96.0 (pinned in `rust-toolchain.toml`, target
`wasm32-wasip1`), Node 24 / pnpm 10 / wasmtime via `mise` (`mise.toml`),
plus `cargo-component` (the devcontainer installs it with
`cargo binstall cargo-component wasm-tools`).

```bash
# mise tasks (preferred)
mise run build              # cargo component build --workspace + pnpm install + pnpm build
mise run test-component     # cargo test --workspace (depends on build)
mise run test-ts            # pnpm test (depends on build)
mise run ci                 # build + both test tasks (what CI runs)
mise run bundle-components  # rebuild the vscode-extension component bundle

# direct
cargo component build --workspace   # build all wasm components
cargo test --workspace              # all Rust tests
cargo fmt                           # format (CI enforces cargo fmt --check)
pnpm install && pnpm build && pnpm test   # TS packages

# Devcontainer image publish (split architecture)
cd .devcontainer && ./push.sh        # local arm64 push: arm64-<sha>, arm64-latest
# then run the "Publish Devcontainer Image" workflow with source_sha=<same sha>
```

CI (`.github/workflows/ci.yml`) runs `mise run ci` then `cargo fmt --check`
inside the devcontainer image. `.github/workflows/deploy-pages.yml` builds
`packages/web-demo` and deploys its `dist/` to GitHub Pages.

## Key design principles

- **Names are not code essence** — all identifiers are meaningless UIDs
- **wasm generation requires only wast + wit** — syms are never needed
- **Minimize identifier change cost** — UIDs are stable, display names are in syms
- **WastComponent is the central type** — partial and full share the same type definition
- **Syntax plugins are stateless** — called fresh each time
- **The compiler IR is a high-level semantic representation** — never a core
  opcode list; anything `wit-component` can do is delegated to `wit-component`

## Tech debt / scheduled cleanup

- **`crates/demo-gen`** — legacy build-time generator that emits the v0.x
  milestone `.wasm` + `manifest.json` for the web-demo playground.
  Conceptually obsolete under the source-code rule above, but kept until the
  replacement (a few consolidated hand-authored demos) is designed. Treat as
  legacy: don't extend it with new milestone entries. Removal path:
  hand-author 2-3 rich `packages/<demo>/` folders, switch
  `transpile-all.mjs` to read from those, delete the crate.

## Agent instructions

When completing a task:
1. Remove the finished item from the relevant `PLAN.md` (PLAN files hold
   *only* future work — delete done items rather than checking them off).
2. If behavior described in this file changed, update this file to match.
3. Commit the doc updates together with the implementation.
