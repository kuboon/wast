//! v0.18 test: `tuple<T1, T2, …>` — positional anonymous record.
//! Layout matches record's (same byte offsets, concatenated flat slots)
//! but WIT inlines tuple types at each use site instead of requiring a
//! type declaration.

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

fn pair_u32_u32_row() -> WastTypeRow {
    WastTypeRow {
        uid: "pair".into(),
        def: WastTypeDef {
            source: TypeSource::Internal("pair".into()),
            definition: WitType::Tuple(vec!["u32".into(), "u32".into()]),
        },
    }
}

#[test]
fn tuple_first_elem() {
    // first(p: tuple<u32, u32>) -> u32  { p.0 }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "first".into(),
            func: WastFunc {
                source: FuncSource::Exported("first".into()),
                params: vec![("p".into(), "pair".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::TupleGet {
                    value: Box::new(Instruction::LocalGet { uid: "p".into() }),
                    index: 0,
                }])),
            },
        }],
        types: vec![pair_u32_u32_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<((u32, u32),), (u32,)>(&mut store, "first")
        .unwrap();
    assert_eq!(func.call(&mut store, ((42, 7),)).unwrap(), (42,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn tuple_second_elem() {
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "second".into(),
            func: WastFunc {
                source: FuncSource::Exported("second".into()),
                params: vec![("p".into(), "pair".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::TupleGet {
                    value: Box::new(Instruction::LocalGet { uid: "p".into() }),
                    index: 1,
                }])),
            },
        }],
        types: vec![pair_u32_u32_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<((u32, u32),), (u32,)>(&mut store, "second")
        .unwrap();
    assert_eq!(func.call(&mut store, ((10, 99),)).unwrap(), (99,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn tuple_construct_and_return() {
    // make-pair(x: u32, y: u32) -> tuple<u32, u32>  { (x, y) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_pair".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-pair".into()),
                params: vec![("x".into(), "u32".into()), ("y".into(), "u32".into())],
                result: Some("pair".into()),
                body: Some(serialize_body(&[Instruction::TupleLiteral {
                    values: vec![
                        Instruction::LocalGet { uid: "x".into() },
                        Instruction::LocalGet { uid: "y".into() },
                    ],
                }])),
            },
        }],
        types: vec![pair_u32_u32_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32, u32), ((u32, u32),)>(&mut store, "make-pair")
        .unwrap();
    assert_eq!(func.call(&mut store, (11, 22)).unwrap(), ((11, 22),));
    func.post_return(&mut store).unwrap();
}

#[test]
fn tuple_heterogeneous_alignment() {
    // make-triple(flag: bool, big: u64, small: u32) -> tuple<bool, u64, u32>
    // Exercises the layout algorithm with mixed alignments (1 + 7pad + 8 + 4).
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_triple".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-triple".into()),
                params: vec![
                    ("flag".into(), "bool".into()),
                    ("big".into(), "u64".into()),
                    ("small".into(), "u32".into()),
                ],
                result: Some("mixed".into()),
                body: Some(serialize_body(&[Instruction::TupleLiteral {
                    values: vec![
                        Instruction::LocalGet { uid: "flag".into() },
                        Instruction::LocalGet { uid: "big".into() },
                        Instruction::LocalGet {
                            uid: "small".into(),
                        },
                    ],
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "mixed".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("mixed".into()),
                definition: WitType::Tuple(vec!["bool".into(), "u64".into(), "u32".into()]),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(bool, u64, u32), ((bool, u64, u32),)>(&mut store, "make-triple")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (true, u64::MAX, 42)).unwrap(),
        ((true, u64::MAX, 42),)
    );
    func.post_return(&mut store).unwrap();
}
