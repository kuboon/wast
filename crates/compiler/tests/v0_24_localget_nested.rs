//! v0.24: `LocalGet` of a compound-typed local used as a field value. Until
//! now a record/tuple field only accepted literal constructors
//! (`RecordLiteral`, `TupleLiteral`). Now we can also do
//! `record pair { a: point, b: point }` + `{ a: p1, b: p2 }` where p1/p2 are
//! params of type `point`. The compiler copies each flat slot directly to
//! memory at the Canonical-ABI offset.

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
// pair { a: point, b: point } from two point params
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
struct Pair {
    #[component(name = "a")]
    a: Point,
    #[component(name = "b")]
    b: Point,
}

#[test]
fn record_field_from_record_param() {
    // make-pair(p1: point, p2: point) -> pair  { { a: p1, b: p2 } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_pair".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-pair".into()),
                params: vec![("p1".into(), "point".into()), ("p2".into(), "point".into())],
                result: Some("pair".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("a".into(), Instruction::LocalGet { uid: "p1".into() }),
                        ("b".into(), Instruction::LocalGet { uid: "p2".into() }),
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
                uid: "pair".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("pair".into()),
                    definition: WitType::Record(vec![
                        ("a".into(), "point".into()),
                        ("b".into(), "point".into()),
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
        .get_typed_func::<(Point, Point), (Pair,)>(&mut store, "make-pair")
        .unwrap();
    let result = func
        .call(&mut store, (Point { x: 1, y: 2 }, Point { x: 3, y: 4 }))
        .unwrap();
    assert_eq!(
        result,
        (Pair {
            a: Point { x: 1, y: 2 },
            b: Point { x: 3, y: 4 },
        },)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// tuple<point, u32> from a point param + a u32
// ---------------------------------------------------------------------------

#[test]
fn tuple_element_from_record_param() {
    // with-weight(p: point, w: u32) -> tuple<point, u32>  { (p, w) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "with_weight".into(),
            func: WastFunc {
                source: FuncSource::Exported("with-weight".into()),
                params: vec![("p".into(), "point".into()), ("w".into(), "u32".into())],
                result: Some("point_and_weight".into()),
                body: Some(serialize_body(&[Instruction::TupleLiteral {
                    values: vec![
                        Instruction::LocalGet { uid: "p".into() },
                        Instruction::LocalGet { uid: "w".into() },
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
                uid: "point_and_weight".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("point_and_weight".into()),
                    definition: WitType::Tuple(vec!["point".into(), "u32".into()]),
                },
            },
        ],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Point, u32), ((Point, u32),)>(&mut store, "with-weight")
        .unwrap();
    let result = func.call(&mut store, (Point { x: 5, y: 6 }, 42)).unwrap();
    assert_eq!(result, ((Point { x: 5, y: 6 }, 42),));
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// record with string+list fields from param (via LocalGet)
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Inner {
    #[component(name = "label")]
    label: String,
    #[component(name = "items")]
    items: Vec<u32>,
}

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Outer {
    #[component(name = "inner")]
    inner: Inner,
    #[component(name = "count")]
    count: u32,
}

#[test]
fn record_field_with_string_and_list_from_param() {
    // wrap(inner: inner, count: u32) -> outer  { { inner: inner, count: count } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![
                    ("inner".into(), "inner".into()),
                    ("count".into(), "u32".into()),
                ],
                result: Some("outer".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        (
                            "inner".into(),
                            Instruction::LocalGet {
                                uid: "inner".into(),
                            },
                        ),
                        (
                            "count".into(),
                            Instruction::LocalGet {
                                uid: "count".into(),
                            },
                        ),
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
                uid: "inner".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("inner".into()),
                    definition: WitType::Record(vec![
                        ("label".into(), "string".into()),
                        ("items".into(), "list_u32".into()),
                    ]),
                },
            },
            WastTypeRow {
                uid: "outer".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("outer".into()),
                    definition: WitType::Record(vec![
                        ("inner".into(), "inner".into()),
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
        .get_typed_func::<(Inner, u32), (Outer,)>(&mut store, "wrap")
        .unwrap();
    let arg = Inner {
        label: "hello".into(),
        items: vec![7, 8, 9],
    };
    let result = func.call(&mut store, (arg.clone(), 3)).unwrap();
    assert_eq!(
        result,
        (Outer {
            inner: arg,
            count: 3,
        },)
    );
    func.post_return(&mut store).unwrap();
}
