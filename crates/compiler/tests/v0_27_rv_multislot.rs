//! v0.27: `LocalGet` of a `result<T, T>` or homogeneous variant local as a
//! field value. When every case's payload has the same WIT type, the memory
//! layout is uniform and we can recurse into the payload type's copy without
//! a runtime disc branch — same pattern as v0.26 Option.

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
// record { status: result<string, string>, label: u32 } from a result param
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct ResStrRec {
    #[component(name = "status")]
    status: Result<String, String>,
    #[component(name = "label")]
    label: u32,
}

#[test]
fn record_with_result_string_field_from_param() {
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![
                    ("r".into(), "res_string".into()),
                    ("n".into(), "u32".into()),
                ],
                result: Some("res_str_rec".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("status".into(), Instruction::LocalGet { uid: "r".into() }),
                        ("label".into(), Instruction::LocalGet { uid: "n".into() }),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "res_string".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("res_string".into()),
                    definition: WitType::Result("string".into(), "string".into()),
                },
            },
            WastTypeRow {
                uid: "res_str_rec".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("res_str_rec".into()),
                    definition: WitType::Record(vec![
                        ("status".into(), "res_string".into()),
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
        .get_typed_func::<(Result<&str, &str>, u32), (ResStrRec,)>(&mut store, "wrap")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Ok("hello"), 1)).unwrap(),
        (ResStrRec {
            status: Ok("hello".into()),
            label: 1,
        },)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Err("oops"), 7)).unwrap(),
        (ResStrRec {
            status: Err("oops".into()),
            label: 7,
        },)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// variant msg { text(string), empty } — text carries a payload, empty doesn't.
// Homogeneous because the distinct-payload-type set has size 1 (just string).
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(variant)]
enum Msg {
    #[component(name = "text")]
    Text(String),
    #[component(name = "empty")]
    Empty,
}

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Letter {
    #[component(name = "body")]
    body: Msg,
    #[component(name = "recipient")]
    recipient: u32,
}

#[test]
fn record_with_variant_string_payload_from_param() {
    // wrap(m: msg, r: u32) -> letter  { { body: m, recipient: r } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![("m".into(), "msg".into()), ("r".into(), "u32".into())],
                result: Some("letter".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("body".into(), Instruction::LocalGet { uid: "m".into() }),
                        (
                            "recipient".into(),
                            Instruction::LocalGet { uid: "r".into() },
                        ),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "msg".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("msg".into()),
                    definition: WitType::Variant(vec![
                        ("text".into(), Some("string".into())),
                        ("empty".into(), None),
                    ]),
                },
            },
            WastTypeRow {
                uid: "letter".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("letter".into()),
                    definition: WitType::Record(vec![
                        ("body".into(), "msg".into()),
                        ("recipient".into(), "u32".into()),
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
        .get_typed_func::<(Msg, u32), (Letter,)>(&mut store, "wrap")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Msg::Text("hi".into()), 42)).unwrap(),
        (Letter {
            body: Msg::Text("hi".into()),
            recipient: 42,
        },)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Msg::Empty, 0)).unwrap(),
        (Letter {
            body: Msg::Empty,
            recipient: 0,
        },)
    );
    func.post_return(&mut store).unwrap();
}
