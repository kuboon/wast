//! v0.17 test: general `variant` type — N cases with optional payload.
//! `option<T>` and `result<T,E>` are specializations and stay routed through
//! their dedicated IR nodes. This test exercises the generic case via a
//! three-case variant.

use wasmtime::component::{Component, ComponentType, Lift, Linker, Lower};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, MatchArm, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

// Rust-side mirror of `variant shape { circle(u32), square(u32), unit }`.
// Payload-bearing cases carry u32; `unit` is a payload-less case.
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

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

fn shape_type_row() -> WastTypeRow {
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
    }
}

#[test]
fn variant_ctor_circle() {
    // mk-circle(r: u32) -> shape  { circle(r) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_circle".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-circle".into()),
                params: vec![("r".into(), "u32".into())],
                result: Some("shape".into()),
                body: Some(serialize_body(&[Instruction::VariantCtor {
                    case: "circle".into(),
                    value: Some(Box::new(Instruction::LocalGet { uid: "r".into() })),
                }])),
            },
        }],
        types: vec![shape_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (Shape,)>(&mut store, "mk-circle")
        .unwrap();
    assert_eq!(func.call(&mut store, (7,)).unwrap(), (Shape::Circle(7),));
    func.post_return(&mut store).unwrap();
}

#[test]
fn variant_ctor_payloadless_unit() {
    // mk-unit() -> shape  { unit }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_unit".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-unit".into()),
                params: vec![],
                result: Some("shape".into()),
                body: Some(serialize_body(&[Instruction::VariantCtor {
                    case: "unit".into(),
                    value: None,
                }])),
            },
        }],
        types: vec![shape_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Shape,)>(&mut store, "mk-unit")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), (Shape::Unit,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn match_variant_dispatch() {
    // describe(s: shape) -> u32
    //   match s { circle(r) => r * 2, square(r) => r * 4, unit => 0 }
    //
    // Arithmetic in arm bodies exercises binding use; different "area
    // proxies" for each case verify the dispatch selects the right arm.
    use wast_pattern_analyzer::ArithOp;
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "describe".into(),
            func: WastFunc {
                source: FuncSource::Exported("describe".into()),
                params: vec![("s".into(), "shape".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::MatchVariant {
                    value: Box::new(Instruction::LocalGet { uid: "s".into() }),
                    arms: vec![
                        MatchArm {
                            case: "circle".into(),
                            binding: Some("r".into()),
                            body: vec![Instruction::Arithmetic {
                                op: ArithOp::Mul,
                                lhs: Box::new(Instruction::LocalGet { uid: "r".into() }),
                                rhs: Box::new(Instruction::Const { value: 2 }),
                            }],
                        },
                        MatchArm {
                            case: "square".into(),
                            binding: Some("r".into()),
                            body: vec![Instruction::Arithmetic {
                                op: ArithOp::Mul,
                                lhs: Box::new(Instruction::LocalGet { uid: "r".into() }),
                                rhs: Box::new(Instruction::Const { value: 4 }),
                            }],
                        },
                        MatchArm {
                            case: "unit".into(),
                            binding: None,
                            body: vec![Instruction::Const { value: 0 }],
                        },
                    ],
                }])),
            },
        }],
        types: vec![shape_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Shape,), (u32,)>(&mut store, "describe")
        .unwrap();

    assert_eq!(func.call(&mut store, (Shape::Circle(5),)).unwrap(), (10,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Shape::Square(5),)).unwrap(), (20,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Shape::Unit,)).unwrap(), (0,));
    func.post_return(&mut store).unwrap();
}
