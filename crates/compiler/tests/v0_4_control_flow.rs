//! v0.4 test: control flow (If/Block/Loop/Br/BrIf) and `LocalSet`.
//! `LocalSet` to an unseen uid introduces a new local; locals live at
//! function scope.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction, serialize_body};
use wast_types::{FuncSource, WastDb, WastFunc, WastFuncRow};

fn compile_component(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

fn single(
    name: &str,
    params: Vec<(&str, &str)>,
    result: Option<&str>,
    body: Vec<Instruction>,
) -> WastDb {
    WastDb {
        funcs: vec![WastFuncRow {
            uid: name.into(),
            func: WastFunc {
                source: FuncSource::Exported(name.into()),
                params: params
                    .into_iter()
                    .map(|(n, t)| (n.into(), t.into()))
                    .collect(),
                result: result.map(|s| s.into()),
                body: Some(serialize_body(&body)),
            },
        }],
        types: vec![],
    }
}

fn local_get(uid: &str) -> Instruction {
    Instruction::LocalGet { uid: uid.into() }
}

#[test]
fn max_via_if_else() {
    // max(a, b: u32) -> u32  {  if a > b { a } else { b }  }
    let db = single(
        "max",
        vec![("a", "u32"), ("b", "u32")],
        Some("u32"),
        vec![Instruction::If {
            condition: Box::new(Instruction::Compare {
                op: CompareOp::Gt,
                lhs: Box::new(local_get("a")),
                rhs: Box::new(local_get("b")),
            }),
            then_body: vec![local_get("a")],
            else_body: vec![local_get("b")],
        }],
    );
    let (engine, component) = compile_component(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32, u32), (u32,)>(&mut store, "max")
        .unwrap();
    assert_eq!(func.call(&mut store, (3, 9)).unwrap(), (9,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (42, 17)).unwrap(), (42,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (5, 5)).unwrap(), (5,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn sum_1_to_n_via_loop() {
    // sum_1_to_n(n: u32) -> u32  {  acc = 0; while n != 0 { acc += n; n -= 1 }; acc  }
    // Uses a new local `acc` seeded via Arithmetic on n so its type resolves to u32
    // (bare Const would default to i32 and break the core type of the accumulator).
    let db = single(
        "sum",
        vec![("n", "u32")],
        Some("u32"),
        vec![
            // acc = n - n  →  0 (of type u32)
            Instruction::LocalSet {
                uid: "acc".into(),
                value: Box::new(Instruction::Arithmetic {
                    op: ArithOp::Sub,
                    lhs: Box::new(local_get("n")),
                    rhs: Box::new(local_get("n")),
                }),
            },
            // block $done { loop $body { br_if $done (n == 0); acc += n; n -= 1; br $body } }
            Instruction::Block {
                label: Some("done".into()),
                body: vec![Instruction::Loop {
                    label: Some("body".into()),
                    body: vec![
                        Instruction::BrIf {
                            label: "done".into(),
                            condition: Box::new(Instruction::Compare {
                                op: CompareOp::Eq,
                                lhs: Box::new(local_get("n")),
                                rhs: Box::new(Instruction::Const { value: 0 }),
                            }),
                        },
                        Instruction::LocalSet {
                            uid: "acc".into(),
                            value: Box::new(Instruction::Arithmetic {
                                op: ArithOp::Add,
                                lhs: Box::new(local_get("acc")),
                                rhs: Box::new(local_get("n")),
                            }),
                        },
                        Instruction::LocalSet {
                            uid: "n".into(),
                            value: Box::new(Instruction::Arithmetic {
                                op: ArithOp::Sub,
                                lhs: Box::new(local_get("n")),
                                rhs: Box::new(Instruction::Const { value: 1 }),
                            }),
                        },
                        Instruction::Br {
                            label: "body".into(),
                        },
                    ],
                }],
            },
            local_get("acc"),
        ],
    );
    let (engine, component) = compile_component(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "sum")
        .unwrap();
    assert_eq!(func.call(&mut store, (10,)).unwrap(), (55,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (0,)).unwrap(), (0,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (100,)).unwrap(), (5050,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn classify_nested_if() {
    // classify(x: i32) -> i32  {  if x < 0 { -1 } else if x > 0 { 1 } else { 0 }  }
    let db = single(
        "classify",
        vec![("x", "i32")],
        Some("i32"),
        vec![Instruction::If {
            condition: Box::new(Instruction::Compare {
                op: CompareOp::Lt,
                lhs: Box::new(local_get("x")),
                rhs: Box::new(Instruction::Const { value: 0 }),
            }),
            then_body: vec![Instruction::Const { value: -1 }],
            else_body: vec![Instruction::If {
                condition: Box::new(Instruction::Compare {
                    op: CompareOp::Gt,
                    lhs: Box::new(local_get("x")),
                    rhs: Box::new(Instruction::Const { value: 0 }),
                }),
                then_body: vec![Instruction::Const { value: 1 }],
                else_body: vec![Instruction::Const { value: 0 }],
            }],
        }],
    );
    let (engine, component) = compile_component(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(i32,), (i32,)>(&mut store, "classify")
        .unwrap();
    assert_eq!(func.call(&mut store, (-5,)).unwrap(), (-1,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (0,)).unwrap(), (0,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (7,)).unwrap(), (1,));
    func.post_return(&mut store).unwrap();
}
