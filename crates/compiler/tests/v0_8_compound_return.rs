//! v0.8 test: returning `option<T>` / `result<T, E>` (primitive payload).
//! Uses the v0.7 memory + cabi_realloc infra to allocate a return buffer
//! and write the Canonical-ABI variant layout (u8 disc + padded payload).

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

#[test]
fn return_some_u32() {
    // mk-some(x: u32) -> option<u32>  { Some(x) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_some".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-some".into()),
                params: vec![("x".into(), "u32".into())],
                result: Some("opt_u32".into()),
                body: Some(serialize_body(&[Instruction::Some {
                    value: Box::new(Instruction::LocalGet { uid: "x".into() }),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "opt_u32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("opt_u32".into()),
                definition: WitType::Option("u32".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (Option<u32>,)>(&mut store, "mk-some")
        .unwrap();
    assert_eq!(func.call(&mut store, (42,)).unwrap(), (Some(42),));
    func.post_return(&mut store).unwrap();
}

#[test]
fn return_none() {
    // mk-none() -> option<u32>  { None }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_none".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-none".into()),
                params: vec![],
                result: Some("opt_u32".into()),
                body: Some(serialize_body(&[Instruction::None])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "opt_u32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("opt_u32".into()),
                definition: WitType::Option("u32".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Option<u32>,)>(&mut store, "mk-none")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), (None,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn return_ok_u32() {
    // mk-ok(x: u32) -> result<u32, u32>  { Ok(x) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_ok".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-ok".into()),
                params: vec![("x".into(), "u32".into())],
                result: Some("res_u32".into()),
                body: Some(serialize_body(&[Instruction::Ok {
                    value: Box::new(Instruction::LocalGet { uid: "x".into() }),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "res_u32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("res_u32".into()),
                definition: WitType::Result("u32".into(), "u32".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (Result<u32, u32>,)>(&mut store, "mk-ok")
        .unwrap();
    assert_eq!(func.call(&mut store, (100,)).unwrap(), (Ok(100),));
    func.post_return(&mut store).unwrap();
}

#[test]
fn return_err_u32() {
    // mk-err(x: u32) -> result<u32, u32>  { Err(x) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_err".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-err".into()),
                params: vec![("x".into(), "u32".into())],
                result: Some("res_u32".into()),
                body: Some(serialize_body(&[Instruction::Err {
                    value: Box::new(Instruction::LocalGet { uid: "x".into() }),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "res_u32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("res_u32".into()),
                definition: WitType::Result("u32".into(), "u32".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (Result<u32, u32>,)>(&mut store, "mk-err")
        .unwrap();
    assert_eq!(func.call(&mut store, (7,)).unwrap(), (Err(7),));
    func.post_return(&mut store).unwrap();
}
