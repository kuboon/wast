//! v0.21: `ListLiteral { values }` — runtime list construction. Tests cover
//! empty list, small list of u32, list of i64, list of strings (nested
//! compound path), and list<u32> as a record field.

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

fn list_u32_type() -> WastTypeRow {
    WastTypeRow {
        uid: "list_u32".into(),
        def: WastTypeDef {
            source: TypeSource::Internal("list_u32".into()),
            definition: WitType::List("u32".into()),
        },
    }
}

#[test]
fn list_literal_empty() {
    // make-empty() -> list<u32>  { ListLiteral { values: [] } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_empty".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-empty".into()),
                params: vec![],
                result: Some("list_u32".into()),
                body: Some(serialize_body(&[Instruction::ListLiteral {
                    values: vec![],
                }])),
            },
        }],
        types: vec![list_u32_type()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Vec<u32>,)>(&mut store, "make-empty")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), (vec![],));
    func.post_return(&mut store).unwrap();
}

#[test]
fn list_literal_u32_const() {
    // make-nums() -> list<u32>  { [1, 2, 3, 42] }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_nums".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-nums".into()),
                params: vec![],
                result: Some("list_u32".into()),
                body: Some(serialize_body(&[Instruction::ListLiteral {
                    values: vec![
                        Instruction::Const { value: 1 },
                        Instruction::Const { value: 2 },
                        Instruction::Const { value: 3 },
                        Instruction::Const { value: 42 },
                    ],
                }])),
            },
        }],
        types: vec![list_u32_type()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Vec<u32>,)>(&mut store, "make-nums")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), (vec![1, 2, 3, 42],));
    func.post_return(&mut store).unwrap();
}

#[test]
fn list_literal_u32_from_params() {
    // pack(a: u32, b: u32, c: u32) -> list<u32>  { [a, b, c] }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "pack".into(),
            func: WastFunc {
                source: FuncSource::Exported("pack".into()),
                params: vec![
                    ("a".into(), "u32".into()),
                    ("b".into(), "u32".into()),
                    ("c".into(), "u32".into()),
                ],
                result: Some("list_u32".into()),
                body: Some(serialize_body(&[Instruction::ListLiteral {
                    values: vec![
                        Instruction::LocalGet { uid: "a".into() },
                        Instruction::LocalGet { uid: "b".into() },
                        Instruction::LocalGet { uid: "c".into() },
                    ],
                }])),
            },
        }],
        types: vec![list_u32_type()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32, u32, u32), (Vec<u32>,)>(&mut store, "pack")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (100, 200, 300)).unwrap(),
        (vec![100, 200, 300],)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn list_literal_i64_8byte_aligned() {
    // big() -> list<i64>  { [-1, 0, MAX] }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "big".into(),
            func: WastFunc {
                source: FuncSource::Exported("big".into()),
                params: vec![],
                result: Some("list_i64".into()),
                body: Some(serialize_body(&[Instruction::ListLiteral {
                    values: vec![
                        Instruction::Const { value: -1 },
                        Instruction::Const { value: 0 },
                        Instruction::Const { value: i64::MAX },
                    ],
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "list_i64".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("list_i64".into()),
                definition: WitType::List("i64".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Vec<i64>,)>(&mut store, "big")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ()).unwrap(),
        (vec![-1i64, 0, i64::MAX],)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn list_literal_of_strings() {
    // greetings() -> list<string>  { ["hello", "world"] }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "greetings".into(),
            func: WastFunc {
                source: FuncSource::Exported("greetings".into()),
                params: vec![],
                result: Some("list_string".into()),
                body: Some(serialize_body(&[Instruction::ListLiteral {
                    values: vec![
                        Instruction::StringLiteral {
                            bytes: b"hello".to_vec(),
                        },
                        Instruction::StringLiteral {
                            bytes: b"world".to_vec(),
                        },
                    ],
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "list_string".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("list_string".into()),
                definition: WitType::List("string".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Vec<String>,)>(&mut store, "greetings")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ()).unwrap(),
        (vec!["hello".to_string(), "world".to_string()],)
    );
    func.post_return(&mut store).unwrap();
}

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Bag {
    #[component(name = "items")]
    items: Vec<u32>,
    #[component(name = "label")]
    label: String,
}

#[test]
fn record_with_list_literal_field() {
    // make-bag(label: string) -> bag  { { items: [7, 8, 9], label: label } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_bag".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-bag".into()),
                params: vec![("label".into(), "string".into())],
                result: Some("bag".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        (
                            "items".into(),
                            Instruction::ListLiteral {
                                values: vec![
                                    Instruction::Const { value: 7 },
                                    Instruction::Const { value: 8 },
                                    Instruction::Const { value: 9 },
                                ],
                            },
                        ),
                        (
                            "label".into(),
                            Instruction::LocalGet {
                                uid: "label".into(),
                            },
                        ),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "bag".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("bag".into()),
                    definition: WitType::Record(vec![
                        ("items".into(), "list_u32".into()),
                        ("label".into(), "string".into()),
                    ]),
                },
            },
            list_u32_type(),
        ],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&str,), (Bag,)>(&mut store, "make-bag")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ("tag",)).unwrap(),
        (Bag {
            items: vec![7, 8, 9],
            label: "tag".to_string(),
        },)
    );
    func.post_return(&mut store).unwrap();
}
