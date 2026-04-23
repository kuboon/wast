//! v0.16 test: `record` type with primitive fields.
//! Record flat form is the concatenation of its fields' flat forms.
//! Record return uses indirect return: RecordLiteral writes each field at
//! its Canonical-ABI byte offset within the return buffer.

use wasmtime::component::{Component, ComponentNamedList, ComponentType, Lift, Linker, Lower};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

// Rust-side mirror of `record point { x: u32, y: u32 }` so typed_func can
// marshal parameters and results. ComponentType/Lower/Lift let wasmtime
// translate to/from the Canonical-ABI layout.
#[derive(ComponentType, Lower, Lift, Clone, Copy, Debug, PartialEq)]
#[component(record)]
struct Point {
    #[component(name = "x")]
    x: u32,
    #[component(name = "y")]
    y: u32,
}

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

fn point_type_row() -> WastTypeRow {
    WastTypeRow {
        uid: "point".into(),
        def: WastTypeDef {
            source: TypeSource::Internal("point".into()),
            definition: WitType::Record(vec![
                ("x".into(), "u32".into()),
                ("y".into(), "u32".into()),
            ]),
        },
    }
}

#[test]
fn record_get_field_x() {
    // get-x(p: point) -> u32  { p.x }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "get_x".into(),
            func: WastFunc {
                source: FuncSource::Exported("get-x".into()),
                params: vec![("p".into(), "point".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::RecordGet {
                    value: Box::new(Instruction::LocalGet { uid: "p".into() }),
                    field: "x".into(),
                }])),
            },
        }],
        types: vec![point_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Point,), (u32,)>(&mut store, "get-x")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (Point { x: 42, y: 7 },)).unwrap(),
        (42,)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Point { x: 100, y: 200 },)).unwrap(),
        (100,)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn record_get_field_y() {
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "get_y".into(),
            func: WastFunc {
                source: FuncSource::Exported("get-y".into()),
                params: vec![("p".into(), "point".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::RecordGet {
                    value: Box::new(Instruction::LocalGet { uid: "p".into() }),
                    field: "y".into(),
                }])),
            },
        }],
        types: vec![point_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Point,), (u32,)>(&mut store, "get-y")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (Point { x: 1, y: 99 },)).unwrap(),
        (99,)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn record_construct_and_return() {
    // make-point(x: u32, y: u32) -> point  { { x: x, y: y } }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_point".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-point".into()),
                params: vec![("x".into(), "u32".into()), ("y".into(), "u32".into())],
                result: Some("point".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("x".into(), Instruction::LocalGet { uid: "x".into() }),
                        ("y".into(), Instruction::LocalGet { uid: "y".into() }),
                    ],
                }])),
            },
        }],
        types: vec![point_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32, u32), (Point,)>(&mut store, "make-point")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (11, 22)).unwrap(),
        (Point { x: 11, y: 22 },)
    );
    func.post_return(&mut store).unwrap();
}

// Record with heterogeneous alignments (u8-bool/u32/u64) so field offsets
// include non-trivial padding — exercises the layout algorithm.
#[derive(ComponentType, Lower, Lift, Clone, Copy, Debug, PartialEq)]
#[component(record)]
struct Mixed {
    #[component(name = "flag")]
    flag: bool,
    #[component(name = "big")]
    big: u64,
    #[component(name = "small")]
    small: u32,
}

#[test]
fn record_mixed_alignment_return() {
    // make-mixed(flag: bool, big: u64, small: u32) -> mixed
    // Layout: flag at 0 (1 byte) + 7 pad → big at 8 (8 bytes) → small at 16
    // (4 bytes), total 24 bytes padded to 8-byte alignment.
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "make_mixed".into(),
            func: WastFunc {
                source: FuncSource::Exported("make-mixed".into()),
                params: vec![
                    ("flag".into(), "bool".into()),
                    ("big".into(), "u64".into()),
                    ("small".into(), "u32".into()),
                ],
                result: Some("mixed".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("flag".into(), Instruction::LocalGet { uid: "flag".into() }),
                        ("big".into(), Instruction::LocalGet { uid: "big".into() }),
                        (
                            "small".into(),
                            Instruction::LocalGet {
                                uid: "small".into(),
                            },
                        ),
                    ],
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "mixed".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("mixed".into()),
                definition: WitType::Record(vec![
                    ("flag".into(), "bool".into()),
                    ("big".into(), "u64".into()),
                    ("small".into(), "u32".into()),
                ]),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(bool, u64, u32), (Mixed,)>(&mut store, "make-mixed")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (true, u64::MAX, 42)).unwrap(),
        (Mixed {
            flag: true,
            big: u64::MAX,
            small: 42
        },)
    );
    func.post_return(&mut store).unwrap();
}

// Ensure the unused `ComponentNamedList` import doesn't break — wasmtime's
// typed_func requires ComponentNamedList to be in scope for named-tuple
// lifting.
#[allow(dead_code)]
fn _check_component_named_list_in_scope<T: ComponentNamedList>(_: T) {}
