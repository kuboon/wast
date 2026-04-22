//! v0.9 test: `MatchOption` / `MatchResult` on compound params.
//! Destructures Canonical-ABI flat-layout option/result values and binds
//! the payload to a named local for use in the some/ok branch.

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

fn local_get(uid: &str) -> Instruction {
    Instruction::LocalGet { uid: uid.into() }
}

#[test]
fn unwrap_or_on_option() {
    // unwrap-or(o: option<u32>, default: u32) -> u32
    //   { match o { some(x) => x, none => default } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "unwrap_or".into(),
            func: WastFunc {
                source: FuncSource::Exported("unwrap-or".into()),
                params: vec![
                    ("o".into(), "opt_u32".into()),
                    ("default".into(), "u32".into()),
                ],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::MatchOption {
                    value: Box::new(local_get("o")),
                    some_binding: "x".into(),
                    some_body: vec![local_get("x")],
                    none_body: vec![local_get("default")],
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
        .get_typed_func::<(Option<u32>, u32), (u32,)>(&mut store, "unwrap-or")
        .unwrap();

    assert_eq!(func.call(&mut store, (Some(42), 99)).unwrap(), (42,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (None, 99)).unwrap(), (99,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Some(0), 99)).unwrap(), (0,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn unwrap_or_default_on_result() {
    // unwrap-or-default(r: result<u32, u32>, default: u32) -> u32
    //   { match r { ok(x) => x, err(_) => default } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "unwrap_or_default".into(),
            func: WastFunc {
                source: FuncSource::Exported("unwrap-or-default".into()),
                params: vec![
                    ("r".into(), "res_u32".into()),
                    ("default".into(), "u32".into()),
                ],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::MatchResult {
                    value: Box::new(local_get("r")),
                    ok_binding: "v".into(),
                    ok_body: vec![local_get("v")],
                    err_binding: "_e".into(),
                    err_body: vec![local_get("default")],
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
        .get_typed_func::<(Result<u32, u32>, u32), (u32,)>(&mut store, "unwrap-or-default")
        .unwrap();

    assert_eq!(func.call(&mut store, (Ok(7), 99)).unwrap(), (7,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Err(123), 99)).unwrap(), (99,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn match_result_exposes_err_value() {
    // err-or-zero(r: result<u32, u32>) -> u32
    //   { match r { ok(_) => 0, err(e) => e } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "err_or_zero".into(),
            func: WastFunc {
                source: FuncSource::Exported("err-or-zero".into()),
                params: vec![("r".into(), "res_u32".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::MatchResult {
                    value: Box::new(local_get("r")),
                    ok_binding: "_o".into(),
                    ok_body: vec![Instruction::Const { value: 0 }],
                    err_binding: "e".into(),
                    err_body: vec![local_get("e")],
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
        .get_typed_func::<(Result<u32, u32>,), (u32,)>(&mut store, "err-or-zero")
        .unwrap();

    assert_eq!(func.call(&mut store, (Ok(42),)).unwrap(), (0,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Err(57),)).unwrap(), (57,));
    func.post_return(&mut store).unwrap();
}
