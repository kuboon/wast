//! Serde-compatible mirror types for the WIT-generated bindings.
//!
//! The WIT bindgen types don't derive Serialize/Deserialize, so we define
//! parallel types here with serde derives and convert between them.
//!
//! The on-disk layout is **row-oriented**: each func/type entry inlines its
//! `uid` alongside its payload, so JSON records map 1:1 to SQLite rows when
//! `wast.db` (SQLite) migration lands. Today the on-disk file is `wast.json`.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PrimitiveType {
    U32,
    U64,
    I32,
    I64,
    F32,
    F64,
    Bool,
    Char,
    String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum WitType {
    Primitive(PrimitiveType),
    Option(String),
    Result(String, String),
    List(String),
    Record(Vec<(String, String)>),
    Variant(Vec<(String, Option<String>)>),
    Tuple(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum FuncSource {
    Internal(String),
    Imported(String),
    Exported(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum TypeSource {
    Internal(String),
    Imported(String),
    Exported(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WastFunc {
    pub source: FuncSource,
    pub params: Vec<(String, String)>,
    pub result: Option<String>,
    pub body: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WastTypeDef {
    pub source: TypeSource,
    pub definition: WitType,
}

/// A row in the on-disk `funcs` table: uid + payload. Serializes flat so
/// every top-level key is a column when migrated to SQLite.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WastFuncRow {
    pub uid: String,
    #[serde(flatten)]
    pub func: WastFunc,
}

/// A row in the on-disk `types` table: uid + payload.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WastTypeRow {
    pub uid: String,
    #[serde(flatten)]
    pub def: WastTypeDef,
}

/// Sym entry used by syms YAML files.
#[derive(Clone, Debug)]
pub struct SymEntry {
    pub uid: String,
    pub display_name: String,
}

/// Syms structure used by syms YAML files (NOT stored in wast.json).
#[derive(Clone, Debug)]
pub struct Syms {
    pub wit_syms: Vec<(String, String)>,
    pub internal: Vec<SymEntry>,
    pub local: Vec<SymEntry>,
}

/// On-disk format for `wast.json` — syms are NOT stored here (they go in
/// `syms.*.yaml`). Row-oriented so each entry maps to a SQLite row when the
/// future `wast.db` (SQLite) migration lands.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WastDb {
    pub funcs: Vec<WastFuncRow>,
    pub types: Vec<WastTypeRow>,
}
