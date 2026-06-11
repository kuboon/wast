# Authoring a syntax plugin

This guide explains what it takes to ship a new `wast` syntax plugin —
e.g. `python-like`, `swift-like`, or any surface syntax of your choice.

## The contract

A syntax plugin is a WASM Component that exports the
**`wast:core/syntax-plugin`** interface (defined in `wit/wast-core.wit`):

```wit
interface syntax-plugin {
  use wast:types/types@0.1.0.{wast-component, wast-error};
  to-text:   func(component: wast-component)
                -> result<string, list<wast-error>>;
  from-text: func(text: string, existing: wast-component)
                -> result<wast-component, list<wast-error>>;
}
```

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

## What a plugin must do

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
body that does not deserialize — otherwise the next `from_text` would
silently drop content.

### `from_text(text, existing)`

Parse a text string back into a `wast-component`. The `existing`
component is provided as a hint — when the user hasn't edited a
particular func / type, the plugin should preserve the original entry
(uid + body bytes) verbatim rather than inventing fresh uids.

Returns a `result<wast-component, list<wast-error>>`. Errors carry a
human-readable message and an optional location.

## Rust plugins: `wast-syntax-core`

The 4 reference plugins under `crates/syntax-plugin/{raw,ruby-like,
ts-like,rust-like}/` are written in Rust. They share two internal
helper crates:

- **`wast-pattern-analyzer`** — defines the `Instruction` tree and
  `serialize_body` / `deserialize_body`. Used by every plugin and by
  `compiler` / `partial-manager` / `demo-gen` too.
- **`wast-syntax-core`** — Rust scaffolding for the plugin-specific
  half: `wit_types` (the one shared Rust projection of the
  `wast:types/types` WIT interface — plugins remap their generated
  bindings onto it via `[package.metadata.component.bindings] with`),
  `convert` (wit_types → `wast-types` serde shapes), `RenderContext`
  (uid → display-name maps), `TypePrinter` trait (per-plugin lexical
  choices for each `wit-type` variant), `format_wit_type` /
  `resolve_type_ref` walkers, and the `scaffold` module
  (`ExistingIndex`, collision-free `UidGen`, per-function reverse local
  maps, signature/param resolution helpers shared by every plugin's
  `from_text`).

A typical Rust plugin's `to_text` looks like:

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
| Decode a body to an `Instruction` tree | `wast-pattern-analyzer` uses `postcard` (a serde-compatible binary format) — implementations exist in every major language | varies |

A Go or TinyGo plugin would lift the `wast-syntax-core` patterns into
Go interfaces; a JS plugin into class methods; etc. None of this changes
the `.wasm` artifact contract — the host still calls
`to_text(component)` and `from_text(text, existing)` on the component.

## Plugins written in wast itself

A long-term goal: the IR types and functions are themselves expressible
in wast, so a plugin author can edit a `.wast.json` file describing
their `to_text` / `from_text` implementation. When this lands, the
visitor pattern collapses into wast's own variant pattern-matching and
recursion primitives — no helper crate needed, since the operations
are first-class in the language.

## Checklist for shipping a Rust plugin

1. New crate at `crates/syntax-plugin/<name>/` with the standard
   `Cargo.toml` (cdylib, depends on `wit-bindgen`, `wast-types`,
   `wast-syntax-core`, `wast-pattern-analyzer`, with
   `package.metadata.component.target` pointing at `wit/` plus the
   `wast:types` target dependency and the `bindings.with` remap shown
   above, and world `syntax-plugin-world`).
2. `src/lib.rs`:
   - `struct MyTypePrinter; impl TypePrinter for MyTypePrinter { ... }`.
   - `to_text` / `from_text` Guest impl wiring `RenderContext` to
     `func_to_text` and `parse_func` helpers.
   - Body rendering and parsing — surface-specific.
4. Tests under `#[cfg(test)] mod tests` — at minimum: signature
   round-trip, body-preservation round-trip, sym-rename round-trip.
5. If you want the demo to render your plugin, add an entry to
   `packages/web-demo/scripts/build-plugins.mjs` and the `PLUGINS`
   array in `packages/web-demo/src/main.js`.
