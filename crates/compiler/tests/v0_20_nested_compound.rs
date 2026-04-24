//! v0.20: records / tuples / variants whose fields are `string` or
//! `list<T>` (not just primitives). `emit_field_store` now routes these
//! through direct (ptr, len) slot stores instead of requiring a primitive
//! payload.

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
// record greeting { message: string, count: u32 }
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Greeting {
    #[component(name = "message")]
    message: String,
    #[component(name = "count")]
    count: u32,
}

fn greeting_type_row() -> WastTypeRow {
    WastTypeRow {
        uid: "greeting".into(),
        def: WastTypeDef {
            source: TypeSource::Internal("greeting".into()),
            definition: WitType::Record(vec![
                ("message".into(), "string".into()),
                ("count".into(), "u32".into()),
            ]),
        },
    }
}

#[test]
fn record_with_string_field_literal() {
    // make-greeting(n: u32) -> greeting  { { message: "hello", count: n } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_greeting".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-greeting".into()),
                params: vec![("n".into(), "u32".into())],
                result: Some("greeting".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        (
                            "message".into(),
                            Instruction::StringLiteral {
                                bytes: b"hello".to_vec(),
                            },
                        ),
                        ("count".into(), Instruction::LocalGet { uid: "n".into() }),
                    ],
                }])),
            },
        }],
        types: vec![greeting_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (Greeting,)>(&mut store, "make-greeting")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (3,)).unwrap(),
        (Greeting {
            message: "hello".into(),
            count: 3
        },)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn record_with_string_field_from_param() {
    // wrap(msg: string, n: u32) -> greeting  { { message: msg, count: n } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![("msg".into(), "string".into()), ("n".into(), "u32".into())],
                result: Some("greeting".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        (
                            "message".into(),
                            Instruction::LocalGet { uid: "msg".into() },
                        ),
                        ("count".into(), Instruction::LocalGet { uid: "n".into() }),
                    ],
                }])),
            },
        }],
        types: vec![greeting_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&str, u32), (Greeting,)>(&mut store, "wrap")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ("world", 42)).unwrap(),
        (Greeting {
            message: "world".into(),
            count: 42
        },)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// tuple<string, u32>
// ---------------------------------------------------------------------------

#[test]
fn tuple_with_string_element() {
    // labeled(msg: string, n: u32) -> tuple<string, u32>  { (msg, n) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "labeled".into(),
            func: WastFunc {
                source: FuncSource::Exported("labeled".into()),
                params: vec![("msg".into(), "string".into()), ("n".into(), "u32".into())],
                result: Some("pair".into()),
                body: Some(serialize_body(&[Instruction::TupleLiteral {
                    values: vec![
                        Instruction::LocalGet { uid: "msg".into() },
                        Instruction::LocalGet { uid: "n".into() },
                    ],
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "pair".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("pair".into()),
                definition: WitType::Tuple(vec!["string".into(), "u32".into()]),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&str, u32), ((String, u32),)>(&mut store, "labeled")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ("hi", 7)).unwrap(),
        (("hi".to_string(), 7u32),)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// variant with string payload
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(variant)]
enum Msg {
    #[component(name = "text")]
    Text(String),
    #[component(name = "empty")]
    Empty,
}

#[test]
fn variant_with_string_payload() {
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_text".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-text".into()),
                params: vec![("s".into(), "string".into())],
                result: Some("msg".into()),
                body: Some(serialize_body(&[Instruction::VariantCtor {
                    case: "text".into(),
                    value: Some(Box::new(Instruction::LocalGet { uid: "s".into() })),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "msg".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("msg".into()),
                definition: WitType::Variant(vec![
                    ("text".into(), Some("string".into())),
                    ("empty".into(), None),
                ]),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&str,), (Msg,)>(&mut store, "mk-text")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ("hello",)).unwrap(),
        (Msg::Text("hello".to_string()),)
    );
    func.post_return(&mut store).unwrap();
}
