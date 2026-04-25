//! v0.33: heterogeneous variant/result `LocalGet` copy where the joined
//! flat payload has multiple uniform-width slots — e.g. `result<string, u32>`
//! whose join is `[i32, i32]`. Each slot is unconditionally copied at its
//! position; cases that use fewer slots leave junk in trailing bytes, but
//! those bytes lie outside the case's own size_align footprint so its
//! reader (which consults the disc first) never observes them.

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

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Status {
    #[component(name = "outcome")]
    outcome: Result<String, u32>,
    #[component(name = "retries")]
    retries: u32,
}

#[test]
fn record_with_result_string_u32_field_from_param() {
    // wrap(r: result<string, u32>, n: u32) -> status  { { outcome: r, retries: n } }
    //
    // result<string, u32>: ok = string ([ptr, len], 2 slots i32),
    //                      err = u32 (1 slot i32). joined payload [i32, i32].
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![
                    ("r".into(), "res_string_u32".into()),
                    ("n".into(), "u32".into()),
                ],
                result: Some("status".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("outcome".into(), Instruction::LocalGet { uid: "r".into() }),
                        ("retries".into(), Instruction::LocalGet { uid: "n".into() }),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "res_string_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("res_string_u32".into()),
                    definition: WitType::Result("string".into(), "u32".into()),
                },
            },
            WastTypeRow {
                uid: "status".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("status".into()),
                    definition: WitType::Record(vec![
                        ("outcome".into(), "res_string_u32".into()),
                        ("retries".into(), "u32".into()),
                    ]),
                },
            },
        ],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Result<&str, u32>, u32), (Status,)>(&mut store, "wrap")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Ok("hello"), 1)).unwrap(),
        (Status {
            outcome: Ok("hello".into()),
            retries: 1,
        },)
    );
    func.post_return(&mut store).unwrap();

    assert_eq!(
        func.call(&mut store, (Err(404), 5)).unwrap(),
        (Status {
            outcome: Err(404),
            retries: 5,
        },)
    );
    func.post_return(&mut store).unwrap();

    // Empty string keeps the (ptr, len=0) shape — both slots end up with
    // valid i32s.
    assert_eq!(
        func.call(&mut store, (Ok(""), 0)).unwrap(),
        (Status {
            outcome: Ok("".into()),
            retries: 0,
        },)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// option<list<u32>> as a record field — exercises the recursive-into-inner
// path for option<T> from v0.26 PLUS the underlying inner copy that now
// allows the multi-slot uniform shape.
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Bag {
    #[component(name = "data")]
    data: Option<Vec<u32>>,
    #[component(name = "tag")]
    tag: u32,
}

#[test]
fn record_with_option_list_field_via_multi_slot_path() {
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![("o".into(), "opt_list".into()), ("t".into(), "u32".into())],
                result: Some("bag".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("data".into(), Instruction::LocalGet { uid: "o".into() }),
                        ("tag".into(), Instruction::LocalGet { uid: "t".into() }),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "list_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("list_u32".into()),
                    definition: WitType::List("u32".into()),
                },
            },
            WastTypeRow {
                uid: "opt_list".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_list".into()),
                    definition: WitType::Option("list_u32".into()),
                },
            },
            WastTypeRow {
                uid: "bag".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("bag".into()),
                    definition: WitType::Record(vec![
                        ("data".into(), "opt_list".into()),
                        ("tag".into(), "u32".into()),
                    ]),
                },
            },
        ],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Option<&[u32]>, u32), (Bag,)>(&mut store, "wrap")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Some(&[1u32, 2, 3][..]), 9)).unwrap(),
        (Bag {
            data: Some(vec![1, 2, 3]),
            tag: 9,
        },)
    );
    func.post_return(&mut store).unwrap();

    assert_eq!(
        func.call(&mut store, (None, 0)).unwrap(),
        (Bag { data: None, tag: 0 },)
    );
    func.post_return(&mut store).unwrap();
}
