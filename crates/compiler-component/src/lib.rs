//! `compiler` WASM Component — wraps the `wast-compiler` rlib in the
//! WIT contract so editors / hosts can compile a `WastComponent` to a
//! `.wasm` Component without depending on the Rust crate directly.
//!
//! The wrapper is a one-function shell: convert the WIT-bindgen
//! `WastComponent` shape to `wast_types::WastDb` (the row-oriented serde
//! shape the compiler consumes), decode `world.wit` bytes as UTF-8, and
//! call `wast_compiler::compile`.

#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use crate::bindings::wast::compiler::types::{
    FuncSource as BindFuncSource, PrimitiveType as BindPrimitiveType, TypeSource as BindTypeSource,
    WastComponent, WastError, WastFunc as BindWastFunc, WastTypeDef as BindWastTypeDef,
    WitType as BindWitType,
};
use wast_types::{
    FuncSource, PrimitiveType, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow,
    WitType,
};

struct Component;

fn err(msg: impl Into<String>) -> WastError {
    WastError {
        message: msg.into(),
        location: None,
    }
}

// ---------------------------------------------------------------------------
// Bindings → wast-types serde shape
// ---------------------------------------------------------------------------

fn primitive(p: BindPrimitiveType) -> PrimitiveType {
    match p {
        BindPrimitiveType::U32 => PrimitiveType::U32,
        BindPrimitiveType::U64 => PrimitiveType::U64,
        BindPrimitiveType::I32 => PrimitiveType::I32,
        BindPrimitiveType::I64 => PrimitiveType::I64,
        BindPrimitiveType::F32 => PrimitiveType::F32,
        BindPrimitiveType::F64 => PrimitiveType::F64,
        BindPrimitiveType::Bool => PrimitiveType::Bool,
        BindPrimitiveType::Char => PrimitiveType::Char,
        BindPrimitiveType::String => PrimitiveType::String,
    }
}

fn func_source(s: BindFuncSource) -> FuncSource {
    match s {
        BindFuncSource::Internal(u) => FuncSource::Internal(u),
        BindFuncSource::Imported(u) => FuncSource::Imported(u),
        BindFuncSource::Exported(u) => FuncSource::Exported(u),
    }
}

fn type_source(s: BindTypeSource) -> TypeSource {
    match s {
        BindTypeSource::Internal(u) => TypeSource::Internal(u),
        BindTypeSource::Imported(u) => TypeSource::Imported(u),
        BindTypeSource::Exported(u) => TypeSource::Exported(u),
    }
}

fn wit_type(t: BindWitType) -> WitType {
    match t {
        BindWitType::Primitive(p) => WitType::Primitive(primitive(p)),
        BindWitType::Option(r) => WitType::Option(r),
        BindWitType::Result((ok, e)) => WitType::Result(ok, e),
        BindWitType::List(r) => WitType::List(r),
        BindWitType::Record(fields) => WitType::Record(fields),
        BindWitType::Variant(cases) => WitType::Variant(cases),
        BindWitType::Tuple(items) => WitType::Tuple(items),
        BindWitType::Enum(cases) => WitType::Enum(cases),
        BindWitType::Flags(cases) => WitType::Flags(cases),
        BindWitType::Resource => WitType::Resource,
        BindWitType::Own(r) => WitType::Own(r),
        BindWitType::Borrow(r) => WitType::Borrow(r),
    }
}

fn func(f: BindWastFunc) -> WastFunc {
    WastFunc {
        source: func_source(f.source),
        params: f.params,
        result: f.result,
        body: f.body,
    }
}

fn type_def(td: BindWastTypeDef) -> WastTypeDef {
    WastTypeDef {
        source: type_source(td.source),
        definition: wit_type(td.definition),
    }
}

/// Flatten the (uid, payload) tuple lists used at the WIT boundary into
/// the row-oriented `WastDb` the compiler consumes.
fn component_to_db(c: WastComponent) -> WastDb {
    let funcs: Vec<WastFuncRow> = c
        .funcs
        .into_iter()
        .map(|(uid, f)| WastFuncRow { uid, func: func(f) })
        .collect();
    let types: Vec<WastTypeRow> = c
        .types
        .into_iter()
        .map(|(uid, td)| WastTypeRow {
            uid,
            def: type_def(td),
        })
        .collect();
    WastDb { funcs, types }
}

// ---------------------------------------------------------------------------
// WIT Guest impl
// ---------------------------------------------------------------------------

impl bindings::exports::wast::compiler::compiler::Guest for Component {
    fn compile(component: WastComponent, world_wit: Vec<u8>) -> Result<Vec<u8>, WastError> {
        let world_str = std::str::from_utf8(&world_wit)
            .map_err(|e| err(format!("world.wit is not valid UTF-8: {e}")))?;
        let db = component_to_db(component);
        wast_compiler::compile(&db, world_str).map_err(|e| err(e.to_string()))
    }
}

bindings::export!(Component with_types_in bindings);
