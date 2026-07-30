//! `ir-json` — the IR itself as the surface syntax.
//!
//! Every other syntax plugin projects the IR into a *language*: names get
//! substituted for uids, control flow gets reshaped into `if`/`case`/`match`,
//! and parsing has to undo all of that. This plugin projects the IR into
//! JSON with **uids left explicit and instruction trees left structural**,
//! which is what makes it the write path for agents:
//!
//! - Identity is recorded, never inferred. Every func, type, param, and
//!   local carries its uid, so `from_text` never has to guess whether an
//!   edit was a rename or a delete-plus-create.
//! - Bodies are instruction trees, not text. No parser, no ambiguity — the
//!   caller edits the same tree the compiler consumes.
//! - Display names are separate, optional fields. An agent that doesn't care
//!   about names omits them and the existing syms survive untouched.
//!
//! The document mirrors `wast.json`'s serde shapes (type definitions are
//! rendered by `wast-types`' own derive) so the surface an agent edits and
//! the file on disk describe types identically.
//!
//! ## Document shape
//!
//! ```json
//! {
//!   "version": 1,
//!   "types": [
//!     {"uid": "point", "source": "internal", "wit_name": "point",
//!      "definition": {"Record": [["x", "u32"], ["y", "u32"]]}}
//!   ],
//!   "funcs": [
//!     {"uid": "get_x", "source": "exported", "wit_name": "get-x",
//!      "name": "get_x",
//!      "params": [{"uid": "p", "type": "point", "name": "p"}],
//!      "result": "u32",
//!      "body": [{"RecordGet": {"value": {"LocalGet": {"uid": "p"}},
//!                              "field": "x"}}]}
//!   ]
//! }
//! ```
//!
//! Instructions use serde's externally-tagged form — `{"Variant": {…}}`, or
//! a bare `"Nop"` / `"Return"` / `"None"` for payload-less ones. `Call.args`
//! is a list of `[param_uid, value]` pairs. Both come from the one shared
//! `Instruction` definition in `wast-pattern-analyzer`, so this surface can
//! never drift from the IR the compiler reads.
//!
//! ## Omission semantics
//!
//! Omitting a field means "leave it alone", which lets a caller send a
//! minimal edit rather than echoing the whole component back:
//!
//! | Omitted | Effect |
//! |---|---|
//! | `funcs` / `types` / `wit_syms` | inherited from `existing` |
//! | a func's `body` | body bytes preserved from `existing` (imports stay body-less) |
//! | `name` (func/type/param) or a `locals` entry | existing display name kept |
//! | `uid` on a new entry | a fresh collision-free uid is generated |
//!
//! An explicit empty string clears a display name; an explicit empty body
//! (`"body": []`) clears the body.
//!
//! The signature fields — `source`, `wit_name`, `params`, `result` — are all
//! **required**, and unknown fields are rejected. Because omission carries
//! meaning here, a typo'd or forgotten key would otherwise look exactly like
//! a deliberate omission: the edit would vanish and the write would still
//! report success.

#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use wast_pattern_analyzer::Instruction;
use wast_syntax_core::convert;
use wast_syntax_core::wit_types::*;

struct Component;

/// Document format version. Bumped only for incompatible shape changes;
/// `from_text` rejects anything else rather than guessing.
const DOC_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Document model
// ---------------------------------------------------------------------------

/// Unknown fields are rejected everywhere in this document. Omission is
/// meaningful here (it means "unchanged"), so a misspelled key would
/// otherwise be silently indistinguishable from a deliberate omission — the
/// edit would vanish and the write would report success.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Doc {
    version: u32,
    /// `None` (key absent) inherits `existing`'s types; `Some` replaces them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    types: Option<Vec<DocType>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    funcs: Option<Vec<DocFunc>>,
    /// WIT-path → display name (`syms.wit_syms`). Keys are unique WIT paths,
    /// so a map is lossless here and keeps the rendering deterministic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wit_syms: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
enum SourceKind {
    Internal,
    Imported,
    Exported,
}

/// A key that must be present, whose value may be `null`.
///
/// Needed because serde answers a *missing* key by handing the field a
/// deserializer that can only say "none" — so any field that asks for an
/// option (`Option<T>`, or a newtype that forwards to one) silently accepts
/// an absent key. For `result` that would mean a forgotten key erases a
/// return type. Asking for `any` instead makes a missing key an error while
/// still accepting an explicit `null`; serializing stays a bare `"u32"`.
#[derive(Serialize)]
#[serde(transparent)]
struct Required(Option<String>);

impl<'de> Deserialize<'de> for Required {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::Null => Ok(Required(std::option::Option::None)),
            serde_json::Value::String(s) => Ok(Required(Some(s))),
            other => Err(serde::de::Error::custom(format!(
                "expected a type uid or null, found {other}"
            ))),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocFunc {
    /// Absent on a newly-added func — a fresh uid is generated for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uid: Option<String>,
    source: SourceKind,
    /// The name carried by `func-source`: the WIT-level (kebab-case) name for
    /// imports/exports, and conventionally the uid for internal funcs.
    wit_name: String,
    /// Display name (`syms.internal`). Absent keeps the existing one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    params: Vec<DocParam>,
    /// Return type, or `null` for none. Required — like `params`, it is part
    /// of the signature, and letting it default would make a forgotten key
    /// silently erase a return type.
    result: Required,
    /// Display names for body-local uids (`syms.local`): uid → name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    locals: BTreeMap<String, String>,
    /// Instruction tree. Absent preserves the existing body bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<Vec<Instruction>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocParam {
    uid: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocType {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uid: Option<String>,
    source: SourceKind,
    wit_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// Rendered by `wast-types`' own serde derive, so it matches `wast.json`.
    definition: wast_types::WitType,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Build an error whose message starts with a stable machine-readable code.
fn coded(code: &str, msg: impl AsRef<str>, location: Option<String>) -> WastError {
    WastError {
        message: format!("{code}: {}", msg.as_ref()),
        location,
    }
}

// ---------------------------------------------------------------------------
// to_text — component → JSON document
// ---------------------------------------------------------------------------

fn sym_of<'a>(syms: &'a [SymEntry], uid: &str) -> Option<&'a str> {
    syms.iter()
        .find(|e| e.uid == uid)
        .map(|e| e.display_name.as_str())
}

fn source_of_func(source: &FuncSource) -> (SourceKind, String) {
    match source {
        FuncSource::Internal(n) => (SourceKind::Internal, n.clone()),
        FuncSource::Imported(n) => (SourceKind::Imported, n.clone()),
        FuncSource::Exported(n) => (SourceKind::Exported, n.clone()),
    }
}

fn source_of_type(source: &TypeSource) -> (SourceKind, String) {
    match source {
        TypeSource::Internal(n) => (SourceKind::Internal, n.clone()),
        TypeSource::Imported(n) => (SourceKind::Imported, n.clone()),
        TypeSource::Exported(n) => (SourceKind::Exported, n.clone()),
    }
}

/// Local uids a body defines or reads, so their display names can travel
/// with the func instead of being looked up in a global sym table.
fn body_local_uids(instructions: &[Instruction]) -> BTreeSet<String> {
    fn walk(instr: &Instruction, out: &mut BTreeSet<String>) {
        match instr {
            Instruction::LocalGet { uid } => {
                out.insert(uid.clone());
            }
            Instruction::LocalSet { uid, value } => {
                out.insert(uid.clone());
                walk(value, out);
            }
            Instruction::MatchOption {
                value,
                some_binding,
                some_body,
                none_body,
            } => {
                out.insert(some_binding.clone());
                walk(value, out);
                some_body.iter().for_each(|i| walk(i, out));
                none_body.iter().for_each(|i| walk(i, out));
            }
            Instruction::MatchResult {
                value,
                ok_binding,
                ok_body,
                err_binding,
                err_body,
            } => {
                out.insert(ok_binding.clone());
                out.insert(err_binding.clone());
                walk(value, out);
                ok_body.iter().for_each(|i| walk(i, out));
                err_body.iter().for_each(|i| walk(i, out));
            }
            Instruction::MatchVariant { value, arms } => {
                walk(value, out);
                for arm in arms {
                    if let Some(binding) = &arm.binding {
                        out.insert(binding.clone());
                    }
                    arm.body.iter().for_each(|i| walk(i, out));
                }
            }
            Instruction::Block { body, .. } | Instruction::Loop { body, .. } => {
                body.iter().for_each(|i| walk(i, out))
            }
            Instruction::If {
                condition,
                then_body,
                else_body,
            } => {
                walk(condition, out);
                then_body.iter().for_each(|i| walk(i, out));
                else_body.iter().for_each(|i| walk(i, out));
            }
            Instruction::BrIf { condition, .. } => walk(condition, out),
            Instruction::Call { args, .. } => args.iter().for_each(|(_, a)| walk(a, out)),
            Instruction::Compare { lhs, rhs, .. } | Instruction::Arithmetic { lhs, rhs, .. } => {
                walk(lhs, out);
                walk(rhs, out);
            }
            Instruction::Some { value }
            | Instruction::Ok { value }
            | Instruction::Err { value }
            | Instruction::IsErr { value }
            | Instruction::StringLen { value }
            | Instruction::ListLen { value }
            | Instruction::RecordGet { value, .. }
            | Instruction::TupleGet { value, .. } => walk(value, out),
            Instruction::ListLiteral { values } | Instruction::TupleLiteral { values } => {
                values.iter().for_each(|i| walk(i, out))
            }
            Instruction::RecordLiteral { fields } => fields.iter().for_each(|(_, v)| walk(v, out)),
            Instruction::VariantCtor { value, .. } => {
                if let Some(v) = value {
                    walk(v, out);
                }
            }
            Instruction::ResourceNew { rep, .. } => walk(rep, out),
            Instruction::ResourceRep { handle, .. } | Instruction::ResourceDrop { handle, .. } => {
                walk(handle, out)
            }
            Instruction::Br { .. }
            | Instruction::Return
            | Instruction::Const { .. }
            | Instruction::None
            | Instruction::StringLiteral { .. }
            | Instruction::FlagsCtor { .. }
            | Instruction::Nop => {}
        }
    }
    let mut out = BTreeSet::new();
    instructions.iter().for_each(|i| walk(i, &mut out));
    out
}

fn to_text_inner(component: &WastComponent) -> Result<String, Vec<WastError>> {
    let mut errors: Vec<WastError> = Vec::new();
    let mut funcs: Vec<DocFunc> = Vec::new();

    for (uid, func) in &component.funcs {
        let (source, wit_name) = source_of_func(&func.source);

        let body = match &func.body {
            Some(bytes) => match wast_pattern_analyzer::deserialize_body(bytes) {
                Ok(instructions) => Some(instructions),
                Err(e) => {
                    errors.push(coded(
                        "invalid_body",
                        format!("func '{uid}': {e}"),
                        Some(uid.clone()),
                    ));
                    continue;
                }
            },
            std::option::Option::None => std::option::Option::None,
        };

        // Local display names: params plus every uid the body touches.
        let mut locals: BTreeMap<String, String> = BTreeMap::new();
        let mut local_uids: BTreeSet<String> =
            func.params.iter().map(|(uid, _)| uid.clone()).collect();
        if let Some(instructions) = &body {
            local_uids.extend(body_local_uids(instructions));
        }
        for local_uid in &local_uids {
            // Param names ride on the param entry, not in `locals`.
            if func.params.iter().any(|(p, _)| p == local_uid) {
                continue;
            }
            if let Some(name) = sym_of(&component.syms.local, local_uid) {
                locals.insert(local_uid.clone(), name.to_string());
            }
        }

        funcs.push(DocFunc {
            uid: Some(uid.clone()),
            source,
            wit_name,
            name: sym_of(&component.syms.internal, uid).map(str::to_string),
            params: func
                .params
                .iter()
                .map(|(param_uid, ty)| DocParam {
                    uid: param_uid.clone(),
                    ty: ty.clone(),
                    name: sym_of(&component.syms.local, param_uid).map(str::to_string),
                })
                .collect(),
            result: Required(func.result.clone()),
            locals,
            body,
        });
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let types: Vec<DocType> = component
        .types
        .iter()
        .map(|(uid, def)| {
            let (source, wit_name) = source_of_type(&def.source);
            DocType {
                uid: Some(uid.clone()),
                source,
                wit_name,
                name: sym_of(&component.syms.internal, uid).map(str::to_string),
                definition: convert::wit_type(&def.definition),
            }
        })
        .collect();

    let doc = Doc {
        version: DOC_VERSION,
        types: Some(types),
        funcs: Some(funcs),
        wit_syms: Some(component.syms.wit_syms.iter().cloned().collect()),
    };

    serde_json::to_string_pretty(&doc)
        .map(|mut s| {
            s.push('\n');
            s
        })
        .map_err(|e| {
            vec![coded(
                "render_error",
                e.to_string(),
                std::option::Option::None,
            )]
        })
}

// ---------------------------------------------------------------------------
// from_text — JSON document → component
// ---------------------------------------------------------------------------

/// Apply one display-name edit to a sym list: `Some(name)` sets it, an empty
/// string removes it, `None` leaves whatever `existing` had.
fn apply_sym(syms: &mut Vec<SymEntry>, uid: &str, name: Option<&String>) {
    let Some(name) = name else { return };
    syms.retain(|e| e.uid != uid);
    if !name.is_empty() {
        syms.push(SymEntry {
            uid: uid.to_string(),
            display_name: name.clone(),
        });
    }
}

fn from_text_inner(text: &str, existing: WastComponent) -> Result<WastComponent, Vec<WastError>> {
    let doc: Doc = serde_json::from_str(text).map_err(|e| {
        vec![coded(
            "parse_error",
            e.to_string(),
            std::option::Option::None,
        )]
    })?;

    if doc.version != DOC_VERSION {
        return Err(vec![coded(
            "unsupported_version",
            format!(
                "document version {} (this build supports version {DOC_VERSION})",
                doc.version
            ),
            std::option::Option::None,
        )]);
    }

    let mut errors: Vec<WastError> = Vec::new();

    // Seed uid generation with every uid already in play so a generated uid
    // can't collide with an existing entity or with another new one.
    let mut used: BTreeSet<String> = existing
        .funcs
        .iter()
        .map(|(uid, _)| uid.clone())
        .chain(existing.types.iter().map(|(uid, _)| uid.clone()))
        .collect();
    if let Some(doc_funcs) = &doc.funcs {
        used.extend(doc_funcs.iter().filter_map(|f| f.uid.clone()));
    }
    if let Some(doc_types) = &doc.types {
        used.extend(doc_types.iter().filter_map(|t| t.uid.clone()));
    }
    let mut uid_gen = wast_syntax_core::scaffold::UidGen::new(used);

    let mut syms_internal = existing.syms.internal.clone();
    let mut syms_local = existing.syms.local.clone();

    // ── funcs ────────────────────────────────────────────────────────────
    let funcs: Vec<(FuncUid, WastFunc)> = match doc.funcs {
        std::option::Option::None => existing.funcs.clone(),
        Some(doc_funcs) => {
            let mut out: Vec<(FuncUid, WastFunc)> = Vec::new();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for doc_func in doc_funcs {
                let uid = doc_func.uid.clone().unwrap_or_else(|| uid_gen.fresh());
                if !seen.insert(uid.clone()) {
                    errors.push(coded(
                        "duplicate_uid",
                        format!("func '{uid}' appears more than once in the document"),
                        Some(uid.clone()),
                    ));
                    continue;
                }

                let source = match doc_func.source {
                    SourceKind::Internal => FuncSource::Internal(doc_func.wit_name.clone()),
                    SourceKind::Imported => FuncSource::Imported(doc_func.wit_name.clone()),
                    SourceKind::Exported => FuncSource::Exported(doc_func.wit_name.clone()),
                };

                let body = match &doc_func.body {
                    Some(instructions) => {
                        match wast_pattern_analyzer::try_serialize_body(instructions) {
                            Ok(bytes) => Some(bytes),
                            Err(e) => {
                                errors.push(coded(
                                    "invalid_body",
                                    format!("func '{uid}': {e}"),
                                    Some(uid.clone()),
                                ));
                                continue;
                            }
                        }
                    }
                    // Body omitted → keep whatever the component already had,
                    // *except* for imports. `extract` deliberately strips the
                    // bodies of callee stubs (the partial only carries their
                    // signature) and `merge` never writes an import's body
                    // back, so re-attaching one here would drag a func this
                    // edit doesn't own into merge's body validation.
                    std::option::Option::None => match doc_func.source {
                        SourceKind::Imported => std::option::Option::None,
                        SourceKind::Internal | SourceKind::Exported => existing
                            .funcs
                            .iter()
                            .find(|(fid, _)| *fid == uid)
                            .and_then(|(_, f)| f.body.clone()),
                    },
                };

                apply_sym(&mut syms_internal, &uid, doc_func.name.as_ref());
                for param in &doc_func.params {
                    apply_sym(&mut syms_local, &param.uid, param.name.as_ref());
                }
                for (local_uid, name) in &doc_func.locals {
                    apply_sym(&mut syms_local, local_uid, Some(name));
                }

                out.push((
                    uid,
                    WastFunc {
                        source,
                        params: doc_func
                            .params
                            .iter()
                            .map(|p| (p.uid.clone(), p.ty.clone()))
                            .collect(),
                        result: doc_func.result.0.clone(),
                        body,
                    },
                ));
            }
            out
        }
    };

    // ── types ────────────────────────────────────────────────────────────
    let types: Vec<(TypeUid, WastTypeDef)> = match doc.types {
        std::option::Option::None => existing.types.clone(),
        Some(doc_types) => {
            let mut out: Vec<(TypeUid, WastTypeDef)> = Vec::new();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for doc_type in doc_types {
                let uid = doc_type.uid.clone().unwrap_or_else(|| uid_gen.fresh());
                if !seen.insert(uid.clone()) {
                    errors.push(coded(
                        "duplicate_uid",
                        format!("type '{uid}' appears more than once in the document"),
                        Some(uid.clone()),
                    ));
                    continue;
                }
                let source = match doc_type.source {
                    SourceKind::Internal => TypeSource::Internal(doc_type.wit_name.clone()),
                    SourceKind::Imported => TypeSource::Imported(doc_type.wit_name.clone()),
                    SourceKind::Exported => TypeSource::Exported(doc_type.wit_name.clone()),
                };
                apply_sym(&mut syms_internal, &uid, doc_type.name.as_ref());
                out.push((
                    uid,
                    WastTypeDef {
                        source,
                        definition: convert::back::wit_type(&doc_type.definition),
                    },
                ));
            }
            out
        }
    };

    if !errors.is_empty() {
        return Err(errors);
    }

    let wit_syms = match doc.wit_syms {
        Some(map) => map.into_iter().collect(),
        std::option::Option::None => existing.syms.wit_syms.clone(),
    };

    Ok(WastComponent {
        funcs,
        types,
        syms: Syms {
            wit_syms,
            internal: syms_internal,
            local: syms_local,
        },
    })
}

// ---------------------------------------------------------------------------
// Guest implementations
// ---------------------------------------------------------------------------

impl bindings::exports::wast::core::syntax_renderer::Guest for Component {
    fn to_text(component: WastComponent) -> Result<String, Vec<WastError>> {
        to_text_inner(&component)
    }
}

impl bindings::exports::wast::core::syntax_editor::Guest for Component {
    fn from_text(text: String, existing: WastComponent) -> Result<WastComponent, Vec<WastError>> {
        from_text_inner(&text, existing)
    }
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;
    use bindings::exports::wast::core::syntax_editor::Guest as _;
    use bindings::exports::wast::core::syntax_renderer::Guest as _;
    use wast_pattern_analyzer::{ArithOp, MatchArm};

    fn sample() -> WastComponent {
        WastComponent {
            funcs: vec![
                (
                    "square".to_string(),
                    WastFunc {
                        source: FuncSource::Internal("square".to_string()),
                        params: vec![("x".to_string(), "u32".to_string())],
                        result: Some("u32".to_string()),
                        body: Some(wast_pattern_analyzer::serialize_body(&[
                            Instruction::Arithmetic {
                                op: ArithOp::Mul,
                                lhs: Box::new(Instruction::LocalGet {
                                    uid: "x".to_string(),
                                }),
                                rhs: Box::new(Instruction::LocalGet {
                                    uid: "x".to_string(),
                                }),
                            },
                        ])),
                    },
                ),
                (
                    "greet".to_string(),
                    WastFunc {
                        source: FuncSource::Exported("greet".to_string()),
                        params: vec![],
                        result: Some("string".to_string()),
                        body: Some(wast_pattern_analyzer::serialize_body(&[
                            Instruction::StringLiteral {
                                bytes: b"hello, wast!".to_vec(),
                            },
                        ])),
                    },
                ),
                (
                    "log".to_string(),
                    WastFunc {
                        source: FuncSource::Imported("log".to_string()),
                        params: vec![("msg".to_string(), "string".to_string())],
                        result: std::option::Option::None,
                        body: std::option::Option::None,
                    },
                ),
            ],
            types: vec![(
                "point".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("point".to_string()),
                    definition: WitType::Record(vec![
                        ("x".to_string(), "u32".to_string()),
                        ("y".to_string(), "u32".to_string()),
                    ]),
                },
            )],
            syms: Syms {
                wit_syms: vec![("log".to_string(), "log".to_string())],
                internal: vec![SymEntry {
                    uid: "square".to_string(),
                    display_name: "square".to_string(),
                }],
                local: vec![SymEntry {
                    uid: "x".to_string(),
                    display_name: "x".to_string(),
                }],
            },
        }
    }

    fn summarize(c: &WastComponent) -> String {
        let funcs: Vec<String> = c
            .funcs
            .iter()
            .map(|(uid, f)| {
                format!(
                    "{uid}|{:?}|{:?}|{:?}|{:?}",
                    f.source, f.params, f.result, f.body
                )
            })
            .collect();
        let types: Vec<String> = c
            .types
            .iter()
            .map(|(uid, t)| format!("{uid}|{:?}|{:?}", t.source, t.definition))
            .collect();
        format!("{funcs:?}{types:?}{:?}", c.syms.internal.len())
    }

    #[test]
    fn roundtrip_is_structurally_lossless() {
        let component = sample();
        let text = Component::to_text(component.clone()).unwrap();
        let parsed = Component::from_text(text, component.clone()).unwrap();
        assert_eq!(
            summarize(&parsed),
            summarize(&component),
            "JSON round-trip must preserve the component exactly"
        );
    }

    #[test]
    fn roundtrip_preserves_body_bytes_exactly() {
        let component = sample();
        let text = Component::to_text(component.clone()).unwrap();
        let parsed = Component::from_text(text, component.clone()).unwrap();
        for (uid, original) in &component.funcs {
            let (_, after) = parsed.funcs.iter().find(|(id, _)| id == uid).unwrap();
            assert_eq!(after.body, original.body, "body bytes changed for '{uid}'");
        }
    }

    #[test]
    fn render_is_deterministic() {
        let component = sample();
        let a = Component::to_text(component.clone()).unwrap();
        let b = Component::to_text(component).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn renders_uids_and_readable_string_literals() {
        let text = Component::to_text(sample()).unwrap();
        assert!(text.contains("\"uid\": \"square\""), "{text}");
        assert!(text.contains("\"wit_name\": \"greet\""), "{text}");
        assert!(text.contains("\"source\": \"imported\""), "{text}");
        // The string literal must be readable, not a byte array.
        assert!(text.contains("\"bytes\": \"hello, wast!\""), "{text}");
        // Type definitions use the same shape as wast.json.
        assert!(text.contains("\"Record\""), "{text}");
    }

    #[test]
    fn edits_a_body_from_hand_written_json() {
        // What an agent actually does: rewrite one func's instruction tree.
        let component = sample();
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "internal", "wit_name": "square",
             "params": [{"uid": "x", "type": "u32"}], "result": "u32",
             "body": [{"Arithmetic": {"op": "Add",
                        "lhs": {"LocalGet": {"uid": "x"}},
                        "rhs": {"LocalGet": {"uid": "x"}}}}]}
          ]
        }"#;
        let parsed = Component::from_text(json.to_string(), component).unwrap();
        assert_eq!(parsed.funcs.len(), 1);
        let body = parsed.funcs[0].1.body.as_ref().unwrap();
        let instrs = wast_pattern_analyzer::deserialize_body(body).unwrap();
        assert_eq!(
            instrs[0],
            Instruction::Arithmetic {
                op: ArithOp::Add,
                lhs: Box::new(Instruction::LocalGet { uid: "x".into() }),
                rhs: Box::new(Instruction::LocalGet { uid: "x".into() }),
            }
        );
        // Types weren't in the document → inherited from `existing`.
        assert_eq!(parsed.types.len(), 1);
    }

    #[test]
    fn omitted_body_preserves_existing_bytes() {
        let component = sample();
        let original = component.funcs[0].1.body.clone();
        // Signature-only edit: no `body` key at all.
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "internal", "wit_name": "square",
             "params": [{"uid": "x", "type": "u64"}], "result": "u64"}
          ]
        }"#;
        let parsed = Component::from_text(json.to_string(), component).unwrap();
        assert_eq!(parsed.funcs[0].1.body, original, "body must be preserved");
        assert_eq!(parsed.funcs[0].1.params[0].1, "u64", "param type edited");
    }

    #[test]
    fn explicit_empty_body_clears_it() {
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "internal", "wit_name": "square",
             "params": [], "result": null, "body": []}
          ]
        }"#;
        let parsed = Component::from_text(json.to_string(), sample()).unwrap();
        let body = parsed.funcs[0].1.body.as_ref().unwrap();
        assert!(
            wast_pattern_analyzer::deserialize_body(body)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn omitted_name_keeps_sym_and_empty_name_clears_it() {
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "internal", "wit_name": "square", "params": [], "result": null}
          ]
        }"#;
        let kept = Component::from_text(json.to_string(), sample()).unwrap();
        assert!(
            kept.syms.internal.iter().any(|e| e.uid == "square"),
            "omitted name must keep the existing sym"
        );

        let cleared_json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "internal", "wit_name": "square",
             "name": "", "params": [], "result": null}
          ]
        }"#;
        let cleared = Component::from_text(cleared_json.to_string(), sample()).unwrap();
        assert!(
            !cleared.syms.internal.iter().any(|e| e.uid == "square"),
            "explicit empty name must clear the sym"
        );
    }

    #[test]
    fn renames_via_name_field_without_touching_uid() {
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "internal", "wit_name": "square",
             "name": "squared", "params": [{"uid": "x", "type": "u32", "name": "value"}],
             "result": "u32"}
          ]
        }"#;
        let parsed = Component::from_text(json.to_string(), sample()).unwrap();
        assert_eq!(parsed.funcs[0].0, "square", "uid must be untouched");
        assert_eq!(
            parsed
                .syms
                .internal
                .iter()
                .find(|e| e.uid == "square")
                .unwrap()
                .display_name,
            "squared"
        );
        assert_eq!(
            parsed
                .syms
                .local
                .iter()
                .find(|e| e.uid == "x")
                .unwrap()
                .display_name,
            "value"
        );
    }

    #[test]
    fn generates_uid_for_new_func_without_one() {
        let json = r#"{
          "version": 1,
          "funcs": [
            {"source": "internal", "wit_name": "helper", "params": [], "result": null, "body": ["Nop"]}
          ]
        }"#;
        let parsed = Component::from_text(json.to_string(), sample()).unwrap();
        assert_eq!(parsed.funcs.len(), 1);
        let uid = &parsed.funcs[0].0;
        assert!(!uid.is_empty(), "a uid must be generated");
        assert_ne!(uid, "square", "generated uid must not collide");
    }

    #[test]
    fn local_display_names_survive_roundtrip() {
        let mut component = sample();
        component.funcs[0].1.body = Some(wast_pattern_analyzer::serialize_body(&[
            Instruction::LocalSet {
                uid: "acc".into(),
                value: Box::new(Instruction::Const { value: 0 }),
            },
            Instruction::MatchVariant {
                value: Box::new(Instruction::LocalGet { uid: "x".into() }),
                arms: vec![MatchArm {
                    case: "c".into(),
                    binding: Some("payload".into()),
                    body: vec![Instruction::LocalGet {
                        uid: "payload".into(),
                    }],
                }],
            },
        ]));
        component.syms.local.push(SymEntry {
            uid: "acc".into(),
            display_name: "accumulator".into(),
        });
        component.syms.local.push(SymEntry {
            uid: "payload".into(),
            display_name: "inner".into(),
        });

        let text = Component::to_text(component.clone()).unwrap();
        assert!(text.contains("\"accumulator\""), "{text}");
        assert!(text.contains("\"inner\""), "{text}");

        let parsed = Component::from_text(text, component).unwrap();
        for (uid, expected) in [("acc", "accumulator"), ("payload", "inner")] {
            assert_eq!(
                parsed
                    .syms
                    .local
                    .iter()
                    .find(|e| e.uid == uid)
                    .unwrap()
                    .display_name,
                expected
            );
        }
    }

    #[test]
    fn omitted_body_on_an_import_stays_absent() {
        // `extract` strips bodies from callee stubs and `merge` never writes
        // an import's body back. Inheriting one from `existing` would hand
        // merge a body belonging to a func this edit doesn't own, and its
        // validation would then judge it.
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "imported", "wit_name": "square",
             "params": [{"uid": "x", "type": "u32"}], "result": "u32"}
          ]
        }"#;
        let parsed = Component::from_text(json.to_string(), sample()).unwrap();
        assert_eq!(
            parsed.funcs[0].1.body,
            std::option::Option::None,
            "an import must not resurrect the body from `existing`"
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        // Omission means "unchanged" here, so a typo'd key must be an error
        // rather than a silently dropped edit.
        for json in [
            r#"{"version": 1, "func": []}"#,
            r#"{"version": 1, "funcs": [{"uid": "f", "source": "internal",
                 "wit_name": "f", "params": [], "result": null,
                 "bodies": ["Nop"]}]}"#,
            r#"{"version": 1, "funcs": [{"uid": "f", "source": "internal",
                 "wit_name": "f", "params": [{"uid": "p", "type": "u32", "nane": "p"}],
                 "result": null}]}"#,
        ] {
            let errs = Component::from_text(json.to_string(), sample()).unwrap_err();
            assert!(
                errs[0].message.starts_with("parse_error:"),
                "{json} → {errs:?}"
            );
        }
    }

    #[test]
    fn rejects_a_func_missing_its_result_field() {
        // `result` is part of the signature like `params`: defaulting it would
        // let a forgotten key erase a return type.
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "internal", "wit_name": "square",
             "params": [{"uid": "x", "type": "u32"}]}
          ]
        }"#;
        let errs = Component::from_text(json.to_string(), sample()).unwrap_err();
        assert!(errs[0].message.starts_with("parse_error:"), "{errs:?}");
    }

    #[test]
    fn renders_result_explicitly_for_void_funcs() {
        // `result` is required on the way in, so it has to be present on the
        // way out — including for funcs that return nothing.
        let text = Component::to_text(sample()).unwrap();
        assert!(text.contains("\"result\": null"), "{text}");
    }

    #[test]
    fn rejects_unknown_document_version() {
        let errs = Component::from_text(r#"{"version": 99, "funcs": []}"#.to_string(), sample())
            .unwrap_err();
        assert!(
            errs[0].message.starts_with("unsupported_version:"),
            "{errs:?}"
        );
    }

    #[test]
    fn rejects_malformed_json_with_coded_error() {
        let errs = Component::from_text("{ not json".to_string(), sample()).unwrap_err();
        assert!(errs[0].message.starts_with("parse_error:"), "{errs:?}");
    }

    #[test]
    fn rejects_unknown_instruction_shape() {
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "square", "source": "internal", "wit_name": "square",
             "params": [], "result": null, "body": [{"Teleport": {"to": "moon"}}]}
          ]
        }"#;
        let errs = Component::from_text(json.to_string(), sample()).unwrap_err();
        assert!(errs[0].message.starts_with("parse_error:"), "{errs:?}");
    }

    #[test]
    fn rejects_duplicate_uid() {
        let json = r#"{
          "version": 1,
          "funcs": [
            {"uid": "f", "source": "internal", "wit_name": "f", "params": [], "result": null},
            {"uid": "f", "source": "internal", "wit_name": "f", "params": [], "result": null}
          ]
        }"#;
        let errs = Component::from_text(json.to_string(), sample()).unwrap_err();
        assert!(errs[0].message.starts_with("duplicate_uid:"), "{errs:?}");
    }

    #[test]
    fn to_text_errors_on_undeserializable_body() {
        let mut component = sample();
        component.funcs[0].1.body = Some(vec![0xff, 0xfe]);
        let errs = Component::to_text(component).unwrap_err();
        assert!(errs[0].message.starts_with("invalid_body:"), "{errs:?}");
        assert_eq!(errs[0].location.as_deref(), Some("square"));
    }

    #[test]
    fn empty_component_roundtrips() {
        let component = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: Syms {
                wit_syms: vec![],
                internal: vec![],
                local: vec![],
            },
        };
        let text = Component::to_text(component.clone()).unwrap();
        let parsed = Component::from_text(text, component).unwrap();
        assert!(parsed.funcs.is_empty());
        assert!(parsed.types.is_empty());
    }
}
