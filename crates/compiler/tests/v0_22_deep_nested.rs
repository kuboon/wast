//! v0.22: deeper nested compound. Fields of record/tuple/variant/list that
//! are themselves compound (record-of-record, option<u32> field, list of
//! records, tuple<record, string>, etc.). All literal-constructed — LocalGet
//! of a nested compound source is deferred to a later milestone.

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
// record-of-record: outer { inner: point, count: u32 }
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Point {
    #[component(name = "x")]
    x: u32,
    #[component(name = "y")]
    y: u32,
}

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Outer {
    #[component(name = "inner")]
    inner: Point,
    #[component(name = "count")]
    count: u32,
}

#[test]
fn record_of_record() {
    // make-outer(a: u32, b: u32, c: u32) -> outer
    //   { inner: { x: a, y: b }, count: c }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_outer".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-outer".into()),
                params: vec![
                    ("a".into(), "u32".into()),
                    ("b".into(), "u32".into()),
                    ("c".into(), "u32".into()),
                ],
                result: Some("outer".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        (
                            "inner".into(),
                            Instruction::RecordLiteral {
                                fields: vec![
                                    ("x".into(), Instruction::LocalGet { uid: "a".into() }),
                                    ("y".into(), Instruction::LocalGet { uid: "b".into() }),
                                ],
                            },
                        ),
                        ("count".into(), Instruction::LocalGet { uid: "c".into() }),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "point".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("point".into()),
                    definition: WitType::Record(vec![
                        ("x".into(), "u32".into()),
                        ("y".into(), "u32".into()),
                    ]),
                },
            },
            WastTypeRow {
                uid: "outer".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("outer".into()),
                    definition: WitType::Record(vec![
                        ("inner".into(), "point".into()),
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
        .get_typed_func::<(u32, u32, u32), (Outer,)>(&mut store, "make-outer")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (3, 4, 7)).unwrap(),
        (Outer {
            inner: Point { x: 3, y: 4 },
            count: 7,
        },)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// record with option<u32> field
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Maybe {
    #[component(name = "flag")]
    flag: Option<u32>,
    #[component(name = "count")]
    count: u32,
}

#[test]
fn record_with_option_field() {
    // some-case(n: u32) -> maybe  { { flag: Some(n), count: 1 } }
    // none-case()         -> maybe  { { flag: None, count: 2 } }
    let db = WastDb {
        funcs: vec![
            WastFuncRow {
                uid: "some_case".into(),
                func: WastFunc {
                    source: FuncSource::Exported("some-case".into()),
                    params: vec![("n".into(), "u32".into())],
                    result: Some("maybe".into()),
                    body: Some(serialize_body(&[Instruction::RecordLiteral {
                        fields: vec![
                            (
                                "flag".into(),
                                Instruction::Some {
                                    value: Box::new(Instruction::LocalGet { uid: "n".into() }),
                                },
                            ),
                            ("count".into(), Instruction::Const { value: 1 }),
                        ],
                    }])),
                },
            },
            WastFuncRow {
                uid: "none_case".into(),
                func: WastFunc {
                    source: FuncSource::Exported("none-case".into()),
                    params: vec![],
                    result: Some("maybe".into()),
                    body: Some(serialize_body(&[Instruction::RecordLiteral {
                        fields: vec![
                            ("flag".into(), Instruction::None),
                            ("count".into(), Instruction::Const { value: 2 }),
                        ],
                    }])),
                },
            },
        ],
        types: vec![
            WastTypeRow {
                uid: "opt_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_u32".into()),
                    definition: WitType::Option("u32".into()),
                },
            },
            WastTypeRow {
                uid: "maybe".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("maybe".into()),
                    definition: WitType::Record(vec![
                        ("flag".into(), "opt_u32".into()),
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

    let some_func = instance
        .get_typed_func::<(u32,), (Maybe,)>(&mut store, "some-case")
        .unwrap();
    assert_eq!(
        some_func.call(&mut store, (42,)).unwrap(),
        (Maybe {
            flag: Some(42),
            count: 1
        },)
    );
    some_func.post_return(&mut store).unwrap();

    let none_func = instance
        .get_typed_func::<(), (Maybe,)>(&mut store, "none-case")
        .unwrap();
    assert_eq!(
        none_func.call(&mut store, ()).unwrap(),
        (Maybe {
            flag: None,
            count: 2
        },)
    );
    none_func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// tuple<record, string>
// ---------------------------------------------------------------------------

#[test]
fn tuple_with_record_and_string() {
    // pair() -> tuple<point, string>  { ({x:1, y:2}, "hello") }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "pair".into(),
            func: WastFunc {
                source: FuncSource::Exported("pair".into()),
                params: vec![],
                result: Some("record_and_string".into()),
                body: Some(serialize_body(&[Instruction::TupleLiteral {
                    values: vec![
                        Instruction::RecordLiteral {
                            fields: vec![
                                ("x".into(), Instruction::Const { value: 1 }),
                                ("y".into(), Instruction::Const { value: 2 }),
                            ],
                        },
                        Instruction::StringLiteral {
                            bytes: b"hello".to_vec(),
                        },
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "point".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("point".into()),
                    definition: WitType::Record(vec![
                        ("x".into(), "u32".into()),
                        ("y".into(), "u32".into()),
                    ]),
                },
            },
            WastTypeRow {
                uid: "record_and_string".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("record_and_string".into()),
                    definition: WitType::Tuple(vec!["point".into(), "string".into()]),
                },
            },
        ],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), ((Point, String),)>(&mut store, "pair")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ()).unwrap(),
        ((Point { x: 1, y: 2 }, "hello".to_string()),)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// list<record>
// ---------------------------------------------------------------------------

#[test]
fn list_of_records() {
    // points() -> list<point>  { [{x:1,y:2}, {x:3,y:4}, {x:5,y:6}] }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "points".into(),
            func: WastFunc {
                source: FuncSource::Exported("points".into()),
                params: vec![],
                result: Some("list_point".into()),
                body: Some(serialize_body(&[Instruction::ListLiteral {
                    values: vec![
                        Instruction::RecordLiteral {
                            fields: vec![
                                ("x".into(), Instruction::Const { value: 1 }),
                                ("y".into(), Instruction::Const { value: 2 }),
                            ],
                        },
                        Instruction::RecordLiteral {
                            fields: vec![
                                ("x".into(), Instruction::Const { value: 3 }),
                                ("y".into(), Instruction::Const { value: 4 }),
                            ],
                        },
                        Instruction::RecordLiteral {
                            fields: vec![
                                ("x".into(), Instruction::Const { value: 5 }),
                                ("y".into(), Instruction::Const { value: 6 }),
                            ],
                        },
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "point".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("point".into()),
                    definition: WitType::Record(vec![
                        ("x".into(), "u32".into()),
                        ("y".into(), "u32".into()),
                    ]),
                },
            },
            WastTypeRow {
                uid: "list_point".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("list_point".into()),
                    definition: WitType::List("point".into()),
                },
            },
        ],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Vec<Point>,)>(&mut store, "points")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ()).unwrap(),
        (vec![
            Point { x: 1, y: 2 },
            Point { x: 3, y: 4 },
            Point { x: 5, y: 6 }
        ],)
    );
    func.post_return(&mut store).unwrap();
}
