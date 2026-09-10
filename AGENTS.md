# WAST Project — Agent Guide

This file describes the repository **as it is now**. Future work lives in the
per-crate/per-package `PLAN.md` files (those contain *only* plans, never
current-state docs).

## Architecture Overview

WAST provides an intermediate layer between human-readable text files and WASM
Components. On-disk storage is `wast.json` (current) with a future migration to
`wast.db` (SQLite).

```
WastComponent ──syntax renderer──> text (read-only projection, any syntax)
text <──syntax editor──> partial/full WastComponent   (write syntaxes only)
partial WastComponent <──partial manager──> full WastComponent  (structured write path)
WastComponent <──wast-codec──> bytes(wast.json, world.wit, syms.en.yaml)
[wast.json, world.wit] --compiler--> wasm component
```

Reading and writing are asymmetric: every syntax plugin exports
`syntax-renderer` (`to-text`); only designated write syntaxes (ir-json, raw,
ts-like) also export `syntax-editor` (`from-text`). Human-facing display
syntaxes (ruby-like, rust-like) are renderer-only — read-only views for
review/diff.

**The structured write path** is how edits actually land, and it's what
agents use:

```
extract(full, targets) → partial ──ir-json.to-text──> JSON document (uids + instruction trees)
                                        ↓ (edit)
merge(partial, full) <──ir-json.from-text── edited JSON document
        ↓ (validates signatures, uids, and bodies)
codec.write → wast.json + syms.<lang>.yaml
```

Nothing in that loop parses a language. `ir-json` renders the IR with uids
left explicit and bodies left as instruction trees, so identity is recorded
rather than inferred and a body edit can't be a syntax error.
`packages/mcp-server/` exposes exactly this loop as MCP tools.

The Rust crates are compiled to wasm components (`cargo component`), transpiled
with jco where a JS host needs them, and consumed by three hosts: the VS Code
extension (`packages/vscode-extension/`), the web demo
(`packages/web-demo/`), and the MCP server (`packages/mcp-server/`). The
build+transpile plumbing the Node hosts share lives in
`scripts/lib/components.mjs`.

## Workspace layout

| Path | What |
|---|---|
| `wit/wast-core.wit` | `wast:core` package — `syntax-renderer`, `syntax-editor`, and `partial-manager` interfaces |
| `wit-types/types.wit` | `wast:types` package — shared type vocabulary (`wast-component`, `wast-error`, …) `use`d by every other WIT package |
| `wit-codec/codec.wit` | `wast:codec` package — `compile-wit` / `read` / `write` / `merge` |
| `wit-compiler/compiler.wit` | `wast:compiler` package — `compile` (Component), `compile-core` (bare core module, browser-instantiable), `emit-wat` (generated WAT text) |
| `crates/wast-types/` | Shared serde types (rlib). Defines the `wast.json` schema (`WastDb`) |
| `crates/wast-codec/` | Codec component: `WastComponent` ↔ `wast.json` / `syms.en.yaml` bytes, `world.wit` validation |
| `crates/partial-manager/` | Extract/merge component (see semantics below) |
| `crates/compiler/` | wast → wasm Component compiler (rlib) |
| `crates/compiler-component/` | WIT wrapper component around the `wast-compiler` rlib |
| `crates/syntax-plugin/ir-json/` | The IR as JSON: the structured write surface (renderer + editor) |
| `crates/syntax-plugin/{raw,ruby-like,ts-like,rust-like}/` | The 4 language-flavored reference syntax-plugin components |
| `crates/syntax-plugin/internal/pattern-analyzer/` | Rlib: `Instruction` IR, body (de)serialization (postcard + JSON), control-flow pattern detection (while/for/for-in/try) |
| `crates/syntax-plugin/internal/syntax-core/` | Rlib: Rust scaffolding for plugins — shared `wit_types` bindings, `convert`, `RenderContext`, `TypePrinter`, `scaffold` editor-side (from_text) helpers |
| `crates/demo-gen/` | Legacy generator for web-demo milestone demos (see Tech debt) |
| `packages/vscode-extension/` | VS Code extension: TreeView, editable `wast://` virtual docs, compile command |
| `packages/web-demo/` | GitHub Pages site. The **playground** (`src/playground.js` + `src/samples.js`) compiles wast to a core wasm module in the browser and runs it; the syntax-plugin showcase and pre-built Components sit below it, collapsed |
| `packages/mcp-server/` | MCP server exposing the structured write path as agent tools ([README](packages/mcp-server/README.md)) |
| `packages/sample-wast/` | Canonical hand-authored sample (`wast.json` + `world.wit` + `syms.en.yaml`) |
| `scripts/lib/components.mjs` | Shared cargo-component build + jco transpile helper for the host bundles |
| `docs/PLUGIN-AUTHORING.md` | How to write a new syntax plugin |

## WIT contract

- All four WIT packages share one type vocabulary: the **`wast:types`** package
  in `wit-types/types.wit`. `wast:core`, `wast:codec`, and `wast:compiler`
  `use wast:types/types@0.1.0.{…}` — one definition, no copies to drift.
- The plugin contract is split into two interfaces:
  **`syntax-renderer`** (`to-text: func(component) -> result<string,
  list<wast-error>>`; plugins must fail rather than render lossy
  placeholders) and **`syntax-editor`** (`from-text: func(text, existing) ->
  result<wast-component, list<wast-error>>`). Two worlds:
  `syntax-renderer-world` (renderer only — read-only plugins) and
  `syntax-plugin-world` (renderer + editor — write-capable plugins).
  Hosts feature-detect editor support by the presence of the
  `syntax-editor` export and treat renderer-only views as read-only.
- Rust crates bind the shared types once via `wast_syntax_core::wit_types` and
  remap their generated bindings onto it with
  `[package.metadata.component.bindings] with = { "wast:types/types@0.1.0" = "wast_syntax_core::wit_types" }`.
- The WIT interfaces are the language-agnostic contract — anyone can
  implement them in any language that targets WASM Components.

## Storage and encoding formats

- **`wast.json`** — row-oriented JSON (each func/type row inlines its `uid`,
  ready for a 1:1 SQLite row mapping). The top-level object has a **required
  integer `version` field** (no serde default — files without it are
  rejected). Current schema version: **1** (`WastDb::CURRENT_VERSION`).
- **Function bodies** — `option<list<u8>>` in WIT. The byte layout is a single
  **format-version byte** (`BODY_FORMAT_VERSION = 1`) followed by the
  `postcard` encoding of `Vec<Instruction>` (see
  `crates/syntax-plugin/internal/pattern-analyzer/`). Decoders reject unknown
  versions. The same `Instruction` derive also produces the **JSON** form the
  structured write path edits (`ir-json` serializes `Vec<Instruction>` with
  serde_json); postcard and JSON differ only where format-adaptive
  serialization makes JSON readable (a `StringLiteral`'s bytes render as a
  string in JSON, raw bytes in postcard). One definition, two encodings — the
  golden-bytes test pins the postcard side so the JSON surface can never
  shift the on-disk format.
- **`wast.db`** — future SQLite format (same logical schema). The codec is
  byte-oriented: hosts pass full file contents in/out, because vscode-web's
  workspace fs has no partial r/w.
- **`world.wit` + `wast.json` + `syms.en.yaml` are SOURCE CODE** —
  hand-authored, or edited via a write syntax's text-pane round-trip
  (`from_text` → `merge` → `codec.write`) or the structured write path.
  Never design build-time generators that emit them. The canonical shared
  sample lives at `packages/sample-wast/`.

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
| **partial-manager** | extract / merge (stage 2 validation, including body validation). The structured write path: agents and tools edit the IR through extract → modify → merge, no text parsing involved |
| **syntax-renderer** | wast → text rendering (every plugin; read-only projection) |
| **syntax-editor** | text → wast parsing (write syntaxes only; stage 1 validation, new UID generation) |
| **mcp-server** | Exposes extract → ir-json edit → merge → compile as MCP tools. Owns path containment (everything stays under the server root) and nothing else |
| **wast-syntax-core** | Optional Rust scaffolding for Rust plugins. Not part of the WIT contract. See [docs/PLUGIN-AUTHORING.md](docs/PLUGIN-AUTHORING.md) |
| **CLI / Editor** | User operations and workflow control |

### partial-manager semantics (condensed)

`extract(full, targets)` builds a partial: every target is included; targets
*without* `include_caller` are forced to `Exported(uid)` (signature locked —
the partial can't prove all callers are visible); targets *with*
`include_caller` keep their original source and pull their direct callers in
(with bodies); callees are added as signature-only `Imported(uid)` stubs.
`merge(partial, full)` verifies `Imported`/`Exported` signatures against
`full`, replaces/adds `Internal` funcs, and validates every body the partial
contributes (calls resolve to a real func and name its params exactly; locals
are params, assignment targets, or match bindings) — deliberately only the
partial's own funcs, so pre-existing breakage elsewhere doesn't fail an
unrelated edit.

Every error message starts with a stable machine-readable `snake_case` code
followed by `": "`, with the uid in `location`. Codes may be added but never
renamed — agents parse them. The set: `signature_mismatch`, `uid_conflict`,
`missing_dependency`, `caller_not_included`, `invalid_body`, `unknown_local`,
`call_arity_mismatch`, `call_arg_unknown`, `call_arg_duplicate`. The
`ir-json` editor adds `parse_error`, `unsupported_version`, and
`duplicate_uid` on its side of the boundary.

Note that the compiler resolves call arguments **by name** against the
callee's params, so an arg-name mismatch is a body that would only fail at
compile time — which is why merge checks it.

### Compiler pipeline (condensed)

```
WastDb + synthesized WIT world
  → emit_core_module (core-only WAT)        # the hand-written part
  → wat::parse_str → core .wasm             # ← compile_core stops here
  → wit_component::embed_component_metadata
  → wit_component::ComponentEncoder         # shell: canon lift/lower, wiring
  → Component .wasm                         # ← compile
```

Three entry points on that one pipeline: `emit_wat` (the generated WAT text),
`compile_core` (the core module), and `compile` (the Component). The middle
one is what makes an in-browser demo possible — no browser can instantiate a
Component Model binary, which is what jco exists to work around, but the core
module underneath needs no host at all for an import-free program: it carries
its own `memory` and *defines* `cabi_realloc` rather than importing it. Note
that `compile`'s `world_wit` argument is unused; the world is synthesized
from the `WastDb`, so no `world.wit` is needed to compile.

Exports whose params and result each occupy a single core value are callable
from JS with no glue. Compound types still cross by the Canonical ABI: a
compound parameter flattens into several core values, and a `string` result
returns a pointer to an (address, length) pair in that memory — about twenty
lines of caller-side reading, which `packages/web-demo/src/playground.js`
does.

The compiler emits only the core module; the component shell is delegated to
`wit-component` (pinned at 0.219 to match wasmtime 27's wasmparser). The IR
stays a high-level semantic representation (not a core-opcode list) so syntax
plugins can round-trip it. Coverage today: numerics, control flow, calls
(internal + imported), option/result/variant/record/tuple/enum/flags,
string/list (params, returns, literals), nested compounds, resources
(exported + imported), heterogeneous narrows/widens. Remaining gaps are listed
in [crates/compiler/PLAN.md](crates/compiler/PLAN.md).

### Syntax plugin editor/renderer status

- **ir-json** (`syntax-plugin-world`): renderer + editor, and the surface the
  structured write path uses. Renders the IR as JSON with uids explicit and
  bodies as instruction trees; `from_text` needs no parser beyond
  `serde_json`. Omitted fields mean "unchanged" (body, display name, whole
  funcs), which is what lets a caller send a minimal edit.
- **raw**, **ts-like** (`syntax-plugin-world`): renderer + editor. Full body
  parsers (S-expression / recursive descent) — structural round-trip of
  signatures *and* bodies.
- **ruby-like**, **rust-like** (`syntax-renderer-world`): renderer-only.
  They project the IR as read-only text for review/diff and never parse
  text back. This is by design, not a gap: display syntaxes don't need a
  parser because edits flow through the structured write path.

### VS Code extension status

Bundles 8 components into `dist/components/` (5 syntax plugins,
partial-manager, codec, compiler). TreeView over workspace `wast.json` files;
`wast://` virtual docs via `FileSystemProvider`. Panes rendered by an
editor-capable plugin (raw, ts-like) are editable — save runs `from_text` →
`merge` → `codec.write`; renderer-only plugins (ruby-like, rust-like) get
read-only panes (`stat()` reports `FilePermission.Readonly`). fs.watch
refresh; `WAST: Compile current component` writes `<dir>/dist/<name>.wasm`.
Runs on Node (desktop) hosts; vscode-web support is future work (see
[packages/vscode-extension/PLAN.md](packages/vscode-extension/PLAN.md)).

### MCP server status

`packages/mcp-server/` bundles the same 8 components and exposes six tools
over stdio: `wast_list_components`, `wast_list_funcs`, `wast_read`,
`wast_write`, `wast_compile`, `wast_render`. Written against the SDK's
low-level `Server` with plain JSON Schema (no zod dependency); tool handlers
live in `src/tools.mjs` as plain functions over `{root, runtime}` so tests
drive them without a transport. Every path argument is resolved inside the
server root and refused if it escapes. See
[its README](packages/mcp-server/README.md) for the tool contract and the
error-code table.

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
# the MCP server has its own bundle:
pnpm --filter @wast/mcp-server build

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
- **Rendering is universal, parsing is exceptional** — every syntax renders
  (read-only projection for review/diff); only designated write syntaxes
  parse. New display syntaxes must NOT grow parsers — edits go through the
  structured write path (partial-manager extract/merge on the IR)
- **Identity is recorded, never inferred** — the write surface carries uids
  explicitly, so a rename is a syms edit and no layer has to guess whether an
  edit was a rename or a delete-plus-create
- **Validate at the boundary, not at compile time** — merge rejects a
  structurally broken body (unknown local, wrong call args) with a coded
  error, so a bad edit fails where it was made
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
