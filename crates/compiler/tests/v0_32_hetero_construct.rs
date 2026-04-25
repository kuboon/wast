//! v0.32: heterogeneous result/variant *construction* at return position.
//! Fixes a pre-existing payload-offset bug — the constructors used the
//! selected case's own alignment, but the Canonical ABI memory layout
//! places the payload at align_up(1, max_case_align). For homogeneous
//! variants the two are equal so the bug was invisible; heterogeneous
//! variants like `result<u32, u64>` would miss-align Ok payloads.

use wasmtime::component::{Component, ComponentType, Lift, Linker, Lower};
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

// ---------------------------------------------------------------------------
// result<u32, u64> built from a u32/u64 param. The u32 case must round-trip
// at the variant's max-aligned offset (8), not the case's own offset (4).
// ---------------------------------------------------------------------------

#[test]
fn ok_u32_in_result_u32_u64() {
    // mk-ok(x: u32) -> result<u32, u64>  { Ok(x) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_ok".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-ok".into()),
                params: vec![("x".into(), "u32".into())],
                result: Some("res_u32_u64".into()),
                body: Some(serialize_body(&[Instruction::Ok {
                    value: Box::new(Instruction::LocalGet { uid: "x".into() }),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "res_u32_u64".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("res_u32_u64".into()),
                definition: WitType::Result("u32".into(), "u64".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (Result<u32, u64>,)>(&mut store, "mk-ok")
        .unwrap();

    assert_eq!(func.call(&mut store, (42u32,)).unwrap(), (Ok(42u32),));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (u32::MAX,)).unwrap(), (Ok(u32::MAX),));
    func.post_return(&mut store).unwrap();
}

#[test]
fn err_u64_in_result_u32_u64() {
    // mk-err(e: u64) -> result<u32, u64>  { Err(e) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_err".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-err".into()),
                params: vec![("e".into(), "u64".into())],
                result: Some("res_u32_u64".into()),
                body: Some(serialize_body(&[Instruction::Err {
                    value: Box::new(Instruction::LocalGet { uid: "e".into() }),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "res_u32_u64".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("res_u32_u64".into()),
                definition: WitType::Result("u32".into(), "u64".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u64,), (Result<u32, u64>,)>(&mut store, "mk-err")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (1_000_000_000_000u64,)).unwrap(),
        (Err(1_000_000_000_000u64),)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (u64::MAX,)).unwrap(),
        (Err(u64::MAX),)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// variant kind { small(u32), big(u64), unit } — three cases with mixed widths.
// Constructing each one round-trips through the variant's payload region at
// offset 8 (max of {align(u32)=4, align(u64)=8, no-payload=1}).
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(variant)]
enum Kind {
    #[component(name = "small")]
    Small(u32),
    #[component(name = "big")]
    Big(u64),
    #[component(name = "unit")]
    Unit,
}

#[test]
fn variant_hetero_construct_all_cases() {
    // mk(which: u32) -> kind  — but variants need their last instruction to
    // be a literal VariantCtor, no branching. Run three distinct funcs:
    // mk-small(x: u32) → small(x); mk-big(x: u64) → big(x); mk-unit() → unit.
    let db = WastDb {
        funcs: vec![
            WastFuncRow {
                uid: "mk_small".into(),
                func: WastFunc {
                    source: FuncSource::Exported("mk-small".into()),
                    params: vec![("x".into(), "u32".into())],
                    result: Some("kind".into()),
                    body: Some(serialize_body(&[Instruction::VariantCtor {
                        case: "small".into(),
                        value: Some(Box::new(Instruction::LocalGet { uid: "x".into() })),
                    }])),
                },
            },
            WastFuncRow {
                uid: "mk_big".into(),
                func: WastFunc {
                    source: FuncSource::Exported("mk-big".into()),
                    params: vec![("x".into(), "u64".into())],
                    result: Some("kind".into()),
                    body: Some(serialize_body(&[Instruction::VariantCtor {
                        case: "big".into(),
                        value: Some(Box::new(Instruction::LocalGet { uid: "x".into() })),
                    }])),
                },
            },
            WastFuncRow {
                uid: "mk_unit".into(),
                func: WastFunc {
                    source: FuncSource::Exported("mk-unit".into()),
                    params: vec![],
                    result: Some("kind".into()),
                    body: Some(serialize_body(&[Instruction::VariantCtor {
                        case: "unit".into(),
                        value: None,
                    }])),
                },
            },
        ],
        types: vec![WastTypeRow {
            uid: "kind".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("kind".into()),
                definition: WitType::Variant(vec![
                    ("small".into(), Some("u32".into())),
                    ("big".into(), Some("u64".into())),
                    ("unit".into(), None),
                ]),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();

    let mk_small = instance
        .get_typed_func::<(u32,), (Kind,)>(&mut store, "mk-small")
        .unwrap();
    assert_eq!(
        mk_small.call(&mut store, (42u32,)).unwrap(),
        (Kind::Small(42),)
    );
    mk_small.post_return(&mut store).unwrap();
    assert_eq!(
        mk_small.call(&mut store, (u32::MAX,)).unwrap(),
        (Kind::Small(u32::MAX),)
    );
    mk_small.post_return(&mut store).unwrap();

    let mk_big = instance
        .get_typed_func::<(u64,), (Kind,)>(&mut store, "mk-big")
        .unwrap();
    assert_eq!(
        mk_big.call(&mut store, (u64::MAX,)).unwrap(),
        (Kind::Big(u64::MAX),)
    );
    mk_big.post_return(&mut store).unwrap();

    let mk_unit = instance
        .get_typed_func::<(), (Kind,)>(&mut store, "mk-unit")
        .unwrap();
    assert_eq!(mk_unit.call(&mut store, ()).unwrap(), (Kind::Unit,));
    mk_unit.post_return(&mut store).unwrap();
}
