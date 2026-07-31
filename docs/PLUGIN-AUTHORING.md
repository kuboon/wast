# Authoring a syntax plugin

This guide explains what it takes to ship a new `wast` syntax plugin —
e.g. `python-like`, `swift-like`, or any surface syntax of your choice.

**New plugins should be renderers.** A syntax plugin's job is to *project*
a `wast-component` as readable text — for humans reviewing and diffing
code. Parsing text back is the exception, not the rule: edits to a wast
program flow through the structured write path — `partial-manager`
extract/merge with [`ir-json`](../crates/syntax-plugin/ir-json/) as the
surface — so a display syntax never needs a parser. Do not grow one.

## The contract

The plugin boundary is two WIT interfaces (defined in `wit/wast-core.wit`):

```wit
interface syntax-renderer {
  use wast:types/types@0.1.0.{wast-component, wast-error};
  to-text:   func(component: wast-component)
                -> result<string, list<wast-error>>;
}

interface syntax-editor {
  use wast:types/types@0.1.0.{wast-component, wast-error};
  from-text: func(text: string, existing: wast-component)
                -> result<wast-component, list<wast-error>>;
}
```

and two worlds:

- **`syntax-renderer-world`** — exports only `syntax-renderer`. This is
  what a new plugin targets. Hosts render its text as a **read-only**
  view (the VS Code extension marks the pane readonly; the web demo
  disables Sync).
- **`syntax-plugin-world`** — exports both interfaces. Only designated
  write syntaxes target this (today: `raw` and `ts-like`). Hosts
  feature-detect editability by the presence of the `syntax-editor`
  export.

The shared `types` interface lives in its own package
(`wit-types/types.wit`, package `wast:types`) and is `use`d by
`wast:core`, `wast:codec`, and `wast:compiler` — one definition, no
copies to drift.

That's the entire boundary. Plugins are independent components — each
plugin's `.wasm` artifact is self-contained, and plugins do not depend
on each other or share runtime state.

This means a plugin can be written in **any** language that targets
WASM Components: Rust, Go (TinyGo), C, JavaScript, MoonBit (when
out-of-experimental), eventually wast itself. The shared `WastComponent`
data shape (defined as WIT records and variants) is what every plugin
sees.

## What a renderer must do

### `to_text(component)`

Render a `wast-component` (uids, types, funcs, syms, bodies) as a string
of text in the plugin's surface syntax. Typical work:

1. Build name lookups from `component.syms` so funcs/locals/types render
   as their display names rather than as raw uids.
2. Walk `component.types`, formatting each `wit-type` variant into the
   surface's lexemes (e.g. `option<T>` vs `Option<T>` vs `T?`).
3. Walk `component.funcs`, formatting signatures and bodies. Bodies are
   bytes; a plugin typically uses `wast-pattern-analyzer` (or equivalent
   logic in another language) to deserialize them into an `Instruction`
   tree, then render that tree.

Returns `result<string, list<wast-error>>`. A plugin must **fail** (not
render a lossy placeholder) when it cannot faithfully render — e.g. a
body that does not deserialize — otherwise the reader would silently see
incomplete code.

Rendering must also be **deterministic**: the same component renders to
the same text, so diffs of rendered text are meaningful.

## Write syntaxes: `from_text(text, existing)`

Only applies to plugins targeting `syntax-plugin-world`. Parse a text
string back into a `wast-component`. The `existing` component is
provided as a hint — when the user hasn't edited a particular func /
type, the plugin should preserve the original entry (uid + body bytes)
verbatim rather than inventing fresh uids.

Returns a `result<wast-component, list<wast-error>>`. Errors carry a
human-readable message and an optional location.

Adding a new write syntax is a project-level decision, not a plugin
checklist item: a parser must handle broken intermediate states, keep
uid identity stable through edits, and stay lossless — the exact costs
the renderer-only rule exists to avoid.

`ir-json` is the write syntax that sidesteps those costs entirely, and
it's worth reading as the reference: because its surface *is* the IR —
uids explicit, bodies as `Instruction` trees, type definitions rendered by
`wast-types`' own derive — its `from_text` needs no parser beyond
`serde_json`, and identity never has to be inferred. That's the bar a new
write syntax has to clear.

## Rust plugins: `wast-syntax-core`

The 5 reference plugins under `crates/syntax-plugin/` are written in Rust
(`ir-json`, `raw`, and `ts-like` are editors; `ruby-like` and `rust-like`
are renderer-only). They share two internal helper crates:

- **`wast-pattern-analyzer`** — defines the `Instruction` tree and
  `serialize_body` / `deserialize_body`. Used by every plugin and by
  `compiler` / `partial-manager` / `demo-gen` too.
- **`wast-syntax-core`** — Rust scaffolding: `wit_types` (the one shared
  Rust projection of the `wast:types/types` WIT interface — plugins
  remap their generated bindings onto it via
  `[package.metadata.component.bindings] with`), `convert` (wit_types →
  `wast-types` serde shapes), `RenderContext` (uid → display-name maps),
  `TypePrinter` trait (per-plugin lexical choices for each `wit-type`
  variant), `format_wit_type` / `resolve_type_ref` walkers, and the
  `scaffold` module (editor-side helpers: `ExistingIndex`, collision-free
  `UidGen`, per-function reverse local maps, signature/param resolution —
  renderer-only plugins don't need it), plus `convert::back` for the
  serde-native → WIT-bindings direction.

A typical Rust renderer's `to_text` looks like:

```rust
use wast_syntax_core::wit_types::*; // the shared bindings types
use wast_syntax_core::{RenderContext, convert};

fn to_text(component: WastComponent) -> Result<String, Vec<WastError>> {
    let native_syms  = convert::syms(&component.syms);
    let native_types = convert::type_list(&component.types);
    let ctx          = RenderContext::new(&native_syms, &native_types);

    let mut parts = Vec::new();
    for (uid, func) in &component.funcs {
        parts.push(func_to_text(uid, func, &ctx)?);
    }
    Ok(parts.join("\n\n"))
}

struct MyTypePrinter;
impl TypePrinter for MyTypePrinter {
    fn option(&self, inner: &str) -> String { format!("{inner}?") }
    fn record(&self, fields: &[(String, String)]) -> String {
        let body = fields.iter().map(|(n, t)| format!("{n}: {t}"))
            .collect::<Vec<_>>().join(", ");
        format!("{{ {body} }}")
    }
    // ... fill in the rest of the trait
}
```

Each plugin's `Cargo.toml` carries

```toml
[package.metadata.component.target]
path = "../../../wit"
world = "syntax-renderer-world"   # editors: "syntax-plugin-world"

[package.metadata.component.target.dependencies]
"wast:types" = { path = "../../../wit-types" }

[package.metadata.component.bindings]
with = { "wast:types/types@0.1.0" = "wast_syntax_core::wit_types" }
```

so `cargo component` generates the plugin's bindings *against* the one
shared types module instead of emitting a structurally-identical-but-
distinct copy per crate. That is what lets every plugin reuse
`wast_syntax_core::{convert, scaffold}` directly.

`wast-syntax-core` does **not** factor out body-instruction rendering.
Surface differences in control-flow constructs (Ruby's `case/when`,
TS's `switch/case`, Rust's `match`) are structural, not just lexical,
so each plugin still owns its `render_instruction`. A future
`BodyPrinter` visitor may land if a clean abstraction emerges.

## Plugins in other languages

Because the WIT contract is the boundary, other-language plugins do
**not** depend on `wast-syntax-core`. They re-implement the equivalent
logic in their language. The pieces to port are small:

| Concern | Algorithm | Lines (Rust ref impl) |
|---|---|---|
| Build uid → name maps from `syms` | iterate three lists, fold into hash maps | ~25 |
| Resolve a type uid to text | look up in `types`, recurse via `format_wit_type`, fallback to display name then uid | ~10 |
| Walk a `wit-type` and format it | `match` on the variant, recurse into refs, defer lexical choices to a printer | ~30 |
| Decode a body to an `Instruction` tree | body bytes are a single format-version byte (currently `1`) followed by the `postcard` encoding of the instruction list (`wast-pattern-analyzer`'s `serialize_body` / `deserialize_body`); `postcard` is a serde-compatible binary format with implementations in every major language | varies |

A Go or TinyGo plugin would lift the `wast-syntax-core` patterns into
Go interfaces; a JS plugin into class methods; etc. None of this changes
the `.wasm` artifact contract — the host still calls
`to_text(component)` on the component's `syntax-renderer` export.

## Plugins written in wast itself

A long-term goal: the IR types and functions are themselves expressible
in wast, so a plugin author can edit a `.wast.json` file describing
their `to_text` implementation. When this lands, the visitor pattern
collapses into wast's own variant pattern-matching and recursion
primitives — no helper crate needed, since the operations are
first-class in the language.

## Checklist for shipping a Rust renderer

1. New crate at `crates/syntax-plugin/<name>/` with the standard
   `Cargo.toml` (cdylib, depends on `wit-bindgen`, `wit-bindgen-rt`,
   `wast-syntax-core`, `wast-pattern-analyzer` — plus `wast-types` if you
   use the serde-native shapes directly — with
   `package.metadata.component.target` pointing at `wit/` plus the
   `wast:types` target dependency and the `bindings.with` remap shown
   above, and world **`syntax-renderer-world`**).
2. `src/lib.rs`:
   - `struct MyTypePrinter; impl TypePrinter for MyTypePrinter { ... }`.
   - `impl bindings::exports::wast::core::syntax_renderer::Guest` wiring
     `RenderContext` to a `func_to_text` helper.
   - Body rendering — surface-specific.
3. Tests under `#[cfg(test)] mod tests` — at minimum: signature
   rendering, body rendering of each instruction shape, deterministic
   render (two renders agree), error on undeserializable body.
4. If you want the demo to render your plugin, add an entry to the
   `targets` array in `packages/web-demo/scripts/build-plugins.mjs` and
   the `PLUGINS` array in `packages/web-demo/src/main.js` (with the
   read-only capability line).
