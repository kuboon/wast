//! Serde-compatible mirror types for the WIT-generated bindings.
//!
//! The WIT bindgen types don't derive Serialize/Deserialize, so we define
//! parallel types here with serde derives and convert between them.

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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SymEntry {
    pub uid: String,
    pub display_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Syms {
    pub wit_syms: Vec<(String, String)>,
    pub internal: Vec<SymEntry>,
    pub local: Vec<SymEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WastComponent {
    pub funcs: Vec<(String, WastFunc)>,
    pub types: Vec<(String, WastTypeDef)>,
    pub syms: Syms,
}
