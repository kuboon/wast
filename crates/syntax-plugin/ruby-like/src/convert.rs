//! Convert this plugin's per-crate WIT bindings into the canonical
//! `wast-types` Rust shape so we can hand types to `wast-syntax-core`.
//!
//! Each `cargo component` build regenerates `bindings.rs` for this crate,
//! producing a `bindings::wast::core::types::*` module whose Rust types
//! are *structurally* identical to `wast-types`'s but distinct at the
//! Rust level. This file is the one-time bridge.

use crate::bindings::wast::core::types as bind;
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
