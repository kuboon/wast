//! Shared scaffolding for Rust-written `syntax-plugin` components.
//!
//! Each `syntax-plugin` is its own WASM Component implementing the
//! `wast:core/syntax-plugin` WIT interface. The interface is the
//! language-agnostic boundary — anyone can write a plugin in any language
//! that targets WASM Components by implementing `to-text` / `from-text`.
//!
//! This crate is **NOT** part of that contract. It is a Rust convenience
//! layer that lifts surface-syntax-independent work (name-map building,
//! WIT-type traversal, type-ref resolution) out of each Rust plugin so
//! they only contain their actual surface-syntax decisions.
//!
//! Plugins written in other languages would re-implement the equivalent
//! logic in those languages — the algorithms here are simple enough that
//! a direct port is a one-day job. See `docs/PLUGIN-AUTHORING.md`.
//!
//! # What this crate owns
//! - `RenderContext`: pre-built uid → display-name lookups for funcs,
//!   locals, and types, plus a borrowed view of the type definitions.
//! - `TypePrinter` trait: each plugin declares its lexical choices for
//!   each `WitType` variant (e.g. `option<T>` vs `Option<T>` vs `T?`).
//! - `format_wit_type` / `resolve_type_ref`: the shared traversal that
//!   walks a `WitType` and dispatches to the plugin's `TypePrinter`.
//!
//! Body-instruction rendering is **not** factored out by this crate.
//! Surface differences in control-flow (`case/when` vs `switch/case` vs
//! `match`) are structural, not just lexical, so each plugin keeps its
//! own `render_instruction` for now. If a useful pattern emerges across
//! plugins we can revisit.

use std::collections::HashMap;
use wast_types::{PrimitiveType, Syms, WastTypeDef, WitType};

/// Pre-built lookups a plugin needs to render text from a `WastComponent`.
///
/// Plugins build one of these once at the start of `to_text` and pass it
/// (immutably) through every recursive call.
pub struct RenderContext<'a> {
    pub func_names: HashMap<String, String>,
    pub local_names: HashMap<String, String>,
    pub type_names: HashMap<String, String>,
    pub types: &'a [(String, WastTypeDef)],
}

impl<'a> RenderContext<'a> {
    /// Build a context from the canonical `wast-types` shape. Plugins that
    /// receive WIT bindings from `wit-bindgen` should convert their
    /// `Syms` / type list to `wast-types` first (mechanical match-arm
    /// boilerplate, ~40 lines per plugin).
    pub fn new(syms: &Syms, types: &'a [(String, WastTypeDef)]) -> Self {
        let mut func_names = HashMap::new();
        let mut local_names = HashMap::new();
        let mut type_names = HashMap::new();

        // wit_syms: `(uid, display)` pairs — same map seeds both funcs and
        // types (a wit_path can refer to either).
        for (uid, name) in &syms.wit_syms {
            func_names.insert(uid.clone(), name.clone());
            type_names.insert(uid.clone(), name.clone());
        }
        // internal: per-component uids; same reasoning.
        for entry in &syms.internal {
            func_names.insert(entry.uid.clone(), entry.display_name.clone());
            type_names.insert(entry.uid.clone(), entry.display_name.clone());
        }
        // local: param/local names, separate namespace.
        for entry in &syms.local {
            local_names.insert(entry.uid.clone(), entry.display_name.clone());
        }

        Self {
            func_names,
            local_names,
            type_names,
            types,
        }
    }

    /// Resolve a func uid to its display name, falling back to the uid
    /// itself if no syms entry exists.
    pub fn func_name<'b>(&'b self, uid: &'b str) -> &'b str {
        self.func_names.get(uid).map(|s| s.as_str()).unwrap_or(uid)
    }

    pub fn local_name<'b>(&'b self, uid: &'b str) -> &'b str {
        self.local_names.get(uid).map(|s| s.as_str()).unwrap_or(uid)
    }

    pub fn type_name<'b>(&'b self, uid: &'b str) -> &'b str {
        self.type_names.get(uid).map(|s| s.as_str()).unwrap_or(uid)
    }
}

/// A plugin's surface-syntax choices for rendering each `WitType` shape.
///
/// The shared `format_wit_type` walker handles recursion; each method
/// here just decides how to format a node *given its already-rendered
/// children*. So plugins never re-walk the tree, they only fill in
/// language-specific lexemes.
///
/// # Example — three plugins for `option<u32>`
/// ```text
/// ruby-like:  option<u32>
/// ts-like:    u32 | null
/// rust-like:  Option<u32>
/// ```
/// All three call `format_wit_type` on the same `WitType::Option("u32")`,
/// the walker calls `printer.option("u32")`, and each impl returns the
/// surface-specific string.
pub trait TypePrinter {
    fn primitive(&self, p: &PrimitiveType) -> String;
    fn option(&self, inner: &str) -> String;
    fn result(&self, ok: &str, err: &str) -> String;
    fn list(&self, inner: &str) -> String;
    fn record(&self, fields: &[(String, String)]) -> String;
    fn variant(&self, cases: &[(String, Option<String>)]) -> String;
    fn tuple(&self, items: &[String]) -> String;
    fn enum_(&self, cases: &[String]) -> String;
    fn flags(&self, cases: &[String]) -> String;
    fn resource(&self) -> String;
    fn own(&self, target: &str) -> String;
    fn borrow(&self, target: &str) -> String;
}

/// Resolve a type reference (uid) to text.
///
/// If the uid maps to an inline definition in `ctx.types`, recursively
/// format that definition via the plugin's `TypePrinter` (so callers
/// see `record { x: u32, y: u32 }` rather than the bare uid). Otherwise
/// fall back to the display name from syms, then to the uid itself.
pub fn resolve_type_ref<P: TypePrinter>(
    type_ref: &str,
    ctx: &RenderContext,
    printer: &P,
) -> String {
    for (uid, td) in ctx.types {
        if uid == type_ref {
            return format_wit_type(&td.definition, ctx, printer);
        }
    }
    ctx.type_name(type_ref).to_string()
}

/// Walk a `WitType` and render it via the plugin's `TypePrinter`.
///
/// Recursion handles nested types automatically: `list<option<u32>>`
/// dispatches to `printer.list(printer.option(printer.primitive(U32)))`
/// without each plugin re-implementing the walk.
pub fn format_wit_type<P: TypePrinter>(t: &WitType, ctx: &RenderContext, printer: &P) -> String {
    match t {
        WitType::Primitive(p) => printer.primitive(p),
        WitType::Option(inner) => printer.option(&resolve_type_ref(inner, ctx, printer)),
        WitType::Result(ok, err) => printer.result(
            &resolve_type_ref(ok, ctx, printer),
            &resolve_type_ref(err, ctx, printer),
        ),
        WitType::List(inner) => printer.list(&resolve_type_ref(inner, ctx, printer)),
        WitType::Record(fields) => {
            let rendered: Vec<(String, String)> = fields
                .iter()
                .map(|(name, tref)| {
                    (
                        ctx.type_name(name).to_string(),
                        resolve_type_ref(tref, ctx, printer),
                    )
                })
                .collect();
            printer.record(&rendered)
        }
        WitType::Variant(cases) => {
            let rendered: Vec<(String, Option<String>)> = cases
                .iter()
                .map(|(name, tref)| {
                    (
                        ctx.type_name(name).to_string(),
                        tref.as_ref().map(|t| resolve_type_ref(t, ctx, printer)),
                    )
                })
                .collect();
            printer.variant(&rendered)
        }
        WitType::Tuple(items) => {
            let rendered: Vec<String> = items
                .iter()
                .map(|t| resolve_type_ref(t, ctx, printer))
                .collect();
            printer.tuple(&rendered)
        }
        WitType::Enum(cases) => printer.enum_(cases),
        WitType::Flags(cases) => printer.flags(cases),
        WitType::Resource => printer.resource(),
        WitType::Own(target) => printer.own(ctx.type_name(target)),
        WitType::Borrow(target) => printer.borrow(ctx.type_name(target)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wast_types::{SymEntry, TypeSource};

    struct DemoPrinter;
    impl TypePrinter for DemoPrinter {
        fn primitive(&self, p: &PrimitiveType) -> String {
            match p {
                PrimitiveType::U32 => "u32".into(),
                PrimitiveType::U64 => "u64".into(),
                PrimitiveType::I32 => "i32".into(),
                PrimitiveType::I64 => "i64".into(),
                PrimitiveType::F32 => "f32".into(),
                PrimitiveType::F64 => "f64".into(),
                PrimitiveType::Bool => "bool".into(),
                PrimitiveType::Char => "char".into(),
                PrimitiveType::String => "string".into(),
            }
        }
        fn option(&self, inner: &str) -> String {
            format!("option<{inner}>")
        }
        fn result(&self, ok: &str, err: &str) -> String {
            format!("result<{ok}, {err}>")
        }
        fn list(&self, inner: &str) -> String {
            format!("list<{inner}>")
        }
        fn record(&self, fields: &[(String, String)]) -> String {
            let body = fields
                .iter()
                .map(|(n, t)| format!("{n}: {t}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("record {{ {body} }}")
        }
        fn variant(&self, cases: &[(String, Option<String>)]) -> String {
            let body = cases
                .iter()
                .map(|(n, t)| match t {
                    Some(t) => format!("{n}({t})"),
                    None => n.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("variant {{ {body} }}")
        }
        fn tuple(&self, items: &[String]) -> String {
            format!("tuple<{}>", items.join(", "))
        }
        fn enum_(&self, cases: &[String]) -> String {
            format!("enum {{ {} }}", cases.join(", "))
        }
        fn flags(&self, cases: &[String]) -> String {
            format!("flags {{ {} }}", cases.join(", "))
        }
        fn resource(&self) -> String {
            "resource".into()
        }
        fn own(&self, t: &str) -> String {
            format!("own<{t}>")
        }
        fn borrow(&self, t: &str) -> String {
            format!("borrow<{t}>")
        }
    }

    #[test]
    fn render_context_resolves_names_with_uid_fallback() {
        let syms = Syms {
            wit_syms: vec![("ns/foo".into(), "foo".into())],
            internal: vec![SymEntry {
                uid: "f1".into(),
                display_name: "alpha".into(),
            }],
            local: vec![SymEntry {
                uid: "x".into(),
                display_name: "input".into(),
            }],
        };
        let ctx = RenderContext::new(&syms, &[]);
        assert_eq!(ctx.func_name("ns/foo"), "foo");
        assert_eq!(ctx.func_name("f1"), "alpha");
        assert_eq!(ctx.local_name("x"), "input");
        assert_eq!(ctx.local_name("missing"), "missing"); // fallback
    }

    #[test]
    fn format_wit_type_recurses_through_nested_refs() {
        // option<list<u32>>: walks inner list, then primitive.
        let types = vec![
            (
                "list_u32".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("list_u32".into()),
                    definition: WitType::List("u32".into()),
                },
            ),
            (
                "opt_list".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("opt_list".into()),
                    definition: WitType::Option("list_u32".into()),
                },
            ),
        ];
        let ctx = RenderContext::new(
            &Syms {
                wit_syms: vec![],
                internal: vec![],
                local: vec![],
            },
            &types,
        );
        let out = format_wit_type(&WitType::Option("list_u32".into()), &ctx, &DemoPrinter);
        assert_eq!(out, "option<list<u32>>");
    }

    #[test]
    fn format_wit_type_records_with_inline_field_types() {
        let types: Vec<(String, WastTypeDef)> = vec![];
        let ctx = RenderContext::new(
            &Syms {
                wit_syms: vec![],
                internal: vec![],
                local: vec![],
            },
            &types,
        );
        let t = WitType::Record(vec![("x".into(), "u32".into()), ("y".into(), "u32".into())]);
        let out = format_wit_type(&t, &ctx, &DemoPrinter);
        assert_eq!(out, "record { x: u32, y: u32 }");
    }
}
