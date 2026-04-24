//! Shared serde types for WAST on-disk storage.
//!
//! The WIT bindgen types don't derive Serialize/Deserialize, so we define
//! parallel types here with serde derives. Each host crate (file-manager,
//! file-manager-hosted) keeps its own `binding_to_native` / `native_to_binding`
//! conversion functions.
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
    /// Named enumeration — all cases are payload-less. Stored as a u8 disc
    /// (for ≤256 cases) in the Canonical ABI.
    Enum(Vec<String>),
    /// Bitflag set — up to 32 flags for a single i32, 33-64 for i64.
    Flags(Vec<String>),
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
