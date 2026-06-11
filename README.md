# wast

**wast** is an experiment in name-free source code for the WebAssembly
Component Model. A program is stored as a `WastComponent` — functions and
types identified only by stable, meaningless UIDs, with a typed instruction
IR for bodies — serialized to `wast.json` alongside a `world.wit` interface
and per-language `syms.*.yaml` files that hold nothing but human display
names. Pluggable *syntax plugins* render the same component as Ruby-like,
TypeScript-like, Rust-like, or raw S-expression text and parse edits back,
so the surface syntax (and every identifier's display name, per language) is
a view, not the source of truth. A compiler turns `wast.json` + `world.wit`
directly into a runnable WASM Component — display names are never needed to
produce code.

## Architecture

```
text  <──syntax plugin──>  partial/full WastComponent
partial WastComponent  <──partial-manager──>  full WastComponent
WastComponent  <──wast-codec──>  bytes (wast.json, world.wit, syms.en.yaml)
[wast.json, world.wit]  ──compiler──>  .wasm Component
```

The core is a Rust workspace whose crates are built as WASM Components
(`cargo component`) against a small set of WIT contracts (`wast:types`,
`wast:core`, `wast:codec`, `wast:compiler`). Those components are consumed by
two hosts:

- the **VS Code extension** (`packages/vscode-extension/`) — tree view over
  `wast.json` files, editable virtual documents (`wast://`) with a
  `from_text → merge → write` save flow, and a compile command, and
- the **web demo** (`packages/web-demo/`) — a browser playground using
  jco-transpiled components, deployed to GitHub Pages:
  <https://kuboon.github.io/wast/>.

Because the plugin boundary is a WIT interface, a syntax plugin can be
written in any language that targets WASM Components — see
[docs/PLUGIN-AUTHORING.md](docs/PLUGIN-AUTHORING.md).

## Getting started

Prerequisites:

- [mise](https://mise.jdx.dev/) — manages Node 24, pnpm 10, wasmtime
  (`mise.toml`); run `mise install` once
- Rust **1.96.0** — pinned via `rust-toolchain.toml` (with the
  `wasm32-wasip1` target; rustup picks this up automatically)
- `cargo-component` and `wasm-tools` — `cargo binstall cargo-component wasm-tools`
  (or `cargo install`)

Build everything (Rust components + TS packages):

```bash
mise run build
```

or per ecosystem:

```bash
cargo component build --workspace          # all wasm components
pnpm install && pnpm build                 # extension + web demo
```

Run the tests:

```bash
mise run ci            # what CI runs: build + cargo test + pnpm test
# or directly:
cargo test --workspace
pnpm test
```

## Project layout

| Path | What |
|---|---|
| `wit/`, `wit-types/`, `wit-codec/`, `wit-compiler/` | WIT contracts (`wast:core`, shared `wast:types`, `wast:codec`, `wast:compiler`) |
| `crates/wast-types/` | Shared serde types; defines the `wast.json` schema |
| `crates/wast-codec/` | `WastComponent` ↔ `wast.json` / `syms.en.yaml` codec component |
| `crates/partial-manager/` | Extract/merge partial components |
| `crates/compiler/` + `crates/compiler-component/` | wast → wasm Component compiler (rlib + WIT wrapper component) |
| `crates/syntax-plugin/{raw,ruby-like,ts-like,rust-like}/` | The 4 reference syntax plugins |
| `crates/syntax-plugin/internal/` | Shared plugin libraries (`pattern-analyzer` IR, `syntax-core` scaffolding) |
| `packages/vscode-extension/` | VS Code extension |
| `packages/web-demo/` | Browser playground ([live](https://kuboon.github.io/wast/)) |
| `packages/sample-wast/` | Canonical hand-authored sample component |
| `docs/PLUGIN-AUTHORING.md` | Guide to writing a new syntax plugin |
| `AGENTS.md` | Current-state guide for contributors and AI agents |

## License

[MIT](LICENSE)
