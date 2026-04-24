//! v0.26: `LocalGet` of an `option<T>` local where T is a multi-slot type
//! (string, list, record, tuple). Option's memory layout is fully determined
//! by T, so no runtime disc-branch is needed — we recurse into
//! `emit_copy_from_local` for T at the aligned payload offset.

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
// record { msg: option<string>, count: u32 } from an option<string> param
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct OptStrRec {
    #[component(name = "msg")]
    msg: Option<String>,
    #[component(name = "count")]
    count: u32,
}

#[test]
fn record_with_option_string_field_from_param() {
    // wrap(o: option<string>, c: u32) -> r  { { msg: o, count: c } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![
                    ("o".into(), "opt_string".into()),
                    ("c".into(), "u32".into()),
                ],
                result: Some("opt_str_rec".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("msg".into(), Instruction::LocalGet { uid: "o".into() }),
                        ("count".into(), Instruction::LocalGet { uid: "c".into() }),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "opt_string".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_string".into()),
                    definition: WitType::Option("string".into()),
                },
            },
            WastTypeRow {
                uid: "opt_str_rec".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_str_rec".into()),
                    definition: WitType::Record(vec![
                        ("msg".into(), "opt_string".into()),
                        ("count".into(), "u32".into()),
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
        .get_typed_func::<(Option<&str>, u32), (OptStrRec,)>(&mut store, "wrap")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Some("hello"), 3)).unwrap(),
        (OptStrRec {
            msg: Some("hello".into()),
            count: 3,
        },)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (None, 7)).unwrap(),
        (OptStrRec {
            msg: None,
            count: 7,
        },)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Some(""), 1)).unwrap(),
        (OptStrRec {
            msg: Some("".into()),
            count: 1,
        },)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// record { items: option<list<u32>>, label: u32 } from an option<list> param
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct OptListRec {
    #[component(name = "items")]
    items: Option<Vec<u32>>,
    #[component(name = "label")]
    label: u32,
}

#[test]
fn record_with_option_list_field_from_param() {
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap_list".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap-list".into()),
                params: vec![("o".into(), "opt_list".into()), ("n".into(), "u32".into())],
                result: Some("opt_list_rec".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("items".into(), Instruction::LocalGet { uid: "o".into() }),
                        ("label".into(), Instruction::LocalGet { uid: "n".into() }),
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
                uid: "opt_list_rec".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_list_rec".into()),
                    definition: WitType::Record(vec![
                        ("items".into(), "opt_list".into()),
                        ("label".into(), "u32".into()),
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
        .get_typed_func::<(Option<&[u32]>, u32), (OptListRec,)>(&mut store, "wrap-list")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Some(&[1, 2, 3][..]), 10)).unwrap(),
        (OptListRec {
            items: Some(vec![1, 2, 3]),
            label: 10,
        },)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (None, 20)).unwrap(),
        (OptListRec {
            items: None,
            label: 20,
        },)
    );
    func.post_return(&mut store).unwrap();
}
