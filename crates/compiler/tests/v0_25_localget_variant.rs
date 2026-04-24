//! v0.25: `LocalGet` of an Option/Result/Variant local as a field value.
//! Extends v0.24 (which covered Record/Tuple/String/List/Enum/Flags/Handle
//! sources) with the disc+payload compounds. Payload write uses the
//! flat-joined core type's store op at the max-aligned byte offset.

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
// record with option<u32> field, sourced from an option<u32> param
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct OptRec {
    #[component(name = "flag")]
    flag: Option<u32>,
    #[component(name = "count")]
    count: u32,
}

#[test]
fn record_with_option_field_from_param() {
    // wrap(o: option<u32>, c: u32) -> opt_rec  { { flag: o, count: c } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![("o".into(), "opt_u32".into()), ("c".into(), "u32".into())],
                result: Some("opt_rec".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("flag".into(), Instruction::LocalGet { uid: "o".into() }),
                        ("count".into(), Instruction::LocalGet { uid: "c".into() }),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "opt_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_u32".into()),
                    definition: WitType::Option("u32".into()),
                },
            },
            WastTypeRow {
                uid: "opt_rec".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_rec".into()),
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
    let func = instance
        .get_typed_func::<(Option<u32>, u32), (OptRec,)>(&mut store, "wrap")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Some(42), 3)).unwrap(),
        (OptRec {
            flag: Some(42),
            count: 3
        },)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (None, 7)).unwrap(),
        (OptRec {
            flag: None,
            count: 7
        },)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// record with result<u32, u32> field, sourced from a result param
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct ResRec {
    #[component(name = "status")]
    status: Result<u32, u32>,
    #[component(name = "retries")]
    retries: u32,
}

#[test]
fn record_with_result_field_from_param() {
    // wrap(r: result<u32, u32>, n: u32) -> res_rec  { { status: r, retries: n } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![("r".into(), "res_u32".into()), ("n".into(), "u32".into())],
                result: Some("res_rec".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("status".into(), Instruction::LocalGet { uid: "r".into() }),
                        ("retries".into(), Instruction::LocalGet { uid: "n".into() }),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "res_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("res_u32".into()),
                    definition: WitType::Result("u32".into(), "u32".into()),
                },
            },
            WastTypeRow {
                uid: "res_rec".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("res_rec".into()),
                    definition: WitType::Record(vec![
                        ("status".into(), "res_u32".into()),
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
        .get_typed_func::<(Result<u32, u32>, u32), (ResRec,)>(&mut store, "wrap")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Ok(42), 1)).unwrap(),
        (ResRec {
            status: Ok(42),
            retries: 1
        },)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Err(9), 5)).unwrap(),
        (ResRec {
            status: Err(9),
            retries: 5
        },)
    );
    func.post_return(&mut store).unwrap();
}

// ---------------------------------------------------------------------------
// tuple<shape, u32> where shape is a variant built from a param
// ---------------------------------------------------------------------------

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(variant)]
enum Shape {
    #[component(name = "circle")]
    Circle(u32),
    #[component(name = "square")]
    Square(u32),
    #[component(name = "unit")]
    Unit,
}

#[test]
fn tuple_with_variant_field_from_param() {
    // pair(s: shape, n: u32) -> tuple<shape, u32>  { (s, n) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "pair".into(),
            func: WastFunc {
                source: FuncSource::Exported("pair".into()),
                params: vec![("s".into(), "shape".into()), ("n".into(), "u32".into())],
                result: Some("shape_and_u32".into()),
                body: Some(serialize_body(&[Instruction::TupleLiteral {
                    values: vec![
                        Instruction::LocalGet { uid: "s".into() },
                        Instruction::LocalGet { uid: "n".into() },
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "shape".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("shape".into()),
                    definition: WitType::Variant(vec![
                        ("circle".into(), Some("u32".into())),
                        ("square".into(), Some("u32".into())),
                        ("unit".into(), None),
                    ]),
                },
            },
            WastTypeRow {
                uid: "shape_and_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("shape_and_u32".into()),
                    definition: WitType::Tuple(vec!["shape".into(), "u32".into()]),
                },
            },
        ],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Shape, u32), ((Shape, u32),)>(&mut store, "pair")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Shape::Circle(7), 99)).unwrap(),
        ((Shape::Circle(7), 99),)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Shape::Square(11), 0)).unwrap(),
        ((Shape::Square(11), 0),)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Shape::Unit, 5)).unwrap(),
        ((Shape::Unit, 5),)
    );
    func.post_return(&mut store).unwrap();
}
