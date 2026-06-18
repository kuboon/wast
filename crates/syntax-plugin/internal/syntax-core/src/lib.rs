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

use std::collections::BTreeMap;
use wast_types::{PrimitiveType, Syms, WastTypeDef, WitType};

/// Pre-built lookups a plugin needs to render text from a `WastComponent`.
///
/// Plugins build one of these once at the start of `to_text` and pass it
/// (immutably) through every recursive call.
pub struct RenderContext<'a> {
    pub func_names: BTreeMap<String, String>,
    pub local_names: BTreeMap<String, String>,
    pub type_names: BTreeMap<String, String>,
    pub types: &'a [(String, WastTypeDef)],
}

impl<'a> RenderContext<'a> {
    /// Build a context from the canonical `wast-types` shape. Plugins that
    /// receive WIT bindings from `wit-bindgen` should convert their
    /// `Syms` / type list to `wast-types` first (mechanical match-arm
    /// boilerplate, ~40 lines per plugin).
    pub fn new(syms: &Syms, types: &'a [(String, WastTypeDef)]) -> Self {
        let mut func_names = BTreeMap::new();
        let mut local_names = BTreeMap::new();
        let mut type_names = BTreeMap::new();

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

// ---------------------------------------------------------------------------
// Shared WIT bindings for `wast:types/types`
// ---------------------------------------------------------------------------

#[allow(warnings)]
#[rustfmt::skip]
mod wit_bindings {
    wit_bindgen::generate!({
        path: "../../../../wit-types",
        world: "types-world",
        generate_unused_types: true,
    });
}

/// The one shared Rust projection of the `wast:types/types` WIT interface.
///
/// Every Rust plugin remaps its generated bindings onto this module via
/// `[package.metadata.component.bindings] with = { "wast:types/types" =
/// "wast_syntax_core::wit_types" }`, so all plugins (and this crate's
/// scaffolding) speak the same concrete Rust types instead of each crate
/// owning a structurally-identical-but-distinct copy.
pub mod wit_types {
    pub use super::wit_bindings::wast::types::types::*;
}

/// Convert the shared WIT bindings types into the canonical `wast-types`
/// serde shape used by `RenderContext` / `TypePrinter`.
pub mod convert {
    use super::wit_types as bind;
    use wast_types as native;

    pub fn primitive(p: &bind::PrimitiveType) -> native::PrimitiveType {
        match p {
            bind::PrimitiveType::U32 => native::PrimitiveType::U32,
            bind::PrimitiveType::U64 => native::PrimitiveType::U64,
            bind::PrimitiveType::I32 => native::PrimitiveType::I32,
            bind::PrimitiveType::I64 => native::PrimitiveType::I64,
            bind::PrimitiveType::F32 => native::PrimitiveType::F32,
            bind::PrimitiveType::F64 => native::PrimitiveType::F64,
            bind::PrimitiveType::Bool => native::PrimitiveType::Bool,
            bind::PrimitiveType::Char => native::PrimitiveType::Char,
            bind::PrimitiveType::String => native::PrimitiveType::String,
        }
    }

    pub fn wit_type(t: &bind::WitType) -> native::WitType {
        match t {
            bind::WitType::Primitive(p) => native::WitType::Primitive(primitive(p)),
            bind::WitType::Option(uid) => native::WitType::Option(uid.clone()),
            bind::WitType::Result((ok, err)) => native::WitType::Result(ok.clone(), err.clone()),
            bind::WitType::List(uid) => native::WitType::List(uid.clone()),
            bind::WitType::Record(fields) => native::WitType::Record(fields.clone()),
            bind::WitType::Variant(cases) => native::WitType::Variant(cases.clone()),
            bind::WitType::Tuple(refs) => native::WitType::Tuple(refs.clone()),
            bind::WitType::Enum(cases) => native::WitType::Enum(cases.clone()),
            bind::WitType::Flags(cases) => native::WitType::Flags(cases.clone()),
            bind::WitType::Resource => native::WitType::Resource,
            bind::WitType::Own(t) => native::WitType::Own(t.clone()),
            bind::WitType::Borrow(t) => native::WitType::Borrow(t.clone()),
        }
    }

    pub fn type_source(s: &bind::TypeSource) -> native::TypeSource {
        match s {
            bind::TypeSource::Internal(s) => native::TypeSource::Internal(s.clone()),
            bind::TypeSource::Imported(s) => native::TypeSource::Imported(s.clone()),
            bind::TypeSource::Exported(s) => native::TypeSource::Exported(s.clone()),
        }
    }

    pub fn type_def(td: &bind::WastTypeDef) -> native::WastTypeDef {
        native::WastTypeDef {
            source: type_source(&td.source),
            definition: wit_type(&td.definition),
        }
    }

    pub fn type_list(
        types: &[(bind::TypeUid, bind::WastTypeDef)],
    ) -> Vec<(String, native::WastTypeDef)> {
        types
            .iter()
            .map(|(uid, td)| (uid.clone(), type_def(td)))
            .collect()
    }

    pub fn syms(s: &bind::Syms) -> native::Syms {
        native::Syms {
            wit_syms: s.wit_syms.clone(),
            internal: s
                .internal
                .iter()
                .map(|e| native::SymEntry {
                    uid: e.uid.clone(),
                    display_name: e.display_name.clone(),
                })
                .collect(),
            local: s
                .local
                .iter()
                .map(|e| native::SymEntry {
                    uid: e.uid.clone(),
                    display_name: e.display_name.clone(),
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared `from_text` scaffolding
// ---------------------------------------------------------------------------

/// Language-independent helpers each Rust plugin's `from_text` needs:
/// reverse name maps, existing-component lookups, collision-free UID
/// generation, and signature resolution. Plugins only keep the actual
/// surface-syntax parsing.
pub mod scaffold {
    use super::wit_types::{FuncSource, FuncUid, SymEntry, WastComponent, WastFunc, WitTypeRef};
    use std::collections::{BTreeMap, BTreeSet};

    /// The uid a func's source tag carries (internal/imported/exported all
    /// wrap the same string payload).
    pub fn source_uid_of(f: &WastFunc) -> &str {
        match &f.source {
            FuncSource::Internal(u) | FuncSource::Imported(u) | FuncSource::Exported(u) => {
                u.as_str()
            }
        }
    }

    /// Pre-built lookups over the `existing` component handed to
    /// `from_text`, shared by every plugin.
    pub struct ExistingIndex<'a> {
        /// func uid → func
        pub by_uid: BTreeMap<String, &'a WastFunc>,
        /// source uid → (func uid, func)
        pub by_source: BTreeMap<String, (&'a str, &'a WastFunc)>,
        /// Every uid already taken in the component (funcs, sources,
        /// types) — used to keep fresh UIDs collision-free.
        pub used_uids: BTreeSet<String>,
    }

    impl<'a> ExistingIndex<'a> {
        pub fn new(existing: &'a WastComponent) -> Self {
            let mut by_uid = BTreeMap::new();
            let mut by_source = BTreeMap::new();
            let mut used_uids = BTreeSet::new();
            for (uid, f) in &existing.funcs {
                by_uid.insert(uid.clone(), f);
                by_source.insert(source_uid_of(f).to_string(), (uid.as_str(), f));
                used_uids.insert(uid.clone());
                used_uids.insert(source_uid_of(f).to_string());
            }
            for (uid, _) in &existing.types {
                used_uids.insert(uid.clone());
            }
            for e in &existing.syms.internal {
                used_uids.insert(e.uid.clone());
            }
            for e in &existing.syms.local {
                used_uids.insert(e.uid.clone());
            }
            Self {
                by_uid,
                by_source,
                used_uids,
            }
        }

        /// Reverse map (display name → uid) from a uid → name map,
        /// e.g. `RenderContext::func_names`.
        pub fn reverse(names: &BTreeMap<String, String>) -> BTreeMap<String, String> {
            names.iter().map(|(k, v)| (v.clone(), k.clone())).collect()
        }

        /// Find the existing func a parsed signature refers to: source uid
        /// first (the tag `to_text` rendered), then func uid.
        pub fn find(&self, source_uid: &str, func_uid: &str) -> Option<&'a WastFunc> {
            self.by_source
                .get(source_uid)
                .map(|(_, f)| *f)
                .or_else(|| self.by_uid.get(func_uid).copied())
        }
    }

    /// Collision-free UID generator. Each `from_text` call creates one
    /// seeded with the existing component's uids, so a fresh uid can never
    /// shadow an existing func/type/local — even across plugin
    /// instantiations (the old static-counter scheme restarted at the same
    /// value every instantiation and could re-issue a uid that a previous
    /// save had already persisted).
    pub struct UidGen {
        used: BTreeSet<String>,
        counter: u32,
    }

    impl UidGen {
        pub fn new(used: BTreeSet<String>) -> Self {
            Self {
                used,
                counter: 0xa000,
            }
        }

        /// Return a uid not present in `used`, and reserve it. The
        /// counter keeps advancing past 0xffff (uids just grow a hex
        /// digit), so the generator never re-issues a taken uid.
        pub fn fresh(&mut self) -> String {
            loop {
                let candidate = format!("{:04x}", self.counter);
                self.counter = self.counter.wrapping_add(1);
                if self.used.insert(candidate.clone()) {
                    return candidate;
                }
            }
        }
    }

    /// Resolve a rendered function name back to `(func_uid, source_uid)`.
    ///
    /// Known names reuse existing uids (via syms reverse map, source uids,
    /// or func uids); unknown names get a fresh collision-free uid.
    pub fn resolve_func_uid(
        name: &str,
        rev_func: &BTreeMap<String, String>,
        existing: &ExistingIndex,
        uid_gen: &mut UidGen,
    ) -> (String, String) {
        if let Some(source_uid) = rev_func.get(name) {
            if let Some((func_uid, _)) = existing.by_source.get(source_uid.as_str()) {
                return (func_uid.to_string(), source_uid.clone());
            }
            return (source_uid.clone(), source_uid.clone());
        }
        // No syms entry overrides the rendered name. `to_text` falls back
        // to the source-val first, then the func-uid. Try both directions
        // before fabricating a fresh UID — otherwise body Calls referencing
        // the existing UID become dangling references.
        if let Some((func_uid, _)) = existing.by_source.get(name) {
            return (func_uid.to_string(), name.to_string());
        }
        if let Some(f) = existing.by_uid.get(name) {
            return (name.to_string(), source_uid_of(f).to_string());
        }
        let uid = uid_gen.fresh();
        (uid.clone(), uid)
    }

    /// Build the *per-function* reverse local map (display name → uid).
    ///
    /// `syms.local` is a flat component-wide list, but local uids are only
    /// meaningful within their own function. A single global reverse map
    /// lets one function's local name resolve to *another* function's uid
    /// whenever two functions display the same local name — so the map
    /// must be scoped to the uids the target function actually owns
    /// (params + locals referenced by its existing body).
    pub fn func_rev_local(
        local_names: &BTreeMap<String, String>,
        existing_func: Option<&WastFunc>,
    ) -> BTreeMap<String, String> {
        let mut rev = BTreeMap::new();
        let Some(f) = existing_func else {
            return rev;
        };
        let mut uids: BTreeSet<String> = f.params.iter().map(|(uid, _)| uid.clone()).collect();
        if let Some(body) = &f.body {
            if let Ok(instrs) = wast_pattern_analyzer::deserialize_body(body) {
                for i in &instrs {
                    collect_local_uids(i, &mut uids);
                }
            }
        }
        for uid in uids {
            if let Some(name) = local_names.get(&uid) {
                rev.insert(name.clone(), uid.clone());
            }
        }
        rev
    }

    fn collect_local_uids(instr: &wast_pattern_analyzer::Instruction, out: &mut BTreeSet<String>) {
        use wast_pattern_analyzer::Instruction as I;
        match instr {
            I::LocalGet { uid } => {
                out.insert(uid.clone());
            }
            I::LocalSet { uid, value } => {
                out.insert(uid.clone());
                collect_local_uids(value, out);
            }
            I::Call { args, .. } => {
                for (_, a) in args {
                    collect_local_uids(a, out);
                }
            }
            I::Compare { lhs, rhs, .. } | I::Arithmetic { lhs, rhs, .. } => {
                collect_local_uids(lhs, out);
                collect_local_uids(rhs, out);
            }
            I::If {
                condition,
                then_body,
                else_body,
            } => {
                collect_local_uids(condition, out);
                for c in then_body.iter().chain(else_body) {
                    collect_local_uids(c, out);
                }
            }
            I::Block { body, .. } | I::Loop { body, .. } => {
                for c in body {
                    collect_local_uids(c, out);
                }
            }
            I::BrIf { condition, .. } => collect_local_uids(condition, out),
            I::Some { value }
            | I::Ok { value }
            | I::Err { value }
            | I::IsErr { value }
            | I::StringLen { value }
            | I::ListLen { value } => collect_local_uids(value, out),
            I::RecordGet { value, .. } | I::TupleGet { value, .. } => {
                collect_local_uids(value, out)
            }
            I::RecordLiteral { fields } => {
                for (_, v) in fields {
                    collect_local_uids(v, out);
                }
            }
            I::TupleLiteral { values } | I::ListLiteral { values } => {
                for v in values {
                    collect_local_uids(v, out);
                }
            }
            I::VariantCtor { value, .. } => {
                if let Some(v) = value {
                    collect_local_uids(v, out);
                }
            }
            I::MatchVariant { value, arms } => {
                collect_local_uids(value, out);
                for arm in arms {
                    if let Some(b) = &arm.binding {
                        out.insert(b.clone());
                    }
                    for c in &arm.body {
                        collect_local_uids(c, out);
                    }
                }
            }
            I::MatchOption {
                value,
                some_binding,
                some_body,
                none_body,
            } => {
                out.insert(some_binding.clone());
                collect_local_uids(value, out);
                for c in some_body.iter().chain(none_body) {
                    collect_local_uids(c, out);
                }
            }
            I::MatchResult {
                value,
                ok_binding,
                ok_body,
                err_binding,
                err_body,
            } => {
                out.insert(ok_binding.clone());
                out.insert(err_binding.clone());
                collect_local_uids(value, out);
                for c in ok_body.iter().chain(err_body) {
                    collect_local_uids(c, out);
                }
            }
            I::ResourceNew { rep: value, .. }
            | I::ResourceRep { handle: value, .. }
            | I::ResourceDrop { handle: value, .. } => collect_local_uids(value, out),
            I::Nop
            | I::Return
            | I::Const { .. }
            | I::Br { .. }
            | I::None
            | I::StringLiteral { .. }
            | I::FlagsCtor { .. } => {}
        }
    }

    /// Resolve parameter `(name, type-string)` pairs to `(uid, type-ref)`,
    /// reusing syms-mapped uids where they exist and treating the rendered
    /// name as the uid otherwise (which is exactly what `to_text` emitted
    /// when no sym overrode it). `parse_type` is the plugin's
    /// surface-specific type-string parser.
    pub fn resolve_params(
        parsed: &[(String, String)],
        rev_local: &BTreeMap<String, String>,
        mut parse_type: impl FnMut(&str) -> WitTypeRef,
    ) -> Vec<(FuncUid, WitTypeRef)> {
        parsed
            .iter()
            .map(|(pname, ptype)| {
                let param_uid = rev_local
                    .get(pname.as_str())
                    .cloned()
                    .unwrap_or_else(|| pname.clone());
                (param_uid, parse_type(ptype))
            })
            .collect()
    }

    /// Ensure a sym entry exists for a function.
    pub fn ensure_func_sym(source_uid: &str, name: &str, syms_internal: &mut Vec<SymEntry>) {
        if !syms_internal.iter().any(|e| e.uid == source_uid) {
            syms_internal.push(SymEntry {
                uid: source_uid.to_string(),
                display_name: name.to_string(),
            });
        }
    }

    /// Split `s` on `delimiter` at top level (not inside any of `()`,
    /// `[]`, `{}`, or `<>` brackets). Respecting `{}` and `<>` is
    /// essential for rendered compound types like
    /// `record { x: u32, y: u32 }` and `option<result<u32, u64>>`.
    pub fn split_top_level(s: &str, delimiter: char) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut depth = 0i32;
        let mut start = 0;
        for (i, ch) in s.char_indices() {
            match ch {
                '(' | '[' | '{' | '<' => depth += 1,
                ')' | ']' | '}' | '>' => depth -= 1,
                c if c == delimiter && depth == 0 => {
                    parts.push(&s[start..i]);
                    start = i + ch.len_utf8();
                }
                _ => {}
            }
        }
        parts.push(&s[start..]);
        parts
    }
}

#[cfg(test)]
mod scaffold_tests {
    use super::scaffold::{ExistingIndex, UidGen, func_rev_local};
    use super::wit_types::{FuncSource, Syms, WastComponent, WastFunc};
    use std::collections::BTreeMap;

    fn comp_with_funcs(funcs: Vec<(String, WastFunc)>) -> WastComponent {
        WastComponent {
            funcs,
            types: vec![],
            syms: Syms {
                wit_syms: vec![],
                internal: vec![],
                local: vec![],
            },
        }
    }

    #[test]
    fn uid_gen_skips_existing_uids() {
        let comp = comp_with_funcs(vec![(
            "a000".to_string(),
            WastFunc {
                source: FuncSource::Internal("a001".to_string()),
                params: vec![],
                result: None,
                body: None,
            },
        )]);
        let index = ExistingIndex::new(&comp);
        let mut uid_gen = UidGen::new(index.used_uids.clone());
        // "a000" (func uid) and "a001" (source uid) are taken — the old
        // static counter would have re-issued "a000" here.
        assert_eq!(uid_gen.fresh(), "a002");
        assert_eq!(uid_gen.fresh(), "a003");
    }

    #[test]
    fn func_rev_local_scopes_to_the_functions_own_locals() {
        // Two params in *different* functions share the display name "x".
        let mut local_names = BTreeMap::new();
        local_names.insert("p1".to_string(), "x".to_string());
        local_names.insert("p2".to_string(), "x".to_string());

        let f1 = WastFunc {
            source: FuncSource::Internal("f1".to_string()),
            params: vec![("p1".to_string(), "t1".to_string())],
            result: None,
            body: None,
        };
        let f2 = WastFunc {
            source: FuncSource::Internal("f2".to_string()),
            params: vec![("p2".to_string(), "t1".to_string())],
            result: None,
            body: None,
        };

        let rev1 = func_rev_local(&local_names, Some(&f1));
        let rev2 = func_rev_local(&local_names, Some(&f2));
        assert_eq!(rev1.get("x").map(String::as_str), Some("p1"));
        assert_eq!(rev2.get("x").map(String::as_str), Some("p2"));

        // Unknown function: nothing to scope to.
        assert!(func_rev_local(&local_names, None).is_empty());
    }
}
